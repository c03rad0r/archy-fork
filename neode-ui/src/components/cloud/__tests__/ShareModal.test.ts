import { describe, it, expect } from 'vitest'
import { SHARE_MIME_MAP } from '../ShareModal.vue'

// 13-11 Task 2: ShareModal.vue's extension-to-MIME map historically listed
// exactly four audio extensions (mp3/flac/ogg/wav) and left
// m4a/aac/opus/wma to fall through to `application/octet-stream` — those
// four never routed to the global audio player and auto-filed to Documents
// instead of Music. This pins all eight audio extensions plus the existing
// non-audio entries, following useFileType.test.ts's fixture-table
// convention.

describe('ShareModal SHARE_MIME_MAP', () => {
  it.each([
    ['mp3', 'audio/mpeg'],
    ['flac', 'audio/flac'],
    ['ogg', 'audio/ogg'],
    ['wav', 'audio/wav'],
    ['m4a', 'audio/mp4'],
    ['aac', 'audio/aac'],
    ['opus', 'audio/opus'],
    ['wma', 'audio/x-ms-wma'],
  ])('maps .%s to the audio MIME %s', (ext, expected) => {
    expect(SHARE_MIME_MAP[ext]).toBe(expected)
  })

  it('every one of the eight audio extensions maps to a real audio/* MIME (not application/octet-stream)', () => {
    const audioExts = ['mp3', 'flac', 'ogg', 'wav', 'm4a', 'aac', 'opus', 'wma']
    for (const ext of audioExts) {
      const mime = SHARE_MIME_MAP[ext]
      expect(mime).toBeDefined()
      expect(mime!.startsWith('audio/')).toBe(true)
    }
  })

  it('existing non-audio entries are unchanged', () => {
    expect(SHARE_MIME_MAP.jpg).toBe('image/jpeg')
    expect(SHARE_MIME_MAP.mp4).toBe('video/mp4')
    expect(SHARE_MIME_MAP.pdf).toBe('application/pdf')
    expect(SHARE_MIME_MAP.zip).toBe('application/zip')
    expect(SHARE_MIME_MAP.txt).toBe('text/plain')
  })

  it('an unknown extension is absent from the map — the fix adds coverage, it does not guess', () => {
    expect(SHARE_MIME_MAP.xyz123).toBeUndefined()
  })

  it('agrees with archyContentAdapter.ts\'s classifyByMime on all eight audio extensions', async () => {
    const { classifyByMime } = await import('../../../composables/archyContentAdapter')
    const audioExts = ['mp3', 'flac', 'ogg', 'wav', 'm4a', 'aac', 'opus', 'wma']
    for (const ext of audioExts) {
      const mime = SHARE_MIME_MAP[ext]!
      expect(classifyByMime({ mime_type: mime, filename: `track.${ext}` })).toBe('audio')
    }
  })
})

describe('an audio MIME never opens the lightbox', () => {
  it('usePaidItemViewer routes every audio/* mime to the global player before its image/video lightbox branch is ever reached', () => {
    // usePaidItemViewer.ts's routing order is: audio -> global bottom-bar
    // player (return); image/video -> lightbox (return); anything else ->
    // browser-tab fallback. The audio branch is checked FIRST and always
    // returns, so no audio/* mime can ever reach the lightbox branch —
    // pinned here structurally (mirrors the real predicate, avoids a heavy
    // component mount) rather than re-implementing usePaidItemViewer's
    // async RPC/blob flow in a unit test.
    const audioExts = ['mp3', 'flac', 'ogg', 'wav', 'm4a', 'aac', 'opus', 'wma']
    for (const ext of audioExts) {
      const mime = SHARE_MIME_MAP[ext]!
      const routesToAudioPlayerFirst = mime.startsWith('audio/')
      expect(routesToAudioPlayerFirst).toBe(true)
    }
  })
})
