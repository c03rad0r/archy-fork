import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import ReceiveBitcoinModal from '../ReceiveBitcoinModal.vue'
import EcashSeedBackup from '../EcashSeedBackup.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  useRoute: () => ({ fullPath: '/dashboard' }),
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string, params?: Record<string, unknown>) => (params ? `${key}:${JSON.stringify(params)}` : key) }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

vi.mock('qrcode', () => ({
  toCanvas: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@/composables/useLightningRequired', () => ({
  useLightningRequired: () => ({
    requireLightningReady: vi.fn().mockResolvedValue(true),
    handleLightningFailure: vi.fn().mockReturnValue(false),
  }),
}))

beforeEach(() => {
  vi.useRealTimers()
  vi.clearAllMocks()
  document.body.innerHTML = ''
})

// Guards an operator report (2026-09-08): clicking the Ecash tab appeared to
// close the whole Receive modal. Not reproduced here — the tab switch alone
// (success or failure of wallet.ecash-lnaddress) never emits `close` or
// unmounts the dialog — but the RPC-eager tab switch is exactly the kind of
// path a future change could regress, so it's worth pinning down.
describe('ReceiveBitcoinModal — ecash tab click', () => {
  it('offers authenticated setup for an unseeded wallet and retries the address after setup', async () => {
    let active = false
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'wallet.ecash-lnaddress') {
        if (!active) throw new Error('The ecash wallet has no seed yet')
        return { address: 'someone@minibits.cash' } as never
      }
      if (method === 'wallet.ecash-seed-status') {
        return { active, can_activate: true, derivable_from_node_seed: true, source: null } as never
      }
      return {} as never
    })
    const wrapper = mount(ReceiveBitcoinModal, { props: { show: true }, attachTo: document.body })
    const tab = Array.from(document.body.querySelectorAll('button')).find(b => b.textContent?.toLowerCase().includes('ecash'))!
    tab.click()
    await flushPromises()
    expect(document.body.textContent).toContain('Set up your Cashu Lightning address')
    expect(document.body.textContent).not.toContain('receiveBitcoin.lnAddressUnavailable')
    expect(vi.mocked(rpcClient.call).mock.calls.some(([r]) => r.method === 'wallet.ecash-seed-reveal')).toBe(false)
    active = true
    wrapper.findComponent(EcashSeedBackup).vm.$emit('ready')
    await flushPromises()
    expect(document.body.textContent).toContain('someone@minibits.cash')
    expect(wrapper.emitted('close')).toBeFalsy()
    wrapper.unmount()
  })

  it('keeps a seeded wallet on the retry path during a service outage', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'wallet.ecash-seed-status') return { active: true, can_activate: true } as never
      throw new Error('service unavailable')
    })
    const wrapper = mount(ReceiveBitcoinModal, { props: { show: true }, attachTo: document.body })
    Array.from(document.body.querySelectorAll('button')).find(b => b.textContent?.toLowerCase().includes('ecash'))!.click()
    await flushPromises()
    expect(wrapper.findComponent(EcashSeedBackup).exists()).toBe(false)
    expect(document.body.textContent).toContain('receiveBitcoin.lnAddressUnavailable')
    const retry = Array.from(document.body.querySelectorAll('button')).find(b => b.textContent === 'Retry')!
    expect(retry).toBeTruthy()
    retry.click()
    await flushPromises()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([r]) => r.method === 'wallet.ecash-lnaddress')).toHaveLength(2)
    wrapper.unmount()
  })

  it('does not close/emit when the ecash tab is clicked and the RPC succeeds', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ address: 'someone@minibits.cash' } as never)

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    await flushPromises()

    const tabs = Array.from(document.body.querySelectorAll('button'))
    const ecashTab = tabs.find((b) => b.textContent?.toLowerCase().includes('ecash'))
    expect(ecashTab).toBeTruthy()

    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('close')).toBeFalsy()
    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy()
    wrapper.unmount()
  })

  it('does not close/emit when the ecash tab is clicked and the RPC fails', async () => {
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('boom'))

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    await flushPromises()

    const tabs = Array.from(document.body.querySelectorAll('button'))
    const ecashTab = tabs.find((b) => b.textContent?.toLowerCase().includes('ecash'))
    expect(ecashTab).toBeTruthy()

    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('close')).toBeFalsy()
    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy()
    wrapper.unmount()
  })

  it('never overlaps slow Lightning-address claim polls', async () => {
    vi.useFakeTimers()
    let finishClaim!: (value: unknown) => void
    const slowClaim = new Promise((resolve) => { finishClaim = resolve })
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }: { method: string }) => {
      if (method === 'wallet.ecash-lnaddress') {
        return { address: 'someone@minibits.cash' } as never
      }
      if (method === 'wallet.ecash-lnaddress-claim') return slowClaim as never
      return {} as never
    })

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    const ecashTab = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.toLowerCase().includes('ecash'),
    )
    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    await vi.advanceTimersByTimeAsync(24_000)
    const claimCalls = vi.mocked(rpcClient.call).mock.calls.filter(
      ([request]) => request.method === 'wallet.ecash-lnaddress-claim',
    )
    expect(claimCalls).toHaveLength(1)

    finishClaim({ claimed_count: 0, received_sats: 0, failed_count: 0 })
    await flushPromises()
    wrapper.unmount()
    vi.useRealTimers()
  })

  it('shows a recent receipt claimed by another active browser context', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }: { method: string }) => {
      if (method === 'wallet.ecash-lnaddress') {
        return { address: 'someone@minibits.cash' } as never
      }
      if (method === 'wallet.ecash-lnaddress-claim') {
        return {
          received_sats: 0,
          failed_count: 0,
          receipt_id: 7,
          receipt_sats: 1000,
          receipt_at: Math.floor(Date.now() / 1000),
        } as never
      }
      return {} as never
    })

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    const ecashTab = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.toLowerCase().includes('ecash'),
    )
    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(document.body.textContent).toContain('1,000')
    expect(document.body.textContent).toContain('RECEIVED')
    expect(document.body.querySelector('textarea')).toBeNull()
    expect(document.body.querySelectorAll('[role="dialog"]')).toHaveLength(1)
    expect(document.body.querySelector('[role="dialog"] h3')?.textContent).toBe('Payment received')
    wrapper.unmount()
  })

  it('does not replay an older durable receipt when the receive screen is reopened', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }: { method: string }) => {
      if (method === 'wallet.ecash-lnaddress') {
        return { address: 'someone@minibits.cash' } as never
      }
      if (method === 'wallet.ecash-lnaddress-claim') {
        return {
          received_sats: 0,
          failed_count: 0,
          receipt_id: 7,
          receipt_sats: 1000,
          receipt_at: Math.floor(Date.now() / 1000) - 30,
        } as never
      }
      return {} as never
    })

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
    })
    const ecashTab = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.toLowerCase().includes('ecash'),
    )
    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()

    expect(document.body.querySelector('[role="dialog"] h3')?.textContent).not.toBe('Payment received')
    expect(document.body.querySelector('textarea')).not.toBeNull()
    wrapper.unmount()
  })
})
