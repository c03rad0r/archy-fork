import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref, type Ref } from 'vue'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

vi.mock('@/api/filebrowser-client', () => ({
  fileBrowserClient: {
    login: vi.fn(),
    isAuthenticated: false,
    getUsage: vi.fn(),
    listDirectory: vi.fn(),
    readFileAsText: vi.fn(),
  },
}))

import { ContextBroker } from '../contextBroker'
import { useAIPermissionsStore } from '@/stores/aiPermissions'
import { rpcClient } from '@/api/rpc-client'
import { fileBrowserClient } from '@/api/filebrowser-client'

describe('ContextBroker', () => {
  let broker: ContextBroker
  let iframeRef: Ref<HTMLIFrameElement | null>
  let mockPostMessage: ReturnType<typeof vi.fn>

  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()

    mockPostMessage = vi.fn()
    iframeRef = ref<HTMLIFrameElement | null>({
      contentWindow: {
        postMessage: mockPostMessage,
      },
    } as unknown as HTMLIFrameElement)

    broker = new ContextBroker(iframeRef, 'http://localhost:8100')
  })

  it('creates with correct allowed origin', () => {
    expect(broker).toBeDefined()
  })

  it('start registers message listener', () => {
    const addSpy = vi.spyOn(window, 'addEventListener')
    broker.start()
    expect(addSpy).toHaveBeenCalledWith('message', expect.any(Function))
    broker.stop()
    addSpy.mockRestore()
  })

  it('stop removes message listener', () => {
    const removeSpy = vi.spyOn(window, 'removeEventListener')
    broker.start()
    broker.stop()
    expect(removeSpy).toHaveBeenCalledWith('message', expect.any(Function))
    removeSpy.mockRestore()
  })

  it('sendTheme sends theme response to iframe', () => {
    broker.sendTheme()
    expect(mockPostMessage).toHaveBeenCalledWith(
      { type: 'theme:response', theme: { accent: '#fb923c', mode: 'dark' } },
      expect.any(String),
    )
  })

  it('sendPermissionsUpdate sends enabled categories to iframe', () => {
    const perms = useAIPermissionsStore()
    perms.toggle('apps')
    perms.toggle('system')

    broker.sendPermissionsUpdate()
    expect(mockPostMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'permissions:update',
        categories: expect.arrayContaining(['apps', 'system']),
      }),
      expect.any(String),
    )
  })

  it('does not post when iframe has no contentWindow', () => {
    iframeRef.value = null
    broker.sendTheme()
    expect(mockPostMessage).not.toHaveBeenCalled()
  })

  describe('path validation', () => {
    // Access private method via prototype
    const isPathAllowed = (path: string) => {
      return (broker as unknown as { isPathAllowed: (p: string) => boolean }).isPathAllowed(path)
    }

    it('allows paths in /var/lib/archipelago/', () => {
      expect(isPathAllowed('/var/lib/archipelago/bitcoin/data.db')).toBe(true)
    })

    it('allows paths in /var/log/', () => {
      expect(isPathAllowed('/var/log/syslog')).toBe(true)
    })

    it('rejects paths outside allowed directories', () => {
      expect(isPathAllowed('/etc/shadow')).toBe(false)
      expect(isPathAllowed('/root/.ssh/id_rsa')).toBe(false)
    })

    it('rejects paths with sensitive patterns', () => {
      expect(isPathAllowed('/var/lib/archipelago/secret.key')).toBe(false)
      expect(isPathAllowed('/var/lib/archipelago/password.txt')).toBe(false)
      expect(isPathAllowed('/var/lib/archipelago/wallet.dat')).toBe(false)
      expect(isPathAllowed('/var/lib/archipelago/.env')).toBe(false)
      expect(isPathAllowed('/var/lib/archipelago/admin.macaroon')).toBe(false)
    })

    it('strips path traversal sequences before checking', () => {
      // After stripping ../, path becomes /var/lib/archipelago/etc/passwd (still in allowed dir)
      expect(isPathAllowed('/var/lib/archipelago/../../etc/passwd')).toBe(true)
      // Path outside allowed dir is rejected even without traversal
      expect(isPathAllowed('/etc/passwd')).toBe(false)
      // Sensitive pattern inside allowed dir is still blocked
      expect(isPathAllowed('/var/lib/archipelago/../../etc/password')).toBe(false)
    })
  })

  describe('log redaction', () => {
    const redact = (line: string) => {
      return (ContextBroker as unknown as { redactLogLine: (l: string) => string }).redactLogLine(line)
    }

    it('redacts password= patterns', () => {
      const result = redact('rpcpassword=mysecretpassword123')
      expect(result).not.toContain('mysecretpassword123')
      expect(result).toContain('[REDACTED]')
    })

    it('redacts token= patterns', () => {
      const result = redact('token=abc123def456')
      expect(result).not.toContain('abc123def456')
    })

    it('redacts long hex strings (private keys)', () => {
      const hexKey = 'a'.repeat(64)
      const result = redact(`key: ${hexKey}`)
      expect(result).not.toContain(hexKey)
      expect(result).toContain('[REDACTED_KEY]')
    })

    it('redacts long base64 strings (macaroons/tokens)', () => {
      const b64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/AABB'
      const result = redact(`macaroon: ${b64}`)
      expect(result).toContain('[REDACTED')
    })

    it('preserves non-sensitive log lines', () => {
      const line = '2026-03-11 INFO: Bitcoin block height: 841234'
      expect(redact(line)).toBe(line)
    })
  })

  describe('content:request', () => {
    const callContentRequest = (id: string, kind: string, scope?: string) =>
      (
        broker as unknown as {
          handleContentRequest: (id: string, kind: string, scope?: string) => Promise<void>
        }
      ).handleContentRequest(id, kind, scope)

    it('refuses with permitted:false and makes no RPC call when neither media nor files is granted', async () => {
      await callContentRequest('req-denied', 'films', 'own')

      expect(rpcClient.call).not.toHaveBeenCalled()
      expect(mockPostMessage).toHaveBeenCalledWith(
        {
          type: 'content:push',
          id: 'req-denied',
          kind: 'films',
          permitted: false,
          films: [],
          songs: [],
          podcasts: [],
          images: [],
        },
        expect.any(String),
      )
    })

    it('adapts content.list-mine results into a content:push with permitted:true when media is granted', async () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({
        items: [
          { id: 'a', filename: 'movie.mp4', mime_type: 'video/mp4', size_bytes: 10, added_at: '2026-01-01T00:00:00Z' },
        ],
      })

      await callContentRequest('req-1', 'films', 'own')

      expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'content.list-mine' }))
      expect(mockPostMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'content:push',
          id: 'req-1',
          kind: 'films',
          permitted: true,
          films: expect.arrayContaining([expect.objectContaining({ id: 'a' })]),
        }),
        expect.any(String),
      )
    })

    it('discards a stale in-flight response when a newer content:request has since started (out-of-order / AIUI-03 concurrency edge)', async () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()

      let resolveFirst: (v: { items: unknown[] }) => void = () => {}
      const firstPromise = new Promise<{ items: unknown[] }>((resolve) => {
        resolveFirst = resolve
      })

      vi.mocked(rpcClient.call)
        .mockImplementationOnce(() => firstPromise as unknown as Promise<never>)
        .mockResolvedValueOnce({
          items: [
            { id: 'fresh', filename: 'fresh.mp4', mime_type: 'video/mp4', size_bytes: 1, added_at: '2026-02-01T00:00:00Z' },
          ],
        })

      const firstCall = callContentRequest('stale-req', 'films', 'own')
      const secondCall = callContentRequest('fresh-req', 'films', 'own')
      await secondCall

      // The slow first request resolves AFTER the second one already
      // completed — its result must be discarded, not posted.
      resolveFirst({
        items: [
          { id: 'old', filename: 'old.mp4', mime_type: 'video/mp4', size_bytes: 1, added_at: '2026-01-01T00:00:00Z' },
        ],
      })
      await firstCall

      const staleCalls = mockPostMessage.mock.calls.filter(
        ([msg]) => (msg as { type?: string; id?: string }).type === 'content:push' && (msg as { id?: string }).id === 'stale-req',
      )
      expect(staleCalls).toHaveLength(0)

      const freshCalls = mockPostMessage.mock.calls.filter(
        ([msg]) => (msg as { type?: string; id?: string }).type === 'content:push' && (msg as { id?: string }).id === 'fresh-req',
      )
      expect(freshCalls).toHaveLength(1)
    })

    it('delivers BOTH results when two different kinds are requested together', async () => {
      // Regression: the guard used one counter for every kind, so requests for
      // different kinds cancelled each other. useArchy.ts init fires
      // content('all','own') then library('own') back to back and both sequence
      // numbers are assigned synchronously before either awaits — so the first
      // ALWAYS resolved stale and was dropped. Films, podcasts and own files
      // never reached the grid; only music ever did. Different kinds populate
      // different grids and cannot stale each other by definition.
      const perms = useAIPermissionsStore()
      perms.enableAll()

      vi.mocked(rpcClient.call).mockResolvedValue({ items: [], tracks: [] } as never)

      const films = callContentRequest('films-req', 'films', 'own')
      const library = callContentRequest('library-req', 'library', 'own')
      await Promise.all([films, library])

      const pushed = mockPostMessage.mock.calls
        .map(([msg]) => msg as { type?: string; id?: string })
        .filter((m) => m.type === 'content:push')
        .map((m) => m.id)

      expect(pushed).toContain('films-req')
      expect(pushed).toContain('library-req')
    })

    it('still discards a stale response within the same kind and scope', async () => {
      // The guard's real purpose must survive being made per-key.
      const perms = useAIPermissionsStore()
      perms.enableAll()
      vi.mocked(rpcClient.call).mockResolvedValue({ items: [] } as never)

      await callContentRequest('older', 'films', 'own')
      await callContentRequest('newer', 'films', 'own')

      // Both completed in order here, so both post; the ordering guarantee is
      // covered by the out-of-order test above. What this pins is that the key
      // is kind+scope, so a DIFFERENT scope does not collide with this one.
      const scoped = callContentRequest('peer-scope', 'films', 'peers')
      await scoped
      const pushed = mockPostMessage.mock.calls
        .map(([msg]) => msg as { type?: string; id?: string })
        .filter((m) => m.type === 'content:push')
        .map((m) => m.id)
      expect(pushed).toContain('peer-scope')
    })

    // 13-11: kind: 'library' is the one addition this wave makes to the
    // discriminator — it resolves to music.list-tracks, not content.*,
    // since a library track carries real tag-extracted metadata
    // (artist/album/duration) ContentItem has no field for.
    it("kind: 'library' calls music.list-tracks (not content.list-mine) and adapts the result into the songs bucket", async () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()
      vi.mocked(rpcClient.call).mockResolvedValueOnce({
        tracks: [
          {
            id: { source: 'OwnLibrary', path: '/var/lib/archipelago/filebrowser/Music/Artist/Song.flac' },
            title: 'Song',
            artist: 'Artist',
            album: 'Album',
            album_artist: 'Artist',
            duration_secs: 200,
            has_tags: true,
          },
        ],
      })

      await callContentRequest('req-lib', 'library', 'own')

      expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'music.list-tracks' }))
      expect(rpcClient.call).not.toHaveBeenCalledWith(expect.objectContaining({ method: 'content.list-mine' }))
      expect(mockPostMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'content:push',
          id: 'req-lib',
          kind: 'library',
          permitted: true,
          films: [],
          songs: expect.arrayContaining([expect.objectContaining({ title: 'Song', artist: 'Artist', album: 'Album' })]),
        }),
        expect.any(String),
      )
    })

    it("kind: 'library' degrades to an empty songs bucket (not a thrown error) when music.list-tracks fails", async () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()
      vi.mocked(rpcClient.call).mockRejectedValueOnce(new Error('no index yet'))

      await callContentRequest('req-lib-err', 'library', 'own')

      expect(mockPostMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'content:push',
          id: 'req-lib-err',
          kind: 'library',
          permitted: true,
          films: [],
          songs: [],
          podcasts: [],
          images: [],
        }),
        expect.any(String),
      )
    })
  })

  describe('adaptChatSurfaces', () => {
    const callAdapt = (surfaces: unknown) =>
      (
        broker as unknown as {
          adaptChatSurfaces: (s?: unknown) => { tool: string; scope?: string; bundle: { films: unknown[]; songs: unknown[]; podcasts: unknown[]; images: { id: string; url: string; locked: boolean }[] } }[] | undefined
        }
      ).adaptChatSurfaces(surfaces)

    // Live bug (archi-dev-box 2026-08-07): a purchased-scope chat surface
    // adapted the OwnedRpcItem wire shape as if it were ArchyContentItem —
    // id came out undefined, peerOnion was never passed, every URL was ''
    // — so three purchased images rendered as placeholders beside a correct
    // prose answer.
    it('purchased scope normalizes owned items and builds per-seller URLs', () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()
      const out = callAdapt([
        {
          tool: 'content_list',
          scope: 'purchased',
          data: {
            items: [
              {
                onion: 'peer-one.onion',
                content_id: 'cid-1',
                filename: 'signal-test.jpeg',
                mime_type: 'image/jpeg',
                size_bytes: 170000,
                paid_sats: 100,
                purchased_at: '2026-06-20T00:00:00Z',
              },
              {
                onion: 'peer-two.onion',
                content_id: 'cid-2',
                filename: 'got it!.jpg',
                mime_type: 'image/jpeg',
                size_bytes: 253000,
                paid_sats: 100,
                purchased_at: '2026-08-04T00:00:00Z',
              },
            ],
          },
        },
      ])
      expect(out).toHaveLength(1)
      const images = out![0]!.bundle.images
      expect(images).toHaveLength(2)
      // Real ids, real per-seller URLs, and already-paid items are unlocked.
      expect(images.map((i) => i.id)).toEqual(expect.arrayContaining(['cid-1', 'cid-2']))
      expect(images[0]!.url).toBe('/api/peer-content/peer-one.onion/cid-1')
      expect(images[1]!.url).toBe('/api/peer-content/peer-two.onion/cid-2')
      expect(images.every((i) => !i.locked)).toBe(true)
    })

    it('peers scope adapts per-seller so every item URL carries its own onion', () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()
      const out = callAdapt([
        {
          tool: 'content_list',
          scope: 'peers',
          data: {
            items: [
              { id: 'x', filename: 'a.jpg', mime_type: 'image/jpeg', size_bytes: 1, peer: 'seller-a.onion' },
              { id: 'y', filename: 'b.jpg', mime_type: 'image/jpeg', size_bytes: 1, peer: 'seller-b.onion' },
            ],
          },
        },
      ])
      const images = out![0]!.bundle.images
      expect(images.map((i) => i.url).sort()).toEqual([
        '/api/peer-content/seller-a.onion/x',
        '/api/peer-content/seller-b.onion/y',
      ])
    })
  })

  describe('context gathering always answers', () => {
    it('responds when the gatherer never settles — the reported files hang', async () => {
      // A File Browser that accepts the connection and then says nothing:
      // the promise stays PENDING rather than rejecting, which is precisely
      // what sanitizeFiles' try/catch cannot see. Before the timeout, no
      // context:response was ever posted and AIUI waited out its own bridge
      // timeout instead — reported as "`files` context request times out".
      vi.useFakeTimers()
      try {
        const perms = useAIPermissionsStore()
        perms.enableAll()
        ;(fileBrowserClient.login as ReturnType<typeof vi.fn>).mockReturnValue(
          new Promise(() => {}),
        )

        const pending = (
          broker as unknown as {
            handleContextRequest(id: string, category: string): Promise<void>
          }
        ).handleContextRequest('req-hang', 'files')

        await vi.advanceTimersByTimeAsync(10_000)
        await pending

        expect(mockPostMessage).toHaveBeenCalledWith(
          expect.objectContaining({
            type: 'context:response',
            id: 'req-hang',
            data: null,
            permitted: true,
          }),
          expect.anything(),
        )
      } finally {
        vi.useRealTimers()
      }
    })

    it('a healthy category still returns its data, not null', async () => {
      const perms = useAIPermissionsStore()
      perms.enableAll()

      await (
        broker as unknown as {
          handleContextRequest(id: string, category: string): Promise<void>
        }
      ).handleContextRequest('req-ok', 'apps')

      const posted = mockPostMessage.mock.calls.find(
        (c) => (c[0] as { id?: string }).id === 'req-ok',
      )
      expect(posted).toBeDefined()
      expect((posted![0] as { permitted: boolean }).permitted).toBe(true)
      expect((posted![0] as { data: unknown }).data).not.toBeNull()
    })
  })
})
