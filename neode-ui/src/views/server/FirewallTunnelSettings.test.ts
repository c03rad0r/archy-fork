import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn(), vpnStatus: vi.fn() } }))
import { rpcClient } from '@/api/rpc-client'
import FirewallTunnelSettings from './FirewallTunnelSettings.vue'
const mountPage = () => mount(FirewallTunnelSettings, { global: { stubs: { RouterLink: { template: '<a><slot /></a>' }, VpnDeviceSettings: { props: ['configured', 'managementVerified'], template: '<div data-testid="devices" :data-configured="configured" :data-verified="managementVerified" />' } } } })
const vpn = { connected: true, provider: 'tailscale', configured: true, configured_provider: 'nostrvpn', wg_ip: '10.44.0.1' }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(rpcClient.vpnStatus).mockResolvedValue(vpn as any)
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'fips.status') return { service_active: true, authenticated_peer_count: 5 }
    if (method === 'router.info') return { configured: false, router_type: 'unknown' }
    if (method === 'router.list-forwards') return { forwards: [] }
    throw new Error(method)
  })
})
describe('Firewall and tunnel settings', () => {
  it('recognizes the existing device tunnel independently of router setup and never infers firewall protection from FIPS', async () => {
    const w = mountPage(); await flushPromises()
    expect(w.text()).toContain('Your device tunnel is already set up')
    expect(w.text()).toContain('No router connection is saved in this section')
    expect(w.text()).toContain('Not checked')
    expect(w.text()).not.toContain('Protected')
    expect(w.get('[data-testid="devices"]').attributes('data-configured')).toBe('true')
    expect(w.get('[data-testid="devices"]').attributes('data-verified')).toBe('false')
    w.unmount()
  })
  it('only enables device mutations when the backend explicitly verifies management support', async () => {
    vi.mocked(rpcClient.vpnStatus).mockResolvedValue({ ...vpn, peer_management_verified: true } as any)
    const w = mountPage(); await flushPromises()
    expect(w.get('[data-testid="devices"]').attributes('data-verified')).toBe('true'); w.unmount()
  })
  it('clears stale success after failed checks and does not turn missing forward results into an empty list', async () => {
    const w = mountPage(); await flushPromises()
    vi.mocked(rpcClient.vpnStatus).mockRejectedValue(new Error('offline'))
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('offline'))
    await w.get('button').trigger('click'); await flushPromises()
    expect(w.text()).not.toContain('Your device tunnel is already set up')
    expect(w.text()).toContain('Could not check saved entries.')
    expect(w.text()).not.toContain('No entries are saved here.')
    expect(w.get('[data-testid="devices"]').attributes('data-verified')).toBe('false')
    expect(w.get('[role="alert"]').text()).toContain('could not be checked'); w.unmount()
  })
  it('renders named port entries without claiming they were applied or exposing raw JSON', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'router.list-forwards' ? { forwards: [{ id: 'a', service_name: 'My app', internal_port: 80, external_port: 8080, protocol: 'TCP', unexpected_secret: 'never-render' }] } : {})
    const w = mountPage(); await flushPromises()
    expect(w.text()).toContain('My app'); expect(w.text()).toContain('Outside port 8080 → app port 80 (TCP)')
    expect(w.text()).toContain('do not confirm that your router has opened them')
    expect(w.text()).not.toContain('never-render'); w.unmount()
  })
  it('treats malformed forwards as unavailable, not zero entries', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ forwards: [null] })
    const w = mountPage(); await flushPromises()
    expect(w.text()).toContain('Could not check saved entries.'); w.unmount()
  })
  it('does not enable changes from a reply arriving after navigation', async () => {
    let resolve!: (x: any) => void
    vi.mocked(rpcClient.vpnStatus).mockImplementation(() => new Promise(r => { resolve = r }))
    const w = mountPage(); w.unmount(); resolve({ ...vpn, peer_management_verified: true }); await flushPromises()
    expect(vi.mocked(rpcClient.call).mock.calls.every(([arg]) => !arg.method.includes('create') && !arg.method.includes('remove'))).toBe(true)
  })
})
