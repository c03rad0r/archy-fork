import { beforeEach, afterEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import ShareModal from '../ShareModal.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
beforeEach(() => { vi.clearAllMocks(); vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] }) })
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers() })

it.each([false, true])('publishes one complete policy for an existing share: %s', async existing => {
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'content.list-mine') return { items: existing ? [{ id: 'saved-id', filename: 'film.mp4', access: 'free' }] : [] }
    return { item: { id: 'new-id' } }
  })
  const wrapper = mount(ShareModal, { props: { filename: 'film.mp4', filepath: '/film.mp4', isDir: false } })
  await flushPromises()
  const vm = (wrapper.vm as any).$.setupState
  vm.shared = true; vm.accessType = 'paid'; vm.priceSats = 1; vm.acceptedSet = new Set(['ecash'])
  await vm.save()
  const writes = vi.mocked(rpcClient.call).mock.calls.map(([call]) => call).filter(call => ['content.publish','content.configure','content.set-pricing','content.set-availability'].includes(call.method))
  expect(writes).toHaveLength(1)
  expect(writes[0]?.method).toBe(existing ? 'content.configure' : 'content.publish')
  expect(writes[0]?.params).toMatchObject({ access: 'paid', price_sats: 1, accepted_methods: ['ecash'], availability: 'all_peers' })
  expect(writes[0]?.params).toMatchObject(existing ? { id: 'saved-id' } : { filename: 'film.mp4' })
  wrapper.unmount()
})
