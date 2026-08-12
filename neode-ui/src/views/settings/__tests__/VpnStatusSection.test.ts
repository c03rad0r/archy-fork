import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import VpnStatusSection from '../VpnStatusSection.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
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

function makeVpnStatus() {
  return {
    connected: true,
    provider: 'wireguard',
    interface: 'wg0',
    ip_address: '10.0.0.2/32',
    hostname: 'node',
    peers_connected: 2,
    bytes_in: 1024,
    bytes_out: 2048,
  }
}

describe('VpnStatusSection', () => {
  it('keeps VPN status visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce(makeVpnStatus())

    const wrapper = mount(VpnStatusSection)
    await flushPromises()

    expect(wrapper.text()).toContain('Connected')
    expect(wrapper.text()).toContain('wireguard')
    expect(wrapper.text()).toContain('10.0.0.2/32')

    const pending = deferred<ReturnType<typeof makeVpnStatus>>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { fetchVpnStatus: () => Promise<void> }).fetchVpnStatus()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Connected')
    expect(wrapper.text()).toContain('wireguard')
    expect(wrapper.text()).toContain('Refreshing VPN status...')
    expect(wrapper.text()).not.toContain('Loading VPN status...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Connected')
    expect(wrapper.text()).toContain('wireguard')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing VPN status...')
  })
})
