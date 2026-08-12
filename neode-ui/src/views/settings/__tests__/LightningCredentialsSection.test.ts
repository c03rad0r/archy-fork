import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import LightningCredentialsSection from '../LightningCredentialsSection.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    lndMacaroonStatus: vi.fn(),
    lndRotateMacaroons: vi.fn(),
    lndMacaroonRotationProgress: vi.fn(),
  },
}))

const STEP_KEYS = ['preflight', 'backup', 'stop', 'remove', 'start', 'verify', 'btcpay'] as const

type StepState = 'pending' | 'running' | 'done' | 'failed' | 'skipped'

function steps(overrides: Partial<Record<string, { state: StepState; detail?: string }>> = {}) {
  return STEP_KEYS.map((key) => ({
    key,
    label: `label-${key}`,
    state: overrides[key]?.state ?? ('pending' as StepState),
    detail: overrides[key]?.detail ?? null,
  }))
}

function idleRotation() {
  return {
    running: false,
    ok: null,
    started_at: null,
    finished_at: null,
    error: null,
    steps: steps(),
    backup_path: null,
    identity_pubkey: null,
    channels_before: null,
    channels_after: null,
    new_admin_macaroon_sha256: null,
  }
}

function status(overrides: Record<string, unknown> = {}) {
  return {
    installed: true,
    admin_macaroon_sha256: 'a'.repeat(64),
    issued_at: '2026-08-08 06:03:11',
    identity_pubkey: '024a5fd7de13623aeec81095cf8776fedbc0c4109363022c3ec948196202130b92',
    channels_open: 3,
    channels_pending: 1,
    lnd_error: null,
    btcpay_uses_internal_lnd: true,
    btcpay_credential_current: true,
    rotation: idleRotation(),
    ...overrides,
  }
}

function mountSection() {
  return mount(LightningCredentialsSection, {
    global: { stubs: { Teleport: true } },
  })
}

describe('LightningCredentialsSection', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('shows what must survive before offering to rotate', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status())

    const wrapper = mountSection()
    await flushPromises()

    // The channel census is the reassurance an operator needs before clicking a
    // button that invalidates every credential their wallet holds.
    expect(wrapper.text()).toContain('3 open')
    expect(wrapper.text()).toContain('1 pending')
    expect(wrapper.text()).toContain('2026-08-08 06:03:11')
    // A digest is fine to display; the token itself must never be fetched.
    expect(wrapper.text()).toContain('aaaaaaaaaaaaaaaa…')
    expect(wrapper.find('button').attributes('disabled')).toBeUndefined()
  })

  it('warns when BTCPay is stranded on a rotated-out credential', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({ btcpay_credential_current: false }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('BTCPay Server is holding an old Lightning credential')
  })

  it('stays quiet about BTCPay when there is no internal node to warn about', async () => {
    // null means "not configured" — an absence, not a fault. Reporting it as a
    // problem would train operators to ignore the warning that matters.
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({ btcpay_uses_internal_lnd: false, btcpay_credential_current: null }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).not.toContain('holding an old Lightning credential')
  })

  it('blocks rotation while LND is not answering', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({
        lnd_error: 'LND is not answering on its REST port',
        channels_open: null,
        channels_pending: null,
        identity_pubkey: null,
      }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('Lightning is not answering right now')
    // Without a before-reading there is no way to prove the channels came back,
    // so the button must be unavailable rather than merely discouraged.
    expect(wrapper.find('button').attributes('disabled')).toBeDefined()
  })

  it('says Lightning is not installed instead of offering a no-op rotation', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({ installed: false, admin_macaroon_sha256: null }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('Lightning is not set up on this node yet')
    expect(wrapper.findAll('button')).toHaveLength(0)
  })

  async function startRotation(wrapper: ReturnType<typeof mountSection>, pw = 'node-password') {
    await wrapper.find('button').trigger('click')
    await wrapper.find('input[type="password"]').setValue(pw)
    await wrapper.find('form').trigger('submit')
    await flushPromises()
  }

  it('sends the password and starts polling for progress', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus)
      .mockResolvedValueOnce(status())
      .mockResolvedValue(status({ rotation: { ...idleRotation(), running: true } }))
    vi.mocked(rpcClient.lndRotateMacaroons).mockResolvedValue({ status: 'started' })

    const wrapper = mountSection()
    await flushPromises()
    await startRotation(wrapper)

    expect(rpcClient.lndRotateMacaroons).toHaveBeenCalledWith('node-password')

    const callsBefore = vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length
    vi.advanceTimersByTime(4000)
    await flushPromises()
    expect(vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length).toBeGreaterThan(callsBefore)
  })

  it('keeps polling when the first status after starting has not caught up yet', async () => {
    // The node accepts the rotation and then answers a status request that was
    // computed a moment earlier, still saying `running: false`. Cancelling the
    // poll here would freeze the screen on the one action that most needs to show
    // progress — the operator has just invalidated every credential their wallet
    // holds and would be told nothing is happening.
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status())
    vi.mocked(rpcClient.lndRotateMacaroons).mockResolvedValue({ status: 'started' })

    const wrapper = mountSection()
    await flushPromises()
    await startRotation(wrapper)

    const callsBefore = vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length
    vi.advanceTimersByTime(4000)
    await flushPromises()
    expect(vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length).toBeGreaterThan(callsBefore)
  })

  it('gives up polling if the node never reports the rotation as running', async () => {
    // Bounded, so a request that was accepted but never acted on stops polling
    // instead of hammering the node forever.
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status())
    vi.mocked(rpcClient.lndRotateMacaroons).mockResolvedValue({ status: 'started' })

    const wrapper = mountSection()
    await flushPromises()
    await startRotation(wrapper)

    vi.advanceTimersByTime(180_000)
    await flushPromises()
    const callsAfterGiveUp = vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length
    vi.advanceTimersByTime(30_000)
    await flushPromises()
    expect(vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length).toBe(callsAfterGiveUp)
  })

  it('surfaces a rejected password without starting anything', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status())
    vi.mocked(rpcClient.lndRotateMacaroons).mockRejectedValue(
      new Error('Password verification failed'),
    )

    const wrapper = mountSection()
    await flushPromises()

    await wrapper.find('button').trigger('click')
    await wrapper.find('input[type="password"]').setValue('wrong')
    await wrapper.find('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Password verification failed')
    // The dialog stays open so the operator can correct the password.
    expect(wrapper.find('input[type="password"]').exists()).toBe(true)
  })

  it('reports a finished rotation with the re-pair and backup-cleanup steps', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({
        rotation: {
          ...idleRotation(),
          ok: true,
          finished_at: '2026-08-08T12:00:00Z',
          steps: steps({
            preflight: { state: 'done' },
            backup: { state: 'done' },
            stop: { state: 'done' },
            remove: { state: 'done' },
            start: { state: 'done' },
            verify: { state: 'done', detail: 'same node, same 3 channel(s)' },
            btcpay: { state: 'done' },
          }),
          backup_path: '/var/lib/archipelago/lnd/macaroon-rotation-20260808T120000Z',
          channels_before: 3,
          channels_after: 3,
        },
      }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('Rotation complete')
    expect(wrapper.text()).toContain('same node, same 3 channel(s)')
    expect(wrapper.text()).toContain('Re-pair anything that connects to this node')
    // The backup holds the OLD root key, so telling the operator to delete it is
    // part of the job, not a nicety.
    expect(wrapper.text()).toContain('macaroon-rotation-20260808T120000Z')
  })

  it('reports a failed rotation as failed rather than silently idle', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({
        rotation: {
          ...idleRotation(),
          ok: false,
          finished_at: '2026-08-08T12:00:00Z',
          error: 'backup incomplete — refusing to delete anything',
          steps: steps({ preflight: { state: 'done' }, backup: { state: 'failed' } }),
        },
      }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('Rotation failed')
    expect(wrapper.text()).toContain('backup incomplete')
  })

  it('does not poll the node when nothing is running', async () => {
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status())

    mountSection()
    await flushPromises()

    const callsAfterLoad = vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length
    vi.advanceTimersByTime(30_000)
    await flushPromises()
    expect(vi.mocked(rpcClient.lndMacaroonStatus).mock.calls.length).toBe(callsAfterLoad)
  })

  it('renders inside a card, like every other Settings section', () => {
    // Operator-reported twice: the section rendered as bare text on the
    // Settings page. A new section carries its own wrapper, and nothing about
    // adding it to SystemSection.vue's list reminds you it needs one.
    // `wrapper.element` is not the div: the confirm modal is a second root
    // node, so the component is a fragment. Assert on the first div.
    const wrapper = mountSection()
    expect(wrapper.find('div').classes()).toContain('glass-card')
  })

  it('does not claim Lightning is missing while a rotation is running', async () => {
    // Rotation restarts LND, so `installed` goes false for a moment. The
    // screen used to read that literally and tell the operator "Lightning is
    // not set up on this node yet" — seconds after they rotated, on a node
    // with a working wallet — replacing the progress they were watching.
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(
      status({ installed: false, rotation: { ...idleRotation(), running: true } }),
    )

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).not.toContain('Lightning is not set up on this node yet')
    expect(wrapper.text()).toContain('Lightning is restarting')
  })

  it('still tells a node with no Lightning that there is nothing to rotate', async () => {
    // The other half: the message must survive for its real audience, or the
    // fix above has just hidden a true statement.
    vi.mocked(rpcClient.lndMacaroonStatus).mockResolvedValue(status({ installed: false }))

    const wrapper = mountSection()
    await flushPromises()

    expect(wrapper.text()).toContain('Lightning is not set up on this node yet')
  })

  it('does not claim Lightning is missing in the gap before the node reports the rotation', async () => {
    // The window `awaitUntil` exists for: the rotate RPC has been accepted but
    // the node has not yet reported `running: true`. `installed` can already be
    // false there, so the guard has to cover the await window too, not just
    // `running`.
    vi.mocked(rpcClient.lndMacaroonStatus)
      .mockResolvedValueOnce(status())
      .mockResolvedValue(status({ installed: false }))
    vi.mocked(rpcClient.lndRotateMacaroons).mockResolvedValue(undefined as never)

    const wrapper = mountSection()
    await flushPromises()

    await wrapper.find('button').trigger('click')
    await flushPromises()
    const confirm = wrapper.findAll('button').find((b) => /rotate/i.test(b.text()))
    if (confirm) {
      await confirm.trigger('click')
      await flushPromises()
    }

    expect(wrapper.text()).not.toContain('Lightning is not set up on this node yet')
  })
})
