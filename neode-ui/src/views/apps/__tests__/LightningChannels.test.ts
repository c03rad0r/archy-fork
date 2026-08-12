import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import LightningChannels from '@/components/LightningChannelsPanel.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn() }),
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

function makeChannel() {
  return {
    chan_id: '123',
    remote_pubkey: 'peer-pubkey',
    capacity: 100_000,
    local_balance: 60_000,
    remote_balance: 40_000,
    active: true,
    status: 'active',
    channel_point: 'txid:0',
  }
}

describe('LightningChannels', () => {
  it('keeps channels visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce({
      channels: [makeChannel()],
      total_inbound: 40_000,
      total_outbound: 60_000,
    })

    // The panel's setup pulls a Pinia store via useTxExplorer — mount with a
    // fresh Pinia or setup throws before the first render.
    const wrapper = mount(LightningChannels, {
      global: { plugins: [createPinia()] },
    })
    await flushPromises()

    expect(wrapper.text()).toContain('peer-pubkey')
    expect(wrapper.text()).toContain('100.0k sats')

    const pending = deferred<{ channels: []; total_inbound: number; total_outbound: number }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadChannels: () => Promise<void> }).loadChannels()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('peer-pubkey')
    expect(wrapper.text()).toContain('Refreshing channels...')
    expect(wrapper.text()).not.toContain('Loading channels...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('peer-pubkey')
    expect(wrapper.text()).toContain('offline')
  })
})
