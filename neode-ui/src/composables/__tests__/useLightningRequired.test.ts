import { describe, it, expect, beforeEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useLightningRequired } from '../useLightningRequired'
import { rpcClient } from '@/api/rpc-client'

// The gate reads install state off the app store's package list. Stub the
// store rather than the RPC layer so the test pins the decision, not the
// transport.
const packages = vi.hoisted(() => ({ value: {} as Record<string, unknown> }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    get packages() {
      return packages.value
    },
  }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

describe('useLightningRequired', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    packages.value = {}
    // Module-scope `show` is shared by design (one global modal), so reset it
    // between cases or the first opener leaks into the next test.
    useLightningRequired().close()
  })

  it('lets the action through when a Lightning node is running', () => {
    packages.value = { lnd: { state: 'running' }, 'bitcoin-knots': { state: 'running' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('running')
    expect(lightning.requireLightningNode()).toBe(true)
    expect(lightning.show.value).toBe(false)
  })

  it('blocks when the node is present but NOT running, and says so', () => {
    // The bug this closes: `id in packages` is not "usable". A node with an
    // lnd entry in a non-running state produced a raw connection-refused
    // error ("Operation failed. Check server logs for details.").
    packages.value = { lnd: { state: 'stopped' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('stopped')
    expect(lightning.requireLightningNode()).toBe(false)
    expect(lightning.show.value).toBe(true)
    expect(lightning.status.value).toBe('stopped')
  })

  it('blocks and raises the install modal when no Lightning node is installed', () => {
    packages.value = { 'bitcoin-knots': { state: 'running' }, immich: { state: 'running' } }
    const lightning = useLightningRequired()

    expect(lightning.lightningStatus()).toBe('absent')
    expect(lightning.hasLightningNode()).toBe(false)
    // Returns false so the caller bails WITHOUT surfacing an error string —
    // that was the whole defect: a missing prerequisite rendered as a failure.
    expect(lightning.requireLightningNode()).toBe(false)
    expect(lightning.show.value).toBe(true)
    expect(lightning.status.value).toBe('absent')
  })

  it('shares one modal state across call sites', () => {
    packages.value = {}
    const a = useLightningRequired()
    const b = useLightningRequired()

    a.requireLightningNode()
    expect(b.show.value).toBe(true)
    b.close()
    expect(a.show.value).toBe(false)
  })

  it('treats an empty package list as absent', () => {
    packages.value = {}
    expect(useLightningRequired().lightningStatus()).toBe('absent')
  })

  describe('requireLightningReady states the node\u2019s real funding state', () => {
    beforeEach(() => {
      packages.value = { lnd: { state: 'running' } }
      vi.mocked(rpcClient.call).mockReset()
    })

    it('says the channel is confirming, not \u201cno channel\u201d, while pending', async () => {
      // The regression (framework-pt, 2026-09-01): a just-opened channel
      // sits in LND's pending list; the outbound sum is legitimately 0, but
      // the modal claimed the node had no channel at all.
      vi.mocked(rpcClient.call).mockResolvedValue({
        total_inbound: 0,
        total_outbound: 0,
        channels: [{ status: 'pending_open', local_balance: 900000, remote_balance: 0 }],
      })
      const lightning = useLightningRequired()

      expect(await lightning.requireLightningReady('send')).toBe(false)
      expect(lightning.show.value).toBe(true)
      expect(lightning.status.value).toBe('no-funds')
      expect(lightning.fundingReason.value).toBe('pending')
    })

    it('says the balance is on the far side when channels exist but outbound is 0', async () => {
      vi.mocked(rpcClient.call).mockResolvedValue({
        total_inbound: 985000,
        total_outbound: 0,
        channels: [{ status: 'active', local_balance: 0, remote_balance: 985000 }],
      })
      const lightning = useLightningRequired()

      expect(await lightning.requireLightningReady('send')).toBe(false)
      expect(lightning.fundingReason.value).toBe('far-side')
      // The same node CAN receive — the gate must pass for the other way.
      vi.mocked(rpcClient.call).mockResolvedValue({
        total_inbound: 985000,
        total_outbound: 0,
        channels: [{ status: 'active', local_balance: 0, remote_balance: 985000 }],
      })
      expect(await lightning.requireLightningReady('receive')).toBe(true)
    })

    it('keeps the open-a-channel guidance only when there truly is no channel', async () => {
      vi.mocked(rpcClient.call).mockResolvedValue({
        total_inbound: 0,
        total_outbound: 0,
        channels: [],
      })
      const lightning = useLightningRequired()

      expect(await lightning.requireLightningReady('send')).toBe(false)
      expect(lightning.fundingReason.value).toBe('none')
    })

    it('fails OPEN on an RPC error \u2014 a transient blip must not block a working wallet', async () => {
      vi.mocked(rpcClient.call).mockRejectedValue(new Error('Failed to fetch'))
      const lightning = useLightningRequired()

      expect(await lightning.requireLightningReady('send')).toBe(true)
      expect(lightning.show.value).toBe(false)
    })

    it('maps a routing/liquidity payment failure onto the modal without claiming \u201cno channel\u201d', () => {
      const lightning = useLightningRequired()

      expect(lightning.handleLightningFailure(new Error('Payment failed: unable to find a path to destination'))).toBe(true)
      expect(lightning.status.value).toBe('no-funds')
      expect(lightning.fundingReason.value).toBe('failed-payment')
    })

    it('leaves non-funding payment errors to the caller', () => {
      const lightning = useLightningRequired()

      expect(lightning.handleLightningFailure(new Error('Payment failed: Not Found'))).toBe(false)
      expect(lightning.show.value).toBe(false)
    })
  })
})
