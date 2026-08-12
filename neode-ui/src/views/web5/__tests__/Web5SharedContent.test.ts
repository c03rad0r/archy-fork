import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import Web5SharedContent from '../Web5SharedContent.vue'
import { rpcClient } from '@/api/rpc-client'
import type { ContentItemData, PeerContentItem } from '../types'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

function makePeerItem(filename: string): PeerContentItem {
  return {
    id: filename,
    filename,
    mime_type: 'text/plain',
    size_bytes: 128,
    description: '',
    access: 'free',
  }
}

function makeContentItem(filename: string): ContentItemData {
  return {
    ...makePeerItem(filename),
    added_at: '2026-06-10T10:00:00Z',
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

function getButtonByText(wrapper: ReturnType<typeof mount>, text: string) {
  const button = wrapper.findAll('button').find((candidate) => candidate.text().trim() === text)
  if (!button) throw new Error(`Button not found: ${text}`)
  return button
}

describe('Web5SharedContent', () => {
  it('keeps my content visible while a refresh is pending or fails', async () => {
    vi.mocked(rpcClient.call)
      .mockResolvedValueOnce({ items: [makeContentItem('notes.txt')] })

    const wrapper = mount(Web5SharedContent, {
      props: {
        showStagger: false,
        peers: [],
      },
    })

    await (wrapper.vm as unknown as { loadContentItems: () => Promise<void> }).loadContentItems()
    await flushPromises()

    expect(wrapper.text()).toContain('notes.txt')

    const pending = deferred<{ items: ContentItemData[] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadContentItems: () => Promise<void> }).loadContentItems()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('notes.txt')
    expect(wrapper.text()).toContain('Refreshing shared content...')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('notes.txt')
    expect(wrapper.text()).not.toContain('Refreshing shared content...')
  })

  it('keeps peer content visible while refreshing the same peer tab', async () => {
    vi.mocked(rpcClient.call)
      .mockResolvedValueOnce({ items: [makePeerItem('episode-one.txt')] })

    const wrapper = mount(Web5SharedContent, {
      props: {
        showStagger: false,
        peers: [{ onion: 'peer-a.onion', pubkey: 'peer-a', name: 'Peer A' }],
      },
    })

    await getButtonByText(wrapper, 'web5.browsePeers').trigger('click')
    await wrapper.get('select').setValue('peer-a.onion')
    await getButtonByText(wrapper, 'web5.browse').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('episode-one.txt')

    const pending = deferred<{ items: PeerContentItem[] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    await getButtonByText(wrapper, 'web5.browse').trigger('click')
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('episode-one.txt')
    expect(wrapper.text()).toContain('Refreshing peer content...')

    pending.resolve({ items: [makePeerItem('episode-two.txt')] })
    await flushPromises()

    expect(wrapper.text()).toContain('episode-two.txt')
    expect(wrapper.text()).not.toContain('Refreshing peer content...')
  })
})
