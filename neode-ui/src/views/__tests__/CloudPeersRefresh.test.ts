import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'
import Cloud from '../Cloud.vue'
import { rpcClient } from '@/api/rpc-client'

const routeFixture = vi.hoisted(() => ({ name: 'cloud', query: {} as Record<string, string> }))

vi.mock('vue-router', () => ({
  useRoute: () => routeFixture,
  useRouter: () => ({ push: vi.fn() }),
  RouterLink: { name: 'RouterLink', props: ['to'], template: '<a><slot /></a>' },
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ packages: {} }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    federationListNodes: vi.fn(),
    call: vi.fn().mockResolvedValue({ items: [] }),
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

function makePeer() {
  return {
    did: 'did:key:peer',
    pubkey: 'peer',
    onion: 'peer.onion',
    name: 'Peer Alpha',
    trust_level: 'trusted',
    added_at: '2026-06-10T10:00:00Z',
  }
}

describe('Cloud peer list', () => {
  it('keeps peer nodes visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.federationListNodes).mockResolvedValueOnce({ nodes: [makePeer()] })

    const wrapper = mount(Cloud, { global: { plugins: [createPinia()] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Peer Alpha')
    expect(wrapper.text()).not.toContain('No peers yet')

    const pending = deferred<{ nodes: [] }>()
    vi.mocked(rpcClient.federationListNodes).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadPeers: () => Promise<void> }).loadPeers()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Peer Alpha')
    expect(wrapper.text()).toContain('Refreshing peer nodes...')
    expect(wrapper.text()).not.toContain('No peers yet')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Peer Alpha')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing peer nodes...')
    expect(wrapper.text()).not.toContain('No peers yet')
  })
})

it('loads paid files once when returning directly to the selected paid tab', async () => {
  routeFixture.query = { tab: 'paid', category: 'music' }
  vi.mocked(rpcClient.federationListNodes).mockResolvedValue({ nodes: [] })
  vi.mocked(rpcClient.call).mockClear().mockResolvedValue({ items: [] })
  const wrapper = mount(Cloud, { global: { plugins: [createPinia()] } })
  await flushPromises()
  const ownedCalls = vi.mocked(rpcClient.call).mock.calls.filter(([call]) => call.method === 'content.owned-list')
  expect(ownedCalls).toHaveLength(1)
  expect(wrapper.text()).toContain('Nothing purchased yet')
  wrapper.unmount()
  routeFixture.query = {}
})
