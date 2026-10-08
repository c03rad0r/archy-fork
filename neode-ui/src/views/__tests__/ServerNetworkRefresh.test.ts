import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import Server from '../Server.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ packages: {} }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    vpnStatus: vi.fn(),
    dnsStatus: vi.fn(),
    diskStatus: vi.fn(),
  },
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function mountServer(options: { renderTorServices?: boolean } = {}) {
  return mount(Server, {
    global: {
      // The cached-resource layer pulls the Pinia resources store in setup.
      plugins: [createPinia()],
      stubs: {
        QuickActionsCard: true,
        TorServicesCard: options.renderTorServices ? false : true,
        ServerModals: true,
        FipsNetworkCard: true,
      },
    },
  })
}

describe('Server network refresh states', () => {
  it('shows an interface fetch failure without claiming hardware is absent, then retries', async () => {
    sessionStorage.clear()
    let failed = true
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'network.list-interfaces') {
        if (failed) return Promise.reject(new Error('CSRF token missing or invalid'))
        return Promise.resolve({ interfaces: [{ name: 'wlp3s0', type: 'wifi', state: 'up', mac: '', ipv4: ['192.0.2.10'] }] })
      }
      return Promise.resolve({})
    })
    vi.mocked(rpcClient.vpnStatus).mockResolvedValue({ connected: false } as never)
    vi.mocked(rpcClient.dnsStatus).mockResolvedValue({ provider: 'system', resolv_conf_servers: [], doh_enabled: false } as never)
    vi.mocked(rpcClient.diskStatus).mockResolvedValue({ encrypted: false, warnings: [] } as never)
    const wrapper = mountServer()
    await flushPromises()
    expect(wrapper.text()).toContain('Couldn’t load network interfaces.')
    expect(wrapper.text()).not.toContain('No physical interfaces detected')
    failed = false
    const retry = wrapper.findAll('button').find(button => button.text() === 'Retry')!
    await retry.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('wlp3s0')
    expect(wrapper.text()).not.toContain('Couldn’t load network interfaces.')
    wrapper.unmount()
    sessionStorage.clear()
  })
  it('keeps network overview visible while refresh is pending', async () => {
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'network.diagnostics') {
        return Promise.resolve({ tor_connected: true, wifi_count: 2, wifi_ssid: 'Lab WiFi' })
      }
      if (request.method === 'router.list-forwards') {
        return Promise.resolve({ forwards: [{}, {}] })
      }
      if (request.method === 'network.list-interfaces') {
        return Promise.resolve({ interfaces: [] })
      }
      if (request.method === 'tor.list-services') {
        return Promise.resolve({ services: [], tor_running: false })
      }
      if (request.method === 'vpn.list-peers') {
        return Promise.resolve({ peers: [] })
      }
      if (request.method === 'fips.status') {
        return Promise.resolve({ installed: false, service_active: false, key_present: false })
      }
      return Promise.resolve({})
    })
    vi.mocked(rpcClient.vpnStatus).mockResolvedValue({ connected: true, provider: 'wireguard', ip_address: '10.0.0.2/32', wg_ip: '10.0.0.1/24' } as never)
    vi.mocked(rpcClient.dnsStatus).mockResolvedValue({ provider: 'cloudflare', resolv_conf_servers: ['1.1.1.1'], doh_enabled: true } as never)
    vi.mocked(rpcClient.diskStatus).mockResolvedValue({ encrypted: false, warnings: [] } as never)

    const wrapper = mountServer()
    await flushPromises()

    expect(wrapper.text()).toContain('Lab WiFi')
    expect(wrapper.text()).toContain('2 rules')

    const pendingDiagnostics = deferred<{ tor_connected: boolean; wifi_count: number; wifi_ssid: string }>()
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'network.diagnostics') return pendingDiagnostics.promise
      if (request.method === 'router.list-forwards') return Promise.reject(new Error('offline'))
      return Promise.resolve({})
    })
    vi.mocked(rpcClient.vpnStatus).mockRejectedValueOnce(new Error('offline') as never)
    vi.mocked(rpcClient.dnsStatus).mockRejectedValueOnce(new Error('offline') as never)

    const refresh = (wrapper.vm as unknown as { loadNetworkData: () => Promise<void> }).loadNetworkData()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Lab WiFi')
    expect(wrapper.text()).toContain('2 rules')
    expect(wrapper.text()).toContain('Refreshing network...')

    pendingDiagnostics.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Lab WiFi')
    expect(wrapper.text()).toContain('2 rules')
  })

  it('keeps network interfaces visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'network.list-interfaces') {
        return Promise.resolve({
          interfaces: [{ name: 'eth0', type: 'ethernet', state: 'up', mac: '00:11:22:33:44:55', ipv4: ['192.0.2.10'] }],
        })
      }
      if (request.method === 'network.diagnostics') {
        return Promise.resolve({ tor_connected: false })
      }
      if (request.method === 'router.list-forwards') {
        return Promise.resolve({ forwards: [] })
      }
      if (request.method === 'tor.list-services') {
        return Promise.resolve({ services: [], tor_running: false })
      }
      if (request.method === 'vpn.list-peers') {
        return Promise.resolve({ peers: [] })
      }
      if (request.method === 'fips.status') {
        return Promise.resolve({ installed: false, service_active: false, key_present: false })
      }
      return Promise.resolve({})
    })
    vi.mocked(rpcClient.vpnStatus).mockResolvedValue({ connected: false } as never)
    vi.mocked(rpcClient.dnsStatus).mockResolvedValue({ provider: 'system', resolv_conf_servers: [], doh_enabled: false } as never)
    vi.mocked(rpcClient.diskStatus).mockResolvedValue({ encrypted: false, warnings: [] } as never)

    const wrapper = mountServer()
    await flushPromises()

    expect(wrapper.text()).toContain('eth0')
    expect(wrapper.text()).toContain('192.0.2.10')

    const pendingInterfaces = deferred<{ interfaces: [] }>()
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'network.list-interfaces') return pendingInterfaces.promise
      return Promise.resolve({})
    })

    const refresh = (wrapper.vm as unknown as { loadInterfaces: () => Promise<void> }).loadInterfaces()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('eth0')
    expect(wrapper.text()).toContain('192.0.2.10')
    expect(wrapper.text()).toContain('Refreshing interfaces...')

    pendingInterfaces.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('eth0')
    expect(wrapper.text()).toContain('192.0.2.10')
  })

  it('keeps Tor services visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'tor.list-services') {
        return Promise.resolve({
          services: [{
            name: 'filebrowser',
            local_port: 8080,
            onion_address: 'filebrowser123456789.onion',
            enabled: true,
            unauthenticated: false,
            protocol: false,
          }],
          tor_running: true,
        })
      }
      if (request.method === 'network.diagnostics') {
        return Promise.resolve({ tor_connected: true })
      }
      if (request.method === 'router.list-forwards') {
        return Promise.resolve({ forwards: [] })
      }
      if (request.method === 'network.list-interfaces') {
        return Promise.resolve({ interfaces: [] })
      }
      if (request.method === 'vpn.list-peers') {
        return Promise.resolve({ peers: [] })
      }
      if (request.method === 'fips.status') {
        return Promise.resolve({ installed: false, service_active: false, key_present: false })
      }
      return Promise.resolve({})
    })
    vi.mocked(rpcClient.vpnStatus).mockResolvedValue({ connected: false } as never)
    vi.mocked(rpcClient.dnsStatus).mockResolvedValue({ provider: 'system', resolv_conf_servers: [], doh_enabled: false } as never)
    vi.mocked(rpcClient.diskStatus).mockResolvedValue({ encrypted: false, warnings: [] } as never)

    const wrapper = mountServer({ renderTorServices: true })
    await flushPromises()

    expect(wrapper.text()).toContain('filebrowser')
    expect(wrapper.text()).toContain('filebrowser123456789.onion')

    const pendingTor = deferred<{ services: []; tor_running: boolean }>()
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'tor.list-services') return pendingTor.promise
      return Promise.resolve({})
    })

    const refresh = (wrapper.vm as unknown as { loadTorServices: () => Promise<void> }).loadTorServices()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('filebrowser')
    expect(wrapper.text()).toContain('filebrowser123456789.onion')
    expect(wrapper.text()).toContain('Refreshing Tor services...')

    pendingTor.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('filebrowser')
    expect(wrapper.text()).toContain('filebrowser123456789.onion')
  })
})
