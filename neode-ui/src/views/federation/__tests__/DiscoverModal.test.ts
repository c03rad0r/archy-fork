import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import DiscoverModal from '../DiscoverModal.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    handshakeDiscover: vi.fn(),
    handshakeConnect: vi.fn(),
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

function makeNode() {
  return {
    nostr_pubkey: 'pubkey-one',
    nostr_npub: 'npub1abcdefghijklmnopqrstuvwxyz',
    did: 'did:key:node',
    version: '1.8-alpha',
  }
}

describe('DiscoverModal', () => {
  it('keeps discoverable nodes visible while relay refresh is pending or fails', async () => {
    vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [makeNode()] })

    const wrapper = mount(DiscoverModal, {
      props: { visible: false, outboundSent: [] },
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })
    await wrapper.setProps({ visible: true })
    await flushPromises()

    expect(wrapper.text()).toContain('did:key:node')
    expect(wrapper.text()).toContain('version 1.8-alpha')

    const pending = deferred<{ nodes: [] }>()
    vi.mocked(rpcClient.handshakeDiscover).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { refresh: () => Promise<void> }).refresh()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('did:key:node')
    expect(wrapper.text()).toContain('Searching relays...')
    expect(wrapper.text()).not.toContain('No discoverable nodes found')

    pending.reject(new Error('relay offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('did:key:node')
    expect(wrapper.text()).toContain('relay offline')
    expect(wrapper.text()).not.toContain('Searching relays...')
    expect(wrapper.text()).not.toContain('No discoverable nodes found')
  })
})
