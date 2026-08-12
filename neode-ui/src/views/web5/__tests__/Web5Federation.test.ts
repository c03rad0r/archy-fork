import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import Web5Federation from '../Web5Federation.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn().mockResolvedValue({ nodes: [], pending_requests: [] }),
    getNodeDid: vi.fn().mockResolvedValue({ did: 'did:key:test' }),
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

function mountFederation() {
  return mount(Web5Federation, {
    global: {
      stubs: {
        RouterLink: {
          props: ['to'],
          template: '<a :href="to"><slot /></a>',
        },
      },
    },
  })
}

describe('Web5Federation', () => {
  it('surfaces Find Nodes and Fleet routes', () => {
    const wrapper = mountFederation()

    const links = wrapper.findAll('a').map(link => link.attributes('href'))
    expect(links).toContain('/dashboard/server/federation')
    expect(links).toContain('/dashboard/fleet')
    expect(wrapper.text()).toContain('Find Nodes')
    expect(wrapper.text()).toContain('Fleet')
  })

  it('shows federation refresh state without replacing existing counts', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      nodes: [{ status: 'online' }, { status: 'offline' }],
      pending_requests: [{}],
    })
    vi.mocked(rpcClient.getNodeDid).mockResolvedValueOnce({ did: 'did:key:node', pubkey: 'node-pubkey' })

    const wrapper = mountFederation()
    await flushPromises()

    expect(wrapper.text()).toContain('Known Nodes')
    expect(wrapper.text()).toContain('2')
    expect(wrapper.text()).toContain('did:key:node')

    const pending = deferred<{ nodes: []; pending_requests: [] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)
    vi.mocked(rpcClient.getNodeDid).mockResolvedValueOnce({ did: 'did:key:node', pubkey: 'node-pubkey' })

    const refresh = (wrapper.vm as unknown as { loadFederationSummary: () => Promise<void> }).loadFederationSummary()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('2')
    expect(wrapper.text()).toContain('did:key:node')
    expect(wrapper.text()).toContain('Refreshing federation...')
    expect(wrapper.text()).not.toContain('Loading federation...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('2')
    expect(wrapper.text()).toContain('did:key:node')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing federation...')
  })
})
