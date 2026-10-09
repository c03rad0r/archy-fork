import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import PeerFiles from '../PeerFiles.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('vue-router', () => ({
  RouterLink: { template: '<a><slot /></a>' },
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    federationListNodes: vi.fn(),
  },
}))

vi.mock('@/composables/useAudioPlayer', () => ({
  useAudioPlayer: () => ({ play: vi.fn() }),
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

function makeCatalogItem() {
  return {
    id: 'file-1',
    filename: 'notes.txt',
    mime_type: 'text/plain',
    size_bytes: 128,
    description: '',
    access: 'free',
  }
}

describe('PeerFiles', () => {
  it('keeps peer catalog items visible while refresh is pending or fails', async () => {
    vi.mocked(rpcClient.federationListNodes).mockResolvedValue({
      nodes: [{
        did: 'did:key:peer',
        pubkey: 'peer',
        onion: 'peer.onion',
        name: 'Peer',
        trust_level: 'trusted',
      }],
    } as never)
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ items: [makeCatalogItem()] })

    const wrapper = mount(PeerFiles, {
      props: { peerId: 'peer.onion' },
      global: {
        // The shared peer-browse cache lives in the Pinia resources store.
        plugins: [createPinia()],
        stubs: {
          Teleport: true,
        },
      },
    })
    await flushPromises()

    expect(wrapper.text()).toContain('notes.txt')

    const pending = deferred<{ items: [] }>()
    vi.mocked(rpcClient.call).mockReturnValueOnce(pending.promise)

    const refresh = (wrapper.vm as unknown as { loadCatalog: () => Promise<void> }).loadCatalog()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('notes.txt')
    expect(wrapper.text()).toContain('Refreshing peer files...')
    expect(wrapper.text()).not.toContain('Connecting via Tor')

    pending.reject(new Error('offline'))
    await refresh
    await flushPromises()

    expect(wrapper.text()).toContain('notes.txt')
    expect(wrapper.text()).toContain('offline')
    expect(wrapper.text()).not.toContain('Refreshing peer files...')
  })

  it('opens the full-screen lightbox when a FREE image card is clicked', async () => {
    vi.mocked(rpcClient.federationListNodes).mockResolvedValue({
      nodes: [{
        did: 'did:key:peer',
        pubkey: 'peer',
        onion: 'peer.onion',
        name: 'Peer',
        trust_level: 'trusted',
      }],
    } as never)
    const freeImage = {
      id: 'photo-1',
      filename: 'sunset.jpg',
      mime_type: 'image/jpeg',
      size_bytes: 1024,
      description: '',
      access: 'free',
    }
    vi.mocked(rpcClient.call).mockImplementation((async (req: { method: string }) => {
      if (req.method === 'content.browse-peer') return { items: [freeImage] }
      if (req.method === 'content.owned-list') return { items: [], attempts: [] }
      return {}
    }) as never)

    const wrapper = mount(PeerFiles, {
      props: { peerId: 'peer.onion' },
      global: {
        plugins: [createPinia()],
        stubs: {
          Teleport: true,
        },
      },
    })
    await flushPromises()

    expect(wrapper.text()).toContain('sunset.jpg')

    // The free image card click was previously a no-op (the old ternary fell
    // through to `undefined` for non-playable free items).
    await wrapper.find('.aspect-video').trigger('click')
    await flushPromises()

    const viewerImg = wrapper
      .findAll('img')
      .find(img => (img.attributes('src') || '').includes('/api/peer-content/'))
    expect(viewerImg).toBeTruthy()
    expect(viewerImg!.attributes('src')).toContain('photo-1')
    expect(wrapper.text()).toContain('Free · shared by peer')
  })
})
