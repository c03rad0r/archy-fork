import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import Credentials from '../Credentials.vue'
import { rpcClient } from '@/api/rpc-client'

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

function makeCredential(id: string) {
  return {
    id,
    type: ['VerifiableCredential', 'NodeOperator'],
    issuer: 'did:key:issuer',
    credentialSubject: { id: 'did:key:subject' },
    issuanceDate: '2026-06-10T10:00:00Z',
    status: 'active',
  }
}

describe('Credentials', () => {
  it('keeps credentials visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'identity.list') return Promise.resolve({ identities: [] })
      if (request.method === 'identity.list-credentials') {
        return Promise.resolve({ credentials: [makeCredential('cred-one')] })
      }
      return Promise.resolve({})
    })

    const wrapper = mount(Credentials, {
      global: {
        // The cached-resource layer pulls the Pinia resources store in setup.
        plugins: [createPinia()],
        mocks: {
          $router: { push: vi.fn() },
        },
      },
    })
    await flushPromises()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('cred-one')

    const pending = deferred<{ credentials: [] }>()
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'identity.list-credentials') return pending.promise
      return Promise.resolve({ identities: [] })
    })

    const refresh = (wrapper.vm as unknown as { loadCredentials: () => Promise<void> }).loadCredentials()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('cred-one')
    expect(wrapper.text()).toContain('Refreshing credentials...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('cred-one')
    expect(wrapper.text()).not.toContain('Refreshing credentials...')
  })
})
