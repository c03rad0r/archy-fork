import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import Web5Domains from '../Web5Domains.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
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

function makeDomain() {
  return {
    id: 'name-one',
    name: 'satoshi',
    domain: 'example.com',
    nip05: 'satoshi@example.com',
    identity_id: 'identity-one',
    did: 'did:key:identity',
    nostr_pubkey: null,
    status: 'active',
    registered_at: '2026-06-10T10:00:00Z',
    expires_at: null,
  }
}

describe('Web5Domains', () => {
  it('keeps registered names visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ names: [makeDomain()] })

    const wrapper = mount(Web5Domains, {
      props: { showStagger: false, managedIdentities: [] },
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })

    await (wrapper.vm as unknown as { loadDomainNames: () => Promise<void> }).loadDomainNames()
    await flushPromises()

    expect(wrapper.text()).toContain('1 name')
    expect(wrapper.text()).toContain('1 Active')

    const pending = deferred<{ names: [] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadDomainNames: () => Promise<void> }).loadDomainNames()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('1 name')
    expect(wrapper.text()).toContain('1 Active')
    expect(wrapper.text()).toContain('Refreshing domains...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('1 name')
    expect(wrapper.text()).toContain('1 Active')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing domains...')
  })
})
