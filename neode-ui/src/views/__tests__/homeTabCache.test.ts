// 02-06 Task 2: caches Home's system stats, update status and cloud storage
// usage behind keyed useCachedResource entries (every-entry, TTL-gated), and
// wraps the existing loadWeb5Status() wallet fetch in its own resource that
// revalidates UNCONDITIONALLY on every activation (T-02-13) — a money figure
// must never be presented as current without a visible re-check. Web5.vue's
// own two resources (web5.networking-profits, web5.lnd-info) were read and
// are NOT shared here — see 02-06-SUMMARY.md for the finding.

import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Home from '../Home.vue'
import HomeWalletCard from '../home/HomeWalletCard.vue'
import RefreshIndicator from '@/components/RefreshIndicator.vue'

vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ push: vi.fn() }),
  RouterLink: { name: 'RouterLink', props: ['to'], template: '<a><slot /></a>' },
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ packages: {} }),
}))

vi.mock('@/api/websocket', () => ({
  wsClient: { subscribe: vi.fn(() => vi.fn()) },
}))

vi.mock('@/api/filebrowser-client', () => ({
  fileBrowserClient: {
    getUsage: vi.fn().mockResolvedValue({ totalSize: 1024, folderCount: 2, fileCount: 5 }),
  },
}))

// Records every rpcClient.call({method}) invocation. wallet/system/update
// calls all route through call(); nothing Home.vue uses goes through a
// separate convenience method (unlike Server.vue's vpnStatus/dnsStatus).
async function defaultRpcCallImpl({ method }: { method: string }) {
  switch (method) {
    case 'system.stats':
      return { cpu_usage_percent: 10, mem_used_bytes: 100, mem_total_bytes: 200, disk_used_bytes: 1, disk_total_bytes: 2, uptime_secs: 60 }
    case 'bitcoin.getinfo':
      return { block_height: 100, sync_progress: 1 }
    case 'fips.status':
      return { installed: false, service_active: false, key_present: false }
    case 'openwrt.get-status':
      return { tollgate: { installed: false } }
    case 'update.status':
      return { update_available: false }
    case 'lnd.getinfo':
      return { balance_sats: 5000, channel_balance_sats: 2500, synced_to_chain: true }
    case 'wallet.ecash-balance':
      return { balance_sats: 100 }
    case 'wallet.fedimint-balance':
      return { balance_sats: 0 }
    case 'wallet.ark-balance':
      return { spendable_sats: 0 }
    case 'lnd.gettransactions':
      return { transactions: [], incoming_pending_count: 0 }
    case 'lnd.lightning-history':
      return { transactions: [] }
    case 'wallet.ecash-history':
      return { transactions: [] }
    default:
      return {}
  }
}

const rpcCallMock = vi.fn(defaultRpcCallImpl)

const vpnStatusMock = vi.fn(async () => ({
  connected: false, peers_connected: 0, bytes_in: 0, bytes_out: 0, configured: false, configured_provider: '',
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: (...args: unknown[]) => (rpcCallMock as unknown as (...a: unknown[]) => unknown)(...args),
    vpnStatus: (...args: unknown[]) => (vpnStatusMock as unknown as (...a: unknown[]) => unknown)(...args),
  },
}))

const Other = defineComponent({ name: 'Other', render: () => h('div', 'other') })

function mountHomeHost() {
  const Host = defineComponent({
    setup() {
      const show = ref(true)
      return { show }
    },
    render() {
      return h(KeepAlive, null, () =>
        this.show ? h(Home, { key: 'home' }) : h(Other, { key: 'other' }),
      )
    },
  })
  return mount(Host, {
    global: {
      plugins: [createPinia()],
      stubs: {
        HomeSystemCard: true,
        EasyHome: true,
        WalletScanModal: true,
        SendBitcoinModal: true,
        ReceiveBitcoinModal: true,
        TransactionsModal: true,
        WalletSettingsModal: true,
      },
    },
  })
}

function callCountFor(method: string): number {
  return rpcCallMock.mock.calls.filter(([opts]) => (opts as { method: string }).method === method).length
}

async function toggleTab(wrapper: ReturnType<typeof mountHomeHost>, show: boolean) {
  ;(wrapper.vm as unknown as { show: boolean }).show = show
  await wrapper.vm.$nextTick()
}

// loadWeb5Status()'s nested Promise.allSettled chains (balances + histories,
// each wrapping several rpcClient.call().then() hops) need more than one
// macrotask boundary to fully settle under fake timers — a single
// flushPromises() left the wallet resource's loadState at 'loading' in
// practice. Two calls reliably drain it.
async function settle() {
  await flushPromises()
  await flushPromises()
}

beforeEach(() => {
  vi.useFakeTimers()
  rpcCallMock.mockClear()
  // Guard against a leaking permanent mockImplementation() override from a
  // prior test (e.g. the never-resolving-promise case below) — mockClear()
  // only clears call history, not a permanently swapped implementation.
  rpcCallMock.mockImplementation(defaultRpcCallImpl)
  vpnStatusMock.mockClear()
  try {
    sessionStorage.clear()
    localStorage.clear()
  } catch { /* unavailable in some envs */ }
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Home tab cache (Task 2): system/update/storage groups + wallet freshness', () => {
  it('reactivating inside the TTL issues zero new RPCs for the system, update and storage-usage groups', async () => {
    const wrapper = mountHomeHost()
    await settle()
    const before = {
      stats: callCountFor('system.stats'),
      btc: callCountFor('bitcoin.getinfo'),
      fips: callCountFor('fips.status'),
      tollgate: callCountFor('openwrt.get-status'),
      vpn: vpnStatusMock.mock.calls.length,
      update: callCountFor('update.status'),
      usage: vi.mocked((await import('@/api/filebrowser-client')).fileBrowserClient.getUsage).mock.calls.length,
    }

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(3000) // well under every group's TTL (shortest is 10s)
    await toggleTab(wrapper, true)
    await settle()

    expect(callCountFor('system.stats')).toBe(before.stats)
    expect(callCountFor('bitcoin.getinfo')).toBe(before.btc)
    expect(callCountFor('fips.status')).toBe(before.fips)
    expect(callCountFor('openwrt.get-status')).toBe(before.tollgate)
    expect(vpnStatusMock.mock.calls.length).toBe(before.vpn)
    expect(callCountFor('update.status')).toBe(before.update)
    expect(vi.mocked((await import('@/api/filebrowser-client')).fileBrowserClient.getUsage).mock.calls.length).toBe(before.usage)

    wrapper.unmount()
  })

  it('reactivating always triggers a wallet revalidation even when the TTL has not lapsed', async () => {
    const wrapper = mountHomeHost()
    await settle()
    const lndCallsAtMount = callCountFor('lnd.getinfo')
    expect(lndCallsAtMount).toBeGreaterThan(0)

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(1000) // nowhere near stale by any TTL definition
    await toggleTab(wrapper, true)
    await settle()

    // Unconditional — fires again regardless of staleness (T-02-13).
    expect(callCountFor('lnd.getinfo')).toBe(lndCallsAtMount + 1)

    wrapper.unmount()
  })

  it('the previously rendered wallet figure stays in the DOM throughout a wallet revalidation', async () => {
    const wrapper = mount(Home, {
      global: {
        plugins: [createPinia()],
        stubs: {
          HomeSystemCard: true,
          EasyHome: true,
          WalletScanModal: true,
          SendBitcoinModal: true,
          ReceiveBitcoinModal: true,
          TransactionsModal: true,
          WalletSettingsModal: true,
        },
      },
    })
    await settle()

    const walletCardBefore = wrapper.findComponent(HomeWalletCard)
    expect(walletCardBefore.exists()).toBe(true)
    expect(walletCardBefore.props('walletOnchain')).toBe(5000)

    // Trigger a fresh revalidation directly (mirrors what reactivation does)
    // and assert the card's props are still populated with the prior figure
    // before the new fetch resolves — never blanked/reset to 0 mid-flight.
    let resolveLnd!: (v: unknown) => void
    rpcCallMock.mockImplementationOnce(() => new Promise((resolve) => { resolveLnd = resolve as (v: unknown) => void }) as ReturnType<typeof defaultRpcCallImpl>)
    const pending = (wrapper.vm as unknown as { loadWeb5Status: () => Promise<void> }).loadWeb5Status()
    await wrapper.vm.$nextTick()

    const walletCardDuring = wrapper.findComponent(HomeWalletCard)
    expect(walletCardDuring.props('walletOnchain')).toBe(5000)

    resolveLnd({ balance_sats: 6000, channel_balance_sats: 2500, synced_to_chain: true })
    await pending
    await settle()

    wrapper.unmount()
  })

  it('no sessionStorage key exists for the wallet resource after a mount and reactivation cycle', async () => {
    const wrapper = mountHomeHost()
    await settle()
    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(1000)
    await toggleTab(wrapper, true)
    await settle()

    const readSnapshot = (key: string) => {
      try {
        return sessionStorage.getItem(`resource:${key}`)
      } catch {
        return null
      }
    }
    expect(readSnapshot('home.wallet-status')).toBeNull()
    // Non-sensitive groups may persist.
    expect(readSnapshot('home.system-stats')).not.toBeNull()
    expect(readSnapshot('home.update-status')).not.toBeNull()
    expect(readSnapshot('home.cloud-usage')).not.toBeNull()

    wrapper.unmount()
  })

  it('hydrateWalletSnapshot still paints last-known figures before any network round-trip', async () => {
    localStorage.setItem('archy-wallet-snapshot-v1', JSON.stringify({
      onchain: 42000, lightning: 1000, ecash: 0, fedimint: 0, ark: 0, connected: true, transactions: [],
    }))
    // Defer every RPC indefinitely so nothing resolves before the assertion.
    rpcCallMock.mockImplementation(() => new Promise(() => { /* never resolves */ }))

    const wrapper = mount(Home, {
      global: {
        plugins: [createPinia()],
        stubs: {
          HomeSystemCard: true,
          EasyHome: true,
          WalletScanModal: true,
          SendBitcoinModal: true,
          ReceiveBitcoinModal: true,
          TransactionsModal: true,
          WalletSettingsModal: true,
        },
      },
    })
    await wrapper.vm.$nextTick()

    const walletCard = wrapper.findComponent(HomeWalletCard)
    expect(walletCard.props('walletOnchain')).toBe(42000)
    expect(walletCard.props('walletLightning')).toBe(1000)

    wrapper.unmount()
  })

  it('the websocket wallet-push path (02-04) is still present and still triggers a wallet refresh', async () => {
    const { wsClient } = await import('@/api/websocket')
    const wrapper = mountHomeHost()
    await settle()

    expect(vi.mocked(wsClient.subscribe)).toHaveBeenCalled()
    const subscribeCalls = vi.mocked(wsClient.subscribe).mock.calls
    const pushHandler = subscribeCalls[0]?.[0] as (() => void) | undefined
    expect(pushHandler).toBeDefined()
    const lndCallsBefore = callCountFor('lnd.getinfo')

    pushHandler?.()
    vi.advanceTimersByTime(800) // the debounce window
    await settle()

    expect(callCountFor('lnd.getinfo')).toBe(lndCallsBefore + 1)

    wrapper.unmount()
  })

  it('renders RefreshIndicator bound to the wallet resource\'s loadState', async () => {
    const wrapper = mountHomeHost()
    await settle()

    const indicator = wrapper.findComponent(RefreshIndicator)
    expect(indicator.exists()).toBe(true)
    expect(indicator.props('state')).toBe('ready')

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(1000)
    await toggleTab(wrapper, true)
    await wrapper.vm.$nextTick()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('refreshing')

    await settle()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('ready')

    wrapper.unmount()
  })

  it('every fetcher backing the wallet composite call passes dedup:true', async () => {
    const wrapper = mountHomeHost()
    await settle()

    const walletMethods = [
      'lnd.getinfo', 'wallet.ecash-balance', 'wallet.fedimint-balance', 'wallet.ark-balance',
      'lnd.gettransactions', 'lnd.lightning-history', 'wallet.ecash-history',
    ]
    const dedupFlags = rpcCallMock.mock.calls
      .filter(([opts]) => walletMethods.includes((opts as { method: string }).method))
      .map(([opts]) => (opts as { dedup?: boolean }).dedup)
    expect(dedupFlags.length).toBeGreaterThanOrEqual(walletMethods.length)
    expect(dedupFlags.every(Boolean)).toBe(true)

    wrapper.unmount()
  })
})
