import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useScreensaverStore } from '../screensaver'

describe('useScreensaverStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('starts inactive', () => {
    const store = useScreensaverStore()
    expect(store.isActive).toBe(false)
  })

  it('activate sets isActive to true', () => {
    const store = useScreensaverStore()
    store.activate()
    expect(store.isActive).toBe(true)
  })

  it('deactivate sets isActive to false', () => {
    const store = useScreensaverStore()
    store.activate()
    store.deactivate()
    expect(store.isActive).toBe(false)
  })

  it('deactivate starts inactivity timer that activates after 3 minutes', () => {
    const store = useScreensaverStore()
    store.deactivate()
    expect(store.isActive).toBe(false)

    vi.advanceTimersByTime(3 * 60 * 1000)
    expect(store.isActive).toBe(true)
  })

  it('resetInactivityTimer restarts the 3-minute countdown', () => {
    const store = useScreensaverStore()
    store.deactivate()

    // Advance 2 minutes
    vi.advanceTimersByTime(2 * 60 * 1000)
    expect(store.isActive).toBe(false)

    // Reset timer
    store.resetInactivityTimer()

    // Advance another 2 minutes (would have triggered without reset)
    vi.advanceTimersByTime(2 * 60 * 1000)
    expect(store.isActive).toBe(false)

    // Full 3 minutes from reset
    vi.advanceTimersByTime(1 * 60 * 1000)
    expect(store.isActive).toBe(true)
  })

  it('clearInactivityTimer prevents activation', () => {
    const store = useScreensaverStore()
    store.deactivate()
    store.clearInactivityTimer()

    vi.advanceTimersByTime(5 * 60 * 1000)
    expect(store.isActive).toBe(false)
  })

  it('suppression prevents automatic and manual activation until resumed', () => {
    const store = useScreensaverStore()
    store.suppress('video')
    expect(store.isSuppressed).toBe(true)

    store.resetInactivityTimer()
    vi.advanceTimersByTime(5 * 60 * 1000)
    expect(store.isActive).toBe(false)

    store.activate()
    expect(store.isActive).toBe(false)

    store.resume('video')
    expect(store.isSuppressed).toBe(false)
    vi.advanceTimersByTime(3 * 60 * 1000)
    expect(store.isActive).toBe(true)
  })

  it('activate clears any pending timer', () => {
    const store = useScreensaverStore()
    store.deactivate()
    store.activate()

    // If timer wasn't cleared, deactivating and waiting would trigger twice
    store.deactivate()
    vi.advanceTimersByTime(3 * 60 * 1000)
    expect(store.isActive).toBe(true)
  })
})
