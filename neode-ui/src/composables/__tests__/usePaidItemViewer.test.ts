import { describe, it, expect, vi, beforeEach } from 'vitest'

// ── Module boundary stubs (per plan: jsdom has no real blob decoding —
// assert on what was requested and what was routed where, not byte content) ──
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

const playMock = vi.fn()
vi.mock('../useAudioPlayer', () => ({
  useAudioPlayer: () => ({ play: playMock }),
}))

import { rpcClient } from '@/api/rpc-client'
import { usePaidItemViewer, paidItemKey, type OwnedItemLike } from '../usePaidItemViewer'

const mockedRpc = vi.mocked(rpcClient)

const IMAGE_ITEM: OwnedItemLike = {
  onion: 'abc123.onion',
  content_id: 'content-1',
  filename: 'photos/sunset.jpg',
  mime_type: 'image/jpeg',
  size_bytes: 2048,
}
const VIDEO_ITEM: OwnedItemLike = {
  onion: 'abc123.onion',
  content_id: 'content-2',
  filename: 'clips/holiday.mp4',
  mime_type: 'video/mp4',
  size_bytes: 4096,
}
const AUDIO_ITEM: OwnedItemLike = {
  onion: 'abc123.onion',
  content_id: 'content-3',
  filename: 'music/track.mp3',
  mime_type: 'audio/mpeg',
  size_bytes: 1024,
}
const DOC_ITEM: OwnedItemLike = {
  onion: 'abc123.onion',
  content_id: 'content-4',
  filename: 'docs/invoice.pdf',
  mime_type: 'application/pdf',
  size_bytes: 512,
}

function deferred<T>() {
  let resolve!: (v: T) => void
  let reject!: (e: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe('usePaidItemViewer — UIFIX-04 (lightbox routing) + UIFIX-06 (loading/error)', () => {
  let createObjectURLSpy: ReturnType<typeof vi.fn>
  let revokeObjectURLSpy: ReturnType<typeof vi.fn>
  let windowOpenSpy: ReturnType<typeof vi.fn>

  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
    createObjectURLSpy = vi.fn(() => 'blob:mock-url')
    revokeObjectURLSpy = vi.fn()
    URL.createObjectURL = createObjectURLSpy as unknown as typeof URL.createObjectURL
    URL.revokeObjectURL = revokeObjectURLSpy as unknown as typeof URL.revokeObjectURL
    windowOpenSpy = vi.fn()
    window.open = windowOpenSpy as unknown as typeof window.open
    // atob is provided by jsdom; stub it to avoid depending on real base64 semantics.
    vi.stubGlobal('atob', vi.fn(() => 'binarydata'))
  })

  it('routes an image mime to the lightbox, not window.open', async () => {
    mockedRpc.call.mockResolvedValue({ data_base64: 'ZmFrZQ==', mime_type: 'image/jpeg' })
    const viewer = usePaidItemViewer()

    await viewer.open(IMAGE_ITEM)

    expect(windowOpenSpy).not.toHaveBeenCalled()
    expect(viewer.lightboxIndex.value).toBe(0)
    expect(viewer.lightboxItems.value).toHaveLength(1)
    expect(viewer.error.value).toBeNull()
  })

  it('routes a video mime to the lightbox, not window.open', async () => {
    mockedRpc.call.mockResolvedValue({ data_base64: 'ZmFrZQ==', mime_type: 'video/mp4' })
    const viewer = usePaidItemViewer()

    await viewer.open(VIDEO_ITEM)

    expect(windowOpenSpy).not.toHaveBeenCalled()
    expect(viewer.lightboxIndex.value).toBe(0)
    expect(viewer.lightboxItems.value[0]?.name).toBe('holiday.mp4')
  })

  it('routes an audio mime to the audio player, never the lightbox', async () => {
    mockedRpc.call.mockResolvedValue({ data_base64: 'ZmFrZQ==', mime_type: 'audio/mpeg' })
    const viewer = usePaidItemViewer()

    await viewer.open(AUDIO_ITEM)

    expect(playMock).toHaveBeenCalledWith('blob:mock-url', 'track.mp3')
    expect(viewer.lightboxIndex.value).toBeNull()
    expect(windowOpenSpy).not.toHaveBeenCalled()
  })

  it('falls back to the browser tab for a mime with no in-app viewer', async () => {
    mockedRpc.call.mockResolvedValue({ data_base64: 'ZmFrZQ==', mime_type: 'application/pdf' })
    const viewer = usePaidItemViewer()

    await viewer.open(DOC_ITEM)

    expect(windowOpenSpy).toHaveBeenCalledWith('blob:mock-url', '_blank', 'noopener')
    expect(viewer.lightboxIndex.value).toBeNull()
    // Existing revoke timer for the browser-tab path is untouched.
    vi.advanceTimersByTime(60000)
    expect(revokeObjectURLSpy).toHaveBeenCalledWith('blob:mock-url')
  })

  it('the synthetic lightbox item name carries the real extension', async () => {
    mockedRpc.call.mockResolvedValue({ data_base64: 'ZmFrZQ==', mime_type: 'image/jpeg' })
    const viewer = usePaidItemViewer()

    await viewer.open(IMAGE_ITEM)

    expect(viewer.lightboxItems.value[0]?.name.endsWith('.jpg')).toBe(true)
  })

  it('sets opening for the whole duration of the fetch and clears it on success', async () => {
    const d = deferred<{ data_base64: string; mime_type: string }>()
    mockedRpc.call.mockReturnValue(d.promise as unknown as ReturnType<typeof rpcClient.call>)
    const viewer = usePaidItemViewer()

    const p = viewer.open(IMAGE_ITEM)
    expect(viewer.opening.value).toBe(paidItemKey(IMAGE_ITEM))

    d.resolve({ data_base64: 'ZmFrZQ==', mime_type: 'image/jpeg' })
    await p

    expect(viewer.opening.value).toBeNull()
  })

  it('clears opening and does not derive it from a background-refresh flag — it is driven only by the fetch in flight', async () => {
    // No cached-resource / refreshing concept is wired into this composable at
    // all: opening only ever reflects the current open() call's own RPC.
    const d = deferred<{ data_base64: string; mime_type: string }>()
    mockedRpc.call.mockReturnValue(d.promise as unknown as ReturnType<typeof rpcClient.call>)
    const viewer = usePaidItemViewer()

    expect(viewer.opening.value).toBeNull() // idle before any open()
    const p = viewer.open(IMAGE_ITEM)
    expect(viewer.opening.value).toBe(paidItemKey(IMAGE_ITEM))
    d.resolve({ data_base64: 'ZmFrZQ==', mime_type: 'image/jpeg' })
    await p
    expect(viewer.opening.value).toBeNull() // back to idle the instant the fetch settles — no lingering "refreshing" state
  })

  it('surfaces a rejected/timed-out fetch as an error, clears opening, and does not throw past the caller', async () => {
    mockedRpc.call.mockRejectedValue(new Error('Request timeout'))
    const viewer = usePaidItemViewer()

    await expect(viewer.open(IMAGE_ITEM)).resolves.toBeUndefined()

    expect(viewer.error.value).toBeTruthy()
    expect(viewer.opening.value).toBeNull()
    expect(viewer.lightboxIndex.value).toBeNull()
  })

  it('issues exactly one RPC when open() is called twice in quick succession for the same item', async () => {
    const d = deferred<{ data_base64: string; mime_type: string }>()
    mockedRpc.call.mockReturnValue(d.promise as unknown as ReturnType<typeof rpcClient.call>)
    const viewer = usePaidItemViewer()

    const p1 = viewer.open(IMAGE_ITEM)
    const p2 = viewer.open(IMAGE_ITEM)

    expect(mockedRpc.call).toHaveBeenCalledTimes(1)

    d.resolve({ data_base64: 'ZmFrZQ==', mime_type: 'image/jpeg' })
    await Promise.all([p1, p2])

    expect(mockedRpc.call).toHaveBeenCalledTimes(1)
  })
})
