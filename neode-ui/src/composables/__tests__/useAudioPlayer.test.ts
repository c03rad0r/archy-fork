import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useAudioPlayer } from '../useAudioPlayer'

// Mock HTMLAudioElement
let lastMockAudio: MockAudio | undefined

class MockAudio {
  src = ''
  currentTime = 0
  duration = 120
  paused = true
  private listeners: Record<string, Array<() => void>> = {}

  constructor() {
    lastMockAudio = this
  }

  addEventListener(event: string, handler: () => void) {
    if (!this.listeners[event]) this.listeners[event] = []
    this.listeners[event].push(handler)
  }

  removeEventListener() {
    // no-op for tests
  }

  shouldRejectPlay = false

  play() {
    if (this.shouldRejectPlay) {
      return Promise.reject(new DOMException('no supported source', 'NotSupportedError'))
    }
    this.paused = false
    this.emit('play')
    return Promise.resolve()
  }

  pause() {
    this.paused = true
    this.emit('pause')
  }

  private emit(event: string) {
    const handlers = this.listeners[event] || []
    handlers.forEach(h => h())
  }

  // Helper to simulate events in tests
  simulateEvent(event: string) {
    this.emit(event)
  }
}

vi.stubGlobal('Audio', MockAudio)

describe('useAudioPlayer', () => {
  beforeEach(() => {
    // Reset singleton state by stopping any active playback
    const player = useAudioPlayer()
    player.stop()
    if (lastMockAudio) lastMockAudio.shouldRejectPlay = false
  })

  it('controls an app player without copying its stream or losing its position', () => {
    const player = useAudioPlayer()
    player.play('/local.mp3', 'Local')
    const controller = { id: 'demo-session', play: vi.fn(), pause: vi.fn(), seek: vi.fn(), open: vi.fn(), close: vi.fn() }
    player.updateExternal(controller, { title: 'Demo', artist: 'Artist', playing: true, position: 42, duration: 100 }, true)
    expect(lastMockAudio?.paused).toBe(true)
    expect(player.currentSrc.value).toBe('app:demo-session')
    lastMockAudio?.simulateEvent('pause')
    expect(player.playing.value).toBe(true)
    expect(player.currentTime.value).toBe(42)
    player.setExternalVisible('demo-session', false)
    expect(player.externalVisible.value).toBe(false)
    player.pause(); player.seek(55); player.openExternal()
    expect(controller.pause).toHaveBeenCalledOnce()
    expect(controller.seek).toHaveBeenCalledWith(55)
    expect(controller.open).toHaveBeenCalledOnce()
    player.play('app:demo-session', 'Demo')
    expect(controller.play).toHaveBeenCalledOnce()
    player.stop()
    expect(controller.close).toHaveBeenCalledOnce()
    expect(player.currentName.value).toBe('')
  })

  it('returns all expected properties', () => {
    const player = useAudioPlayer()
    expect(player.play).toBeTypeOf('function')
    expect(player.pause).toBeTypeOf('function')
    expect(player.seek).toBeTypeOf('function')
    expect(player.stop).toBeTypeOf('function')
    expect(player.playing).toBeDefined()
    expect(player.currentName).toBeDefined()
    expect(player.currentTime).toBeDefined()
    expect(player.duration).toBeDefined()
    expect(player.progress).toBeDefined()
    expect(player.currentSrc).toBeDefined()
    expect(player.error).toBeDefined()
  })

  it('starts in stopped state', () => {
    const player = useAudioPlayer()
    expect(player.playing.value).toBe(false)
    expect(player.currentSrc.value).toBeNull()
    expect(player.currentName.value).toBe('')
  })

  it('play sets playing state and current source', () => {
    const player = useAudioPlayer()
    player.play('/audio/test.mp3', 'Test Track')
    expect(player.playing.value).toBe(true)
    expect(player.currentSrc.value).toBe('/audio/test.mp3')
    expect(player.currentName.value).toBe('Test Track')
  })

  it('play toggles pause when same source is playing', () => {
    const player = useAudioPlayer()
    player.play('/audio/test.mp3', 'Test')
    expect(player.playing.value).toBe(true)
    // Play same source again — should pause
    player.play('/audio/test.mp3', 'Test')
    expect(player.playing.value).toBe(false)
  })

  it('play switches to new source', () => {
    const player = useAudioPlayer()
    player.play('/audio/first.mp3', 'First')
    player.play('/audio/second.mp3', 'Second')
    expect(player.currentSrc.value).toBe('/audio/second.mp3')
    expect(player.currentName.value).toBe('Second')
  })

  it('pause pauses playback', () => {
    const player = useAudioPlayer()
    player.play('/audio/test.mp3', 'Test')
    player.pause()
    expect(player.playing.value).toBe(false)
  })

  it('stop resets all state', () => {
    const player = useAudioPlayer()
    player.play('/audio/test.mp3', 'Test')
    player.stop()
    expect(player.playing.value).toBe(false)
    expect(player.currentSrc.value).toBeNull()
    expect(player.currentName.value).toBe('')
  })

  it('progress computes correctly', () => {
    const player = useAudioPlayer()
    expect(player.progress.value).toBe(0) // duration is 0

    player.currentTime.value = 30
    player.duration.value = 120
    expect(player.progress.value).toBe(25) // 30/120 * 100
  })

  it('progress is 0 when duration is 0', () => {
    const player = useAudioPlayer()
    player.duration.value = 0
    player.currentTime.value = 10
    expect(player.progress.value).toBe(0)
  })

  it('play() rejection is caught, not left as an unhandled promise rejection', async () => {
    // Regression: play() rejects independently of the 'error' event (e.g. a
    // peer-content 404 with no decodable source) — this used to be an
    // unhandled rejection in the browser console even though the 'error'
    // listener already set a friendly message (2026-07-01).
    const player = useAudioPlayer()
    // Initialize the singleton Audio element first (a no-op play call).
    player.play('/audio/warmup.mp3', 'Warmup')
    player.stop()

    lastMockAudio!.shouldRejectPlay = true
    // Calling play() must not throw synchronously nor leave a rejected
    // promise unhandled — if useAudioPlayer's play() didn't .catch() the
    // rejection, `loading` would never flip back to false, since nothing
    // else resets it on this path (that's the real regression signal).
    expect(() => player.play('/audio/broken.mp3', 'Broken')).not.toThrow()
    await Promise.resolve()
    await Promise.resolve()
    expect(player.loading.value).toBe(false)
  })

  it('shared state across multiple useAudioPlayer calls', () => {
    const p1 = useAudioPlayer()
    const p2 = useAudioPlayer()
    p1.play('/audio/shared.mp3', 'Shared')
    expect(p2.currentSrc.value).toBe('/audio/shared.mp3')
    expect(p2.playing.value).toBe(true)
  })
})
