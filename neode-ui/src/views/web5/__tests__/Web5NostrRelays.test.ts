import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import Web5NostrRelays from '../Web5NostrRelays.vue'
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

function mockRelayCalls() {
  vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
    if (request.method === 'nostr.list-relays') {
      return Promise.resolve({
        relays: [{ url: 'wss://relay.example', connected: true, enabled: true, added_at: '2026-06-10T10:00:00Z' }],
      })
    }
    if (request.method === 'nostr.get-stats') {
      return Promise.resolve({ connected_count: 2, total_relays: 3, enabled_count: 1 })
    }
    return Promise.resolve({})
  })
}

describe('Web5NostrRelays', () => {
  it('keeps relay stats visible while refresh is pending or fails', async () => {
    mockRelayCalls()

    const wrapper = mount(Web5NostrRelays, {
      props: { showStagger: false },
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })

    await (wrapper.vm as unknown as { loadNostrRelays: () => Promise<void> }).loadNostrRelays()
    await flushPromises()

    expect(wrapper.text()).toContain('2 active')
    expect(wrapper.text()).toContain('3 configured')

    const pending = deferred<{ relays: [] }>()
    vi.mocked(rpcClient.call).mockImplementation((request: { method: string }) => {
      if (request.method === 'nostr.list-relays') return pending.promise
      if (request.method === 'nostr.get-stats') {
        return Promise.resolve({ connected_count: 0, total_relays: 0, enabled_count: 0 })
      }
      return Promise.resolve({})
    })

    const refresh = (wrapper.vm as unknown as { loadNostrRelays: () => Promise<void> }).loadNostrRelays()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('2 active')
    expect(wrapper.text()).toContain('3 configured')
    expect(wrapper.text()).toContain('Refreshing relays...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('2 active')
    expect(wrapper.text()).toContain('3 configured')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing relays...')
  })
})
