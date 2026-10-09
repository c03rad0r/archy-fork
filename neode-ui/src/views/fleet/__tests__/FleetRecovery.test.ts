import { afterEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import Fleet from '../../Fleet.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('vue-router', () => ({
  RouterLink: { template: '<a><slot /></a>' },
  useRouter: () => ({ push: vi.fn() }),
}))
afterEach(() => { sessionStorage.clear(); vi.clearAllMocks() })

const stubs = {
  BackButton: true, FleetOverviewCards: true, FleetAlerts: true,
  FleetNodeDetail: true, FleetContainerMatrix: true,
}

it('keeps cached node cards visible beside the outage warning and recovers with Retry', async () => {
  sessionStorage.setItem('archipelago.fleet.cache.v2', JSON.stringify({
    nodes: [{ node_id: 'retained', node_name: 'Retained node', cpu_pct: 0 }],
  }))
  vi.mocked(rpcClient.call).mockRejectedValue(new Error('offline'))
  const wrapper = mount(Fleet, { global: { stubs } })
  try {
    await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toContain('Showing the last received reports')
    expect(wrapper.get('.fleet-node-card').text()).toContain('Retained node')
    vi.mocked(rpcClient.call).mockResolvedValue({ nodes: [{ node_id: 'retained', node_name: 'Recovered node', cpu_pct: 0 }], alerts: [] })
    await wrapper.get('[role="status"] button').trigger('click')
    await flushPromises()
    expect(wrapper.find('[role="status"]').exists()).toBe(false)
    expect(wrapper.get('.fleet-node-card').text()).toContain('Recovered node')
  } finally { wrapper.unmount() }
})

it('shows an actionable unavailable state when there are no saved reports', async () => {
  vi.mocked(rpcClient.call).mockRejectedValue(new Error('offline'))
  const wrapper = mount(Fleet, { global: { stubs } })
  try {
    await flushPromises()
    expect(wrapper.text()).toContain('Fleet data is unavailable')
    expect(wrapper.find('.fleet-node-card').exists()).toBe(false)
    expect(wrapper.findAll('button').some(button => button.text() === 'Retry')).toBe(true)
  } finally { wrapper.unmount() }
})
