// 02-06 Task 1: caches the Server tab's seven load groups (network summary,
// FIPS summary, VPN peers, interfaces, Tor services — shared by
// checkTorStatus and loadTorServices — and disk status) behind keyed
// useCachedResource entries, with per-group TTL/persist decisions and the
// cold-load fan-out kept concurrent (RESEARCH A3 settled: none of the seven
// loaders consumes another's result — see 02-06-SUMMARY.md).

import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Server from '../Server.vue'
import RefreshIndicator from '@/components/RefreshIndicator.vue'

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ packages: {} }),
}))

// Records every rpcClient.call({method}) invocation so tests can assert
// per-group call counts and concurrency without depending on real network
// I/O. vpnStatus/dnsStatus/diskStatus are separate convenience methods on
// rpcClient (not routed through call()), so they're mocked independently.
const rpcCallMock = vi.fn(async ({ method }: { method: string }) => {
  switch (method) {
    case 'network.diagnostics':
      return { tor_connected: true, wifi_count: 2, wifi_ssid: 'Lab WiFi' }
    case 'router.list-forwards':
      return { forwards: [] }
    case 'network.list-interfaces':
      return { interfaces: [] }
    case 'tor.list-services':
      return { services: [], tor_running: false }
    case 'vpn.list-peers':
      return { peers: [] }
    case 'fips.status':
      return { installed: false, service_active: false, key_present: false }
    default:
      return {}
  }
})

const vpnStatusMock = vi.fn(async () => ({
  connected: true, provider: 'wireguard', ip_address: '10.0.0.2/32', wg_ip: '10.0.0.1/24',
  peers_connected: 0, bytes_in: 0, bytes_out: 0, configured: true, configured_provider: 'wireguard',
}))
const dnsStatusMock = vi.fn(async () => ({
  provider: 'system', servers: [], doh_enabled: false, doh_url: null, resolv_conf_servers: [],
}))
const diskStatusMock = vi.fn(async () => ({
  used_bytes: 0, total_bytes: 0, free_bytes: 0, used_percent: 0, level: 'ok' as const,
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: (...args: unknown[]) => (rpcCallMock as unknown as (...a: unknown[]) => unknown)(...args),
    vpnStatus: (...args: unknown[]) => (vpnStatusMock as unknown as (...a: unknown[]) => unknown)(...args),
    dnsStatus: (...args: unknown[]) => (dnsStatusMock as unknown as (...a: unknown[]) => unknown)(...args),
    diskStatus: (...args: unknown[]) => (diskStatusMock as unknown as (...a: unknown[]) => unknown)(...args),
  },
}))

const Other = defineComponent({ name: 'Other', render: () => h('div', 'other') })

function mountServerHost() {
  const Host = defineComponent({
    setup() {
      const show = ref(true)
      return { show }
    },
    render() {
      return h(KeepAlive, null, () =>
        this.show ? h(Server, { key: 'server' }) : h(Other, { key: 'other' }),
      )
    },
  })
  return mount(Host, {
    global: {
      plugins: [createPinia()],
      stubs: {
        QuickActionsCard: true,
        TorServicesCard: true,
        ServerModals: true,
        FipsNetworkCard: true,
      },
    },
  })
}

function callCountFor(method: string): number {
  return rpcCallMock.mock.calls.filter(([opts]) => (opts as { method: string }).method === method).length
}

async function toggleTab(wrapper: ReturnType<typeof mountServerHost>, show: boolean) {
  ;(wrapper.vm as unknown as { show: boolean }).show = show
  await wrapper.vm.$nextTick()
}

beforeEach(() => {
  vi.useFakeTimers()
  rpcCallMock.mockClear()
  vpnStatusMock.mockClear()
  dnsStatusMock.mockClear()
  diskStatusMock.mockClear()
  try {
    sessionStorage.clear()
  } catch { /* unavailable in some envs */ }
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Server tab cache (Task 1): seven load groups', () => {
  it('a cold load fires all seven groups concurrently — every RPC has already started before any microtask resolves', async () => {
    const wrapper = mountServerHost()

    // Synchronous check (no await yet): armServerEntryEffects/onMounted call
    // all seven loaders without awaiting one before starting the next — a
    // serialized chain could not have reached all seven RPCs yet here.
    expect(callCountFor('network.diagnostics')).toBe(1)
    expect(callCountFor('router.list-forwards')).toBe(1)
    // vpnStatus fires twice on a fresh mount: once for network-summary's own
    // fetch, once for the VPN poll's immediate first tick (armVpnPoll, a
    // deliberate every-activation effect independent of this TTL cache).
    expect(vpnStatusMock).toHaveBeenCalledTimes(2)
    expect(dnsStatusMock).toHaveBeenCalledTimes(1)
    expect(callCountFor('network.list-interfaces')).toBe(1)
    expect(callCountFor('tor.list-services')).toBe(1)
    expect(callCountFor('vpn.list-peers')).toBe(1)
    expect(callCountFor('fips.status')).toBe(1)
    expect(diskStatusMock).toHaveBeenCalledTimes(1)

    await flushPromises()
    wrapper.unmount()
  })

  it('reactivating inside every group\'s TTL issues zero additional RPCs across all seven groups', async () => {
    const wrapper = mountServerHost()
    await flushPromises()
    const before = {
      diag: callCountFor('network.diagnostics'),
      fwd: callCountFor('router.list-forwards'),
      iface: callCountFor('network.list-interfaces'),
      tor: callCountFor('tor.list-services'),
      vpnPeers: callCountFor('vpn.list-peers'),
      fips: callCountFor('fips.status'),
      vpnStatus: vpnStatusMock.mock.calls.length,
      dns: dnsStatusMock.mock.calls.length,
      disk: diskStatusMock.mock.calls.length,
    }

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(3000) // well under every group's TTL (shortest is 10s)
    await toggleTab(wrapper, true)
    await flushPromises()

    expect(callCountFor('network.diagnostics')).toBe(before.diag)
    expect(callCountFor('router.list-forwards')).toBe(before.fwd)
    expect(callCountFor('network.list-interfaces')).toBe(before.iface)
    expect(callCountFor('tor.list-services')).toBe(before.tor)
    expect(callCountFor('vpn.list-peers')).toBe(before.vpnPeers)
    expect(callCountFor('fips.status')).toBe(before.fips)
    // dnsStatus is purely network-summary's own TTL-gated call — untouched.
    expect(dnsStatusMock.mock.calls.length).toBe(before.dns)
    // vpnStatus is the one exception: armVpnPoll's immediate first tick on
    // reactivation fires regardless of network-summary's own TTL (a
    // deliberate every-activation VPN-IP freshness effect from 02-04, not
    // something this task's TTL/persist conversion changes).
    expect(vpnStatusMock.mock.calls.length).toBe(before.vpnStatus + 1)
    expect(diskStatusMock.mock.calls.length).toBe(before.disk)

    wrapper.unmount()
  })

  it('reactivating past the short (10s) TTL revalidates the fast groups while the longer-TTL FIPS/Tor/VPN-peer groups stay cached', async () => {
    const wrapper = mountServerHost()
    await flushPromises()
    const diagAtMount = callCountFor('network.diagnostics')
    const ifaceAtMount = callCountFor('network.list-interfaces')
    const diskAtMount = diskStatusMock.mock.calls.length
    const fipsAtMount = callCountFor('fips.status')
    const torAtMount = callCountFor('tor.list-services')
    const vpnPeersAtMount = callCountFor('vpn.list-peers')

    await toggleTab(wrapper, false)
    // Past the 10s fast tier, well under the 30s Tor/VPN-peer tier and the
    // 60s FIPS tier.
    vi.advanceTimersByTime(12000)
    await toggleTab(wrapper, true)
    await flushPromises()

    expect(callCountFor('network.diagnostics')).toBe(diagAtMount + 1)
    expect(callCountFor('network.list-interfaces')).toBe(ifaceAtMount + 1)
    expect(diskStatusMock.mock.calls.length).toBe(diskAtMount + 1)
    // FIPS (60s), Tor services (30s) and VPN peers (30s) are still fresh.
    expect(callCountFor('fips.status')).toBe(fipsAtMount)
    expect(callCountFor('tor.list-services')).toBe(torAtMount)
    expect(callCountFor('vpn.list-peers')).toBe(vpnPeersAtMount)

    wrapper.unmount()
  })

  it('a rejected refresh in one group leaves the other six unaffected and keeps that group\'s last known data rendered', async () => {
    // Mounted directly (no KeepAlive host) so the exposed loadNetworkData()
    // method is reachable, matching ServerNetworkRefresh.test.ts's convention.
    const wrapper = mount(Server, {
      global: {
        plugins: [createPinia()],
        stubs: { QuickActionsCard: true, TorServicesCard: true, ServerModals: true, FipsNetworkCard: true },
      },
    })
    await flushPromises()
    expect(wrapper.text()).toContain('Lab WiFi')

    rpcCallMock.mockImplementationOnce(async ({ method }: { method: string }) => {
      if (method === 'network.diagnostics') throw new Error('offline')
      return {}
    })

    await (wrapper.vm as unknown as { loadNetworkData: () => Promise<void> }).loadNetworkData()
    await flushPromises()

    // Prior network data stays rendered (keep-last-value on error, D-07).
    expect(wrapper.text()).toContain('Lab WiFi')
    // Other groups' own data is untouched by the rejection.
    expect(diskStatusMock).toHaveBeenCalled()
    expect(callCountFor('fips.status')).toBeGreaterThan(0)
    expect(callCountFor('vpn.list-peers')).toBeGreaterThan(0)
    expect(callCountFor('network.list-interfaces')).toBeGreaterThan(0)
    expect(callCountFor('tor.list-services')).toBeGreaterThan(0)

    wrapper.unmount()
  })

  it('groups carrying VPN peer identity, Tor onion addresses, or this node\'s own FIPS identity key declare persist:false; non-identity groups may persist', async () => {
    const wrapper = mountServerHost()
    await flushPromises()

    const readSnapshot = (key: string) => {
      try {
        return sessionStorage.getItem(`resource:${key}`)
      } catch {
        return null
      }
    }
    expect(readSnapshot('server.vpn-peers')).toBeNull() // carries npub (peer identity)
    expect(readSnapshot('server.tor-services')).toBeNull() // carries onion_address
    expect(readSnapshot('server.network-summary')).not.toBeNull() // this node's own status
    // CR-01 follow-up correction: `server.fips-summary` is shared with
    // FipsNetworkCard.vue, whose fuller FipsStatus type shows the real
    // `fips.status` response also carries `npub` — this node's own FIPS
    // identity public key — so this key must be persist:false too (T-02-01),
    // not persist:true as originally assumed when only the narrower
    // installed/service_active/key_present fields were considered.
    expect(readSnapshot('server.fips-summary')).toBeNull()
    expect(readSnapshot('server.interfaces')).not.toBeNull()
    expect(readSnapshot('server.disk-status')).not.toBeNull()

    wrapper.unmount()
  })

  it('every fetcher backing the seven load groups is registered with dedup:true', async () => {
    const wrapper = mountServerHost()
    await flushPromises()

    const cachedMethods = [
      'network.diagnostics', 'router.list-forwards', 'network.list-interfaces',
      'tor.list-services', 'vpn.list-peers', 'fips.status',
    ]
    const dedupFlags = rpcCallMock.mock.calls
      .filter(([opts]) => cachedMethods.includes((opts as { method: string }).method))
      .map(([opts]) => (opts as { dedup?: boolean }).dedup)
    expect(dedupFlags.length).toBe(cachedMethods.length)
    expect(dedupFlags.every(Boolean)).toBe(true)

    wrapper.unmount()
  })

  it('renders RefreshIndicator wired to whether any of the seven groups is refreshing', async () => {
    const wrapper = mountServerHost()
    await flushPromises()

    const indicator = wrapper.findComponent(RefreshIndicator)
    expect(indicator.exists()).toBe(true)
    expect(indicator.props('state')).toBe('ready')

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(12000)
    await toggleTab(wrapper, true)
    await wrapper.vm.$nextTick()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('refreshing')

    await flushPromises()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('ready')

    wrapper.unmount()
  })
})
