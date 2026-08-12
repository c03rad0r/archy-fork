import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import Web5CredentialsSummary from '../Web5CredentialsSummary.vue'
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

function makeCredential() {
  return {
    id: 'cred-one',
    issuer: 'did:key:issuer',
    subject: 'did:key:subject',
    type: 'NodeOperator',
    claims: {},
    issued_at: '2026-06-10T10:00:00Z',
    expires_at: null,
    status: 'active',
  }
}

describe('Web5CredentialsSummary', () => {
  it('keeps credential rows visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ credentials: [makeCredential()] })

    const wrapper = mount(Web5CredentialsSummary, {
      props: { identityCount: 1 },
      global: {
        stubs: {
          RouterLink: true,
        },
      },
    })

    await (wrapper.vm as unknown as { loadCredentials: () => Promise<void> }).loadCredentials()
    await flushPromises()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('did:key:subject')

    const pending = deferred<{ credentials: [] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadCredentials: () => Promise<void> }).loadCredentials()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('Refreshing credentials...')
    expect(wrapper.text()).not.toContain('Loading credentials...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('NodeOperator')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing credentials...')
  })
})
