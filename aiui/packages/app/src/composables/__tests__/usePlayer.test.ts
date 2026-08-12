import { describe, it, expect, beforeEach, vi } from 'vitest'

// Mock Plyr before importing usePlayer
vi.mock('plyr', () => {
  const MockPlyr = vi.fn().mockImplementation(() => ({
    on: vi.fn(),
    play: vi.fn().mockResolvedValue(undefined),
    pause: vi.fn(),
    destroy: vi.fn(),
    currentTime: 0,
    duration: 120,
  }))
  return { default: MockPlyr }
})

vi.mock('plyr/dist/plyr.css', () => ({}))

import { usePlayer } from '../usePlayer'

function makeSong(id: string, title = `Song ${id}`, artist = `Artist ${id}`) {
  return {
    id,
    title,
    artist,
    album: 'Test Album',
    genres: [],
  }
}

describe('usePlayer', () => {
  beforeEach(() => {
    const player = usePlayer()
    player.clearQueue()
    vi.restoreAllMocks()
    // Reset fetch mock
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      json: () => Promise.resolve({ source: 'test', type: 'stream', url: 'https://example.com/audio.mp3' }),
    }))
  })

  it('returns expected API shape', () => {    const player = usePlayer()
    expect(player.currentSong).toBeDefined()
    expect(player.isPlaying).toBeDefined()
    expect(player.isLoading).toBeDefined()
    expect(player.queue).toBeDefined()
    expect(player.play).toBeTypeOf('function')
    expect(player.pause).toBeTypeOf('function')
    expect(player.toggle).toBeTypeOf('function')
    expect(player.seek).toBeTypeOf('function')
    expect(player.addToQueue).toBeTypeOf('function')
    expect(player.playNext).toBeTypeOf('function')
    expect(player.playPrevious).toBeTypeOf('function')
    expect(player.clearQueue).toBeTypeOf('function')
  })

  it('plays a node-source song directly without touching Wavlake', async () => {
    const fetchSpy = vi.fn()
    vi.stubGlobal('fetch', fetchSpy)
    const player = usePlayer()
    const song = {
      ...makeSong('n1', 'Node Track', 'Node Artist'),
      sources: [{ type: 'funkwhale', name: 'This node', url: '/content/abc-123' }],
    }
    await player.play(song as never)
    expect(fetchSpy).not.toHaveBeenCalled()
    expect(player.isLoading.value).toBe(false)
    expect(player.error.value).toBeNull()
    expect(player.currentSong.value?.id).toBe('n1')
    expect(player.playableSource.value?.url).toBe('/content/abc-123')
    expect(player.playableSource.value?.source).toBe('node')
  })

  it('starts with no track', () => {
    const player = usePlayer()
    expect(player.hasTrack.value).toBe(false)
    expect(player.currentSong.value).toBeNull()
    expect(player.isPlaying.value).toBe(false)
  })

  it('addToQueue adds songs', () => {
    const player = usePlayer()
    const song1 = makeSong('1')
    const song2 = makeSong('2')
    player.addToQueue(song1)
    player.addToQueue(song2)
    expect(player.queue.value).toHaveLength(2)
    expect(player.queue.value[0].id).toBe('1')
    expect(player.queue.value[1].id).toBe('2')
  })

  it('addToQueue deduplicates by id', () => {
    const player = usePlayer()
    const song = makeSong('1')
    player.addToQueue(song)
    player.addToQueue(song)
    expect(player.queue.value).toHaveLength(1)
  })

  it('removeFromQueue removes by index', () => {
    const player = usePlayer()
    player.addToQueue(makeSong('1'))
    player.addToQueue(makeSong('2'))
    player.addToQueue(makeSong('3'))
    player.removeFromQueue(1)
    expect(player.queue.value).toHaveLength(2)
    expect(player.queue.value[0].id).toBe('1')
    expect(player.queue.value[1].id).toBe('3')
  })

  it('clearQueue resets all state', () => {
    const player = usePlayer()
    player.addToQueue(makeSong('1'))
    player.addToQueue(makeSong('2'))
    player.clearQueue()
    expect(player.queue.value).toHaveLength(0)
    expect(player.currentIndex.value).toBe(-1)
    expect(player.currentSong.value).toBeNull()
  })

  it('hasNext and hasPrevious compute correctly', () => {
    const player = usePlayer()
    player.addToQueue(makeSong('1'))
    player.addToQueue(makeSong('2'))
    player.addToQueue(makeSong('3'))

    // At start, no previous
    expect(player.hasPrevious.value).toBe(false)

    // Move to middle
    player.currentIndex.value = 1
    expect(player.hasNext.value).toBe(true)
    expect(player.hasPrevious.value).toBe(true)

    // At end
    player.currentIndex.value = 2
    expect(player.hasNext.value).toBe(false)
    expect(player.hasPrevious.value).toBe(true)
  })

  it('progress computes percentage from currentTime/duration', () => {
    const player = usePlayer()
    player.duration.value = 100
    player.currentTime.value = 50
    expect(player.progress.value).toBe(50)

    player.currentTime.value = 0
    expect(player.progress.value).toBe(0)
  })

  it('progress returns 0 when duration is 0', () => {
    const player = usePlayer()
    player.duration.value = 0
    player.currentTime.value = 10
    expect(player.progress.value).toBe(0)
  })
})
