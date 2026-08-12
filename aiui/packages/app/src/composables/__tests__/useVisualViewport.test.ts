import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { withSetup } from './testUtils'
import { useVisualViewport } from '../useVisualViewport'

// Helper to create a mock VisualViewport
function createMockViewport(height = 800) {
  const listeners: Record<string, Set<EventListener>> = {}
  return {
    height,
    width: 390,
    offsetLeft: 0,
    offsetTop: 0,
    pageLeft: 0,
    pageTop: 0,
    scale: 1,
    addEventListener: vi.fn((event: string, handler: EventListener) => {
      if (!listeners[event]) listeners[event] = new Set()
      listeners[event].add(handler)
    }),
    removeEventListener: vi.fn((event: string, handler: EventListener) => {
      listeners[event]?.delete(handler)
    }),
    dispatchEvent: vi.fn(),
    onresize: null,
    onscroll: null,
    _listeners: listeners,
    _setHeight(h: number) {
      this.height = h
      // Fire resize listeners
      listeners['resize']?.forEach(fn => fn(new Event('resize')))
    },
  }
}

describe('useVisualViewport', () => {
  let mockVV: ReturnType<typeof createMockViewport>

  beforeEach(() => {
    vi.useFakeTimers()
    mockVV = createMockViewport(800)
    Object.defineProperty(window, 'visualViewport', { value: mockVV, configurable: true, writable: true })
    Object.defineProperty(window, 'innerHeight', { value: 800, configurable: true, writable: true })
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('returns expected API shape', () => {
    const [result] = withSetup(() => useVisualViewport())
    expect(result.keyboardHeight).toBeDefined()
    expect(result.isKeyboardOpen).toBeDefined()
    expect(result.viewportHeight).toBeDefined()
  })

  it('initializes viewportHeight from visualViewport', () => {
    const [result] = withSetup(() => useVisualViewport())
    expect(result.viewportHeight.value).toBe(800)
  })

  it('detects keyboard open when height shrinks significantly', () => {
    const [result] = withSetup(() => useVisualViewport())
    // Simulate keyboard opening (shrinks to 500px from 800px = 300px keyboard)
    mockVV._setHeight(500)
    vi.advanceTimersByTime(60) // past debounce
    expect(result.isKeyboardOpen.value).toBe(true)
    expect(result.keyboardHeight.value).toBe(300)
    expect(result.viewportHeight.value).toBe(500)
  })

  it('does not trigger keyboard for small viewport changes', () => {
    const [result] = withSetup(() => useVisualViewport())
    // Small change (50px) — not keyboard
    mockVV._setHeight(750)
    vi.advanceTimersByTime(60)
    expect(result.isKeyboardOpen.value).toBe(false)
    expect(result.keyboardHeight.value).toBe(50)
  })

  it('debounces rapid viewport changes', () => {
    const [result] = withSetup(() => useVisualViewport())
    // Rapid changes — only last should apply
    mockVV._setHeight(600)
    vi.advanceTimersByTime(10)
    mockVV._setHeight(550)
    vi.advanceTimersByTime(10)
    mockVV._setHeight(500)
    vi.advanceTimersByTime(60)
    expect(result.viewportHeight.value).toBe(500)
    expect(result.keyboardHeight.value).toBe(300)
  })
})
