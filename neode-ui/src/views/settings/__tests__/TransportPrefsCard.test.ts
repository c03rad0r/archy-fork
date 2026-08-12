import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import TransportPrefsCard from '../TransportPrefsCard.vue'
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

function makePrefs() {
  return {
    federation: 'auto',
    peers: 'fips',
    peer_files: 'tor',
    messaging: 'auto',
    mesh_file_sharing: 'fips',
  }
}

describe('TransportPrefsCard', () => {
  it('keeps transport preferences visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call).mockResolvedValueOnce(makePrefs())

    const wrapper = mount(TransportPrefsCard)
    await flushPromises()

    expect(wrapper.text()).toContain('Federation')
    expect(wrapper.text()).toContain('Peer Files')
    expect(wrapper.text()).toContain('Auto')
    expect(wrapper.text()).toContain('FIPS')
    expect(wrapper.text()).toContain('Tor')

    const pending = deferred<ReturnType<typeof makePrefs>>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { load: () => Promise<void> }).load()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Federation')
    expect(wrapper.text()).toContain('Peer Files')
    expect(wrapper.text()).toContain('Refreshing transport preferences...')
    expect(wrapper.text()).not.toContain('Loading…')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('Federation')
    expect(wrapper.text()).toContain('Peer Files')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing transport preferences...')
  })
})
