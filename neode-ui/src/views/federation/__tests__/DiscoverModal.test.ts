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

it('direct npub entry opens an existing request instead of sending again', async () => {
  vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [] })
  vi.mocked(rpcClient.handshakeConnect).mockClear()
  const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [{ id: 'existing', from_did: 'did:key:peer', from_nostr_pubkey: 'key', from_nostr_npub: 'npub1existing', from_name: null, message: null, state: 'sent', outbound: true, received_at: new Date().toISOString() }] }, global: { stubs: { Teleport: true, PeerRequestModal: true } } })
  await flushPromises()
  await wrapper.get('input[placeholder="npub1…"]').setValue(' NPUB1EXISTING ')
  await wrapper.findAll('button').find(b => b.text() === 'Send Request')!.trigger('click')
  expect(wrapper.emitted('viewRequest')).toHaveLength(1)
  expect(rpcClient.handshakeConnect).not.toHaveBeenCalled()
  wrapper.unmount()
})

it('retains the request and error on send failure and prevents concurrent sends', async () => {
  vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [] })
  const pending = deferred<Awaited<ReturnType<typeof rpcClient.handshakeConnect>>>()
  vi.mocked(rpcClient.handshakeConnect).mockReset().mockReturnValueOnce(pending.promise)
  const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [] }, global: { stubs: { Teleport: true, PeerRequestModal: true } } })
  await flushPromises()
  await wrapper.get('input[placeholder="npub1…"]').setValue('npub1new')
  await wrapper.findAll('button').find(b => b.text() === 'Send Request')!.trigger('click')
  const modal = wrapper.findComponent({ name: 'PeerRequestModal' })
  modal.vm.$emit('send', 'Please connect')
  modal.vm.$emit('send', 'Please connect')
  await flushPromises()
  expect(rpcClient.handshakeConnect).toHaveBeenCalledTimes(1)
  pending.reject(new Error('Relay unavailable'))
  await flushPromises()
  expect(modal.props('show')).toBe(true)
  expect(wrapper.text()).toContain('Relay unavailable')
  expect(wrapper.findComponent({ name: 'PeerRequestModal' }).props('error')).toBe('Relay unavailable')
  expect(wrapper.get('input[placeholder="npub1…"]').element).toHaveProperty('value', 'npub1new')
  expect(wrapper.emitted('sent')).toBeUndefined()
  wrapper.unmount()
})

it('allows a new request after an approved operation has expired without claiming connection', async () => {
  vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [makeNode()] })
  const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [{ id: 'expired', from_did: 'did:key:node', from_nostr_pubkey: 'pubkey-one', from_nostr_npub: 'npub', from_name: null, message: null, state: 'approved', outbound: true, received_at: new Date(Date.now() - 31 * 86400000).toISOString() }] }, global: { stubs: { Teleport: true, PeerRequestModal: true } } })
  await flushPromises()
  expect(wrapper.text()).not.toContain('View request')
  const buttons = wrapper.findAll('button').filter(b => b.text() === 'Send Request')
  await buttons[buttons.length - 1]!.trigger('click')
  expect(wrapper.findComponent({ name: 'PeerRequestModal' }).props('show')).toBe(true)
  expect(wrapper.emitted('sent')).toBeUndefined()
  wrapper.unmount()
})

it('preserves an approval with unknown age rather than allowing a duplicate request', async () => {
  vi.mocked(rpcClient.handshakeDiscover).mockResolvedValueOnce({ nodes: [makeNode()] })
  const wrapper = mount(DiscoverModal, { props: { visible: true, inline: true, outboundSent: [{ id: 'unknown-age', from_did: 'did:key:node', from_nostr_pubkey: 'pubkey-one', from_nostr_npub: 'npub', from_name: null, message: null, state: 'approved', outbound: true, received_at: 'unavailable' }] }, global: { stubs: { Teleport: true, PeerRequestModal: true } } })
  await flushPromises()
  await wrapper.findAll('button').find(b => b.text() === 'View request')!.trigger('click')
  expect(wrapper.emitted('viewRequest')).toHaveLength(1)
  expect(wrapper.findComponent({ name: 'PeerRequestModal' }).props('show')).toBe(false)
  wrapper.unmount()
})
