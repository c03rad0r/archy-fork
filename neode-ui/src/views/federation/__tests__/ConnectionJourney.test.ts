import { flushPromises, shallowMount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import Federation from '../../Federation.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/stores/transport', () => ({ useTransportStore: () => ({ fetchPeers: vi.fn() }) }))
vi.mock('@/stores/app', () => ({ useAppStore: () => ({ serverName: 'Fixture' }) }))
vi.mock('@/stores/sync', () => ({ useSyncStore: () => ({ serverInfo: null }) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: {
  call: vi.fn().mockResolvedValue({}), getNodeDid: vi.fn().mockResolvedValue({ did: 'did:key:self' }),
  nostrDiscoveryStatus: vi.fn().mockResolvedValue({ enabled: false }),
  federationListPendingRequests: vi.fn().mockResolvedValue({ requests: [] }),
} }))
vi.mock('@/composables/useCachedResource', async () => {
  const { ref } = await import('vue')
  return { useCachedResource: () => ({ data: ref([]), loadState: ref('ready'), error: ref(null), refresh: vi.fn() }) }
})
afterEach(() => { vi.useRealTimers() })

describe('connection journey navigation', () => {
  it('renders all tabs with zero peers, accepts deep links and reacts to navigation', async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: Federation }] })
    await router.push('/?view=requests')
    await router.isReady()
    const wrapper = shallowMount(Federation, { global: { plugins: [router] } })
    await flushPromises()
    const selected = () => wrapper.get('[role="tab"][aria-selected="true"]').text()
    expect(selected()).toBe('Requests')
    expect(wrapper.findAll('[role="tab"]').map(t => t.text())).toEqual(['Discover', 'Requests', 'Connected', 'Network Map'])
    expect(wrapper.findComponent({ name: 'PendingRequestsPanel' }).props('showEmpty')).toBe(true)
    await wrapper.findAll('[role="tab"]')[2]!.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.view).toBe('connected')
    await router.push('/?view=discover')
    await flushPromises()
    expect(selected()).toBe('Discover')
    await router.push('/?view=list')
    await flushPromises()
    expect(selected()).toBe('Connected')
    wrapper.unmount()
  })
})

it('does not start a poll timer after navigating away during initial identity loading', async () => {
  let resolve!: (value: Awaited<ReturnType<typeof rpcClient.getNodeDid>>) => void
  vi.mocked(rpcClient.getNodeDid).mockReturnValueOnce(new Promise(done => { resolve = done }))
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: Federation }] })
  await router.push('/')
  const timer = vi.spyOn(globalThis, 'setInterval')
  const wrapper = shallowMount(Federation, { global: { plugins: [router] } })
  wrapper.unmount()
  resolve({ did: 'did:key:late', pubkey: 'fixture' })
  await flushPromises()
  expect(timer).not.toHaveBeenCalled()
  timer.mockRestore()
})
