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

describe('inline connection discovery', () => {
  it('loads on initial entry and preserves local search across tab switches', async () => {
    vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [makeNode(), { ...makeNode(), nostr_pubkey: 'two', did: 'did:key:other' }] })
    const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [] }, global: { stubs: { Teleport: true } } })
    await flushPromises()
    const search = wrapper.get('input[aria-label="Search discovered nodes"]')
    await search.setValue('other')
    expect(wrapper.text()).toContain('did:key:other')
    expect(wrapper.text()).not.toContain('did:key:node')
    await wrapper.setProps({ visible: false })
    await wrapper.setProps({ visible: true })
    expect(wrapper.get('input[aria-label="Search discovered nodes"]').element).toHaveProperty('value', 'other')
    expect(wrapper.text()).toContain('does not confirm')
    wrapper.unmount()
  })

  it('ignores a late search from a departed tab', async () => {
    const old = deferred<{ nodes: ReturnType<typeof makeNode>[] }>()
    vi.mocked(rpcClient.handshakeDiscover).mockReturnValueOnce(old.promise).mockResolvedValueOnce({ nodes: [{ ...makeNode(), did: 'did:key:fresh' }] })
    const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [] }, global: { stubs: { Teleport: true } } })
    await wrapper.setProps({ visible: false })
    await wrapper.setProps({ visible: true })
    await flushPromises()
    old.resolve({ nodes: [makeNode()] })
    await flushPromises()
    expect(wrapper.text()).toContain('did:key:fresh')
    expect(wrapper.text()).not.toContain('did:key:node')
    wrapper.unmount()
  })

  it('routes existing requests and peers without sending another request or claiming confirmation', async () => {
    vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [makeNode()] })
    const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [{ id: 'r', from_did: 'did:key:node', from_nostr_pubkey: 'pubkey-one', from_nostr_npub: 'npub', from_name: null, message: null, state: 'approved', outbound: true, received_at: new Date().toISOString() }] }, global: { stubs: { Teleport: true } } })
    await flushPromises()
    await wrapper.findAll('button').find(b => b.text() === 'View request')!.trigger('click')
    expect(wrapper.emitted('viewRequest')).toHaveLength(1)
    await wrapper.setProps({ peerDids: ['did:key:node'] })
    await wrapper.findAll('button').find(b => b.text() === 'Open node')!.trigger('click')
    expect(wrapper.emitted('openNode')).toEqual([['did:key:node']])
    expect(wrapper.text()).not.toContain('Connected')
    wrapper.unmount()
  })
})
