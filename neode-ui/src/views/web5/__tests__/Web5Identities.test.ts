import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import Web5Identities from '../Web5Identities.vue'
import { rpcClient } from '@/api/rpc-client'
import type { ManagedIdentity } from '../types'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

vi.mock('../utils', () => ({
  safeClipboardWrite: vi.fn(),
}))

function makeIdentity(name: string): ManagedIdentity {
  return {
    id: name,
    name,
    purpose: 'personal',
    pubkey: `${name}-pubkey`,
    did: `did:key:${name}`,
    created_at: '2026-06-10T10:00:00Z',
    is_default: true,
    profile: {},
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe('Web5Identities', () => {
  it('keeps identities visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      identities: [makeIdentity('Personal')],
    })

    const wrapper = mount(Web5Identities, {
      props: { showStagger: false },
    })

    await (wrapper.vm as unknown as { loadIdentities: () => Promise<void> }).loadIdentities()
    await flushPromises()

    expect(wrapper.text()).toContain('Personal')

    const pending = deferred<{ identities: ManagedIdentity[] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadIdentities: () => Promise<void> }).loadIdentities()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Personal')
    expect(wrapper.text()).toContain('Refreshing identities...')
    expect(wrapper.text()).not.toContain('common.loading')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Personal')
    expect(wrapper.text()).not.toContain('Refreshing identities...')
  })

  it('turns a saved profile into the identity success screen with honest relay coverage', async () => {
    const identity = makeIdentity('Personal')
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'identity.update-profile') return Promise.resolve({})
      if (request.method === 'identity.list') return Promise.resolve({ identities: [identity] })
      if (request.method === 'identity.publish-profile') {
        return Promise.resolve({
          event_id: 'event-123', accepted: ['wss://one'],
          rejected: [['wss://two', 'write denied']], relays_attempted: 2, published: true,
        })
      }
      return Promise.resolve({})
    })
    const wrapper = mount(Web5Identities, {
      props: { showStagger: false },
      global: { stubs: { Teleport: true } },
    })
    const vm = wrapper.vm as unknown as {
      openProfileEditor: (identity: ManagedIdentity) => void
      publishProfile: () => Promise<void>
    }
    vm.openProfileEditor(identity)
    await wrapper.vm.$nextTick()
    await vm.publishProfile()
    await flushPromises()

    expect(wrapper.text()).toContain('IDENTITY UPDATED')
    expect(wrapper.text()).toContain('Published to 1/2 configured relays.')
    expect(wrapper.text()).toContain('wss://two rejected the event: write denied')
    expect(wrapper.text()).toContain('event-123')
  })
})
