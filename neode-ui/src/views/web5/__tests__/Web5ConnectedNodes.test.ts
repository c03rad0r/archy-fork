import { mount, flushPromises, enableAutoUnmount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Web5ConnectedNodes from '../Web5ConnectedNodes.vue'
import { rpcClient } from '@/api/rpc-client'

enableAutoUnmount(afterEach)

const pushRoute = vi.hoisted(() => vi.fn())
vi.mock('vue-router', () => ({
  useRouter: () => ({ push: pushRoute }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    listPeers: vi.fn(() => new Promise(() => {})),
    federationListNodes: vi.fn().mockResolvedValue({ nodes: [] }),
    federationListPendingRequests: vi.fn().mockResolvedValue({ requests: [] }),
    checkPeerReachable: vi.fn().mockResolvedValue({ reachable: false }),
    call: vi.fn(),
  },
}))

vi.mock('@/composables/useMessageToast', () => ({
  useMessageToast: () => ({
    loadReceivedMessages: vi.fn(),
  }),
}))

vi.mock('@/stores/web5Badge', () => ({
  useWeb5BadgeStore: () => ({ pendingRequestCount: 0 }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ peerHealth: {} }),
}))

vi.mock('@/composables/useModalKeyboard', () => ({
  useModalKeyboard: vi.fn(),
}))

describe('Web5ConnectedNodes', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    sessionStorage.clear()
  })

  it('puts the three federation destinations and icon refresh in the bottom bar', async () => {
    const wrapper = mount(Web5ConnectedNodes)
    const nav = wrapper.get('nav[aria-label="Node connections"]')
    expect(nav.text()).toContain('My connections')
    expect(nav.text()).toContain('Find Nodes')
    expect(nav.text()).toContain('Map')
    expect(nav.text()).not.toContain('Network Map')
    const mobilePrimaryRow = nav.get('div.grid.grid-cols-2')
    expect(mobilePrimaryRow.findAll('button')).toHaveLength(2)
    expect(mobilePrimaryRow.findAll('button').every(button => button.classes().includes('w-full'))).toBe(true)
    await nav.findAll('button')[0]!.trigger('click')
    await nav.findAll('button')[1]!.trigger('click')
    await nav.findAll('button')[2]!.trigger('click')
    expect(pushRoute.mock.calls.map(([route]) => route.query.view)).toEqual(['connected', 'discover', 'map'])
    expect(nav.findAll('button')[3]!.attributes('aria-label')).toBe('Refresh connected nodes')
  })

  it('shows a loading state for empty trusted nodes while peers are loading', async () => {
    const wrapper = mount(Web5ConnectedNodes)

    wrapper.vm.loadPeers()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('common.loading')
    expect(wrapper.text()).not.toContain('web5.noPeers')
  })

  it('keeps connection requests visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      requests: [{
        id: 'req-1',
        from_did: 'did:key:alice',
        from_pubkey: 'alice-pubkey',
        message: 'Please connect',
        created_at: '2026-06-10T10:00:00Z',
      }],
    })

    const wrapper = mount(Web5ConnectedNodes)
    await (wrapper.vm as unknown as { loadConnectionRequests: () => Promise<void> }).loadConnectionRequests()

    expect(wrapper.text()).toContain('Please connect')

    vi.mocked(rpcClient.call).mockReturnValueOnce(new Promise((_, reject) => {
      setTimeout(() => reject(new Error('offline')), 0)
    }))

    const refresh = (wrapper.vm as unknown as { loadConnectionRequests: () => Promise<void> }).loadConnectionRequests()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Please connect')
    expect(wrapper.text()).toContain('common.loading')

    await refresh

    expect(wrapper.text()).toContain('Please connect')
  })
  it('shows Nostr requests beside legacy requests without requiring discovery first', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ requests: [] })
    vi.mocked(rpcClient.federationListPendingRequests).mockResolvedValueOnce({ requests: [{
      id: 'nostr-1', from_did: 'did:key:yaya', from_name: 'Yaya', from_nostr_npub: 'npub1yaya',
      from_nostr_pubkey: 'abc', message: null, received_at: new Date().toISOString(), state: 'approved', outbound: false,
    }] })
    const wrapper = mount(Web5ConnectedNodes)
    await wrapper.vm.loadConnectionRequests()
    expect(wrapper.text()).toContain('Yaya')
    expect(wrapper.text()).toContain('Approved — connecting')
    expect(wrapper.text()).not.toContain('web5.noRequests')
  })

  it('renders direct peers while federation is slow and distinguishes probe failure from unreachable', async () => {
    vi.mocked(rpcClient.listPeers).mockResolvedValueOnce({ peers: [
      { onion: 'online.onion', pubkey: 'online', name: 'Available node' },
      { onion: 'failed.onion', pubkey: 'failed', name: 'Query failed' },
      { onion: 'offline.onion', pubkey: 'offline', name: 'No route' },
    ] })
    let finish!: (value: { nodes: [] }) => void
    vi.mocked(rpcClient.federationListNodes).mockReturnValueOnce(new Promise(resolve => { finish = resolve }))
    vi.mocked(rpcClient.checkPeerReachable).mockImplementation(async onion => {
      if (onion === 'failed.onion') throw new Error('unauthorized')
      return { onion, reachable: onion === 'online.onion' }
    })
    const wrapper = mount(Web5ConnectedNodes)
    const load = wrapper.vm.loadPeers()
    await flushPromises()
    expect(wrapper.text()).toContain('Available node')
    expect(wrapper.text()).toContain('Status unknown')
    finish({ nodes: [] })
    await load
    await flushPromises()
    expect(wrapper.text()).toContain('Online · Last seen just now')
    expect(wrapper.text()).toContain('Unreachable · No confirmed contact')
    expect(wrapper.text()).toContain('Status unknown · No confirmed contact')
    expect(wrapper.text()).not.toContain('Offline for')
  })

})
