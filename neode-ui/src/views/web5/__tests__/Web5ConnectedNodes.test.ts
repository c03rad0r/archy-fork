import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import Web5ConnectedNodes from '../Web5ConnectedNodes.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    listPeers: vi.fn(() => new Promise(() => {})),
    federationListNodes: vi.fn().mockResolvedValue({ nodes: [] }),
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
})
