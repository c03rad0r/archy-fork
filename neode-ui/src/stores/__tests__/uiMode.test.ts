import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useUIModeStore } from '../uiMode'

describe('useUIModeStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })

  it('defaults to gamer mode when no stored value', () => {
    const store = useUIModeStore()
    expect(store.mode).toBe('gamer')
    expect(store.isGamer).toBe(true)
    expect(store.isEasy).toBe(false)
    expect(store.isChat).toBe(false)
  })

  it('loads stored mode from localStorage', () => {
    localStorage.setItem('archipelago-ui-mode', 'easy')
    const store = useUIModeStore()
    expect(store.mode).toBe('easy')
    expect(store.isEasy).toBe(true)
  })

  it('ignores invalid localStorage values', () => {
    localStorage.setItem('archipelago-ui-mode', 'invalid-mode')
    const store = useUIModeStore()
    expect(store.mode).toBe('gamer')
  })

  it('setMode updates mode and persists', () => {
    const store = useUIModeStore()
    store.setMode('easy')
    expect(store.mode).toBe('easy')
    expect(store.isEasy).toBe(true)
    expect(localStorage.getItem('archipelago-ui-mode')).toBe('easy')
  })

  it('setMode to chat mode', () => {
    const store = useUIModeStore()
    store.setMode('chat')
    expect(store.mode).toBe('chat')
    expect(store.isChat).toBe(true)
    expect(store.isGamer).toBe(false)
  })

  it('cycleMode cycles between easy and gamer', () => {
    const store = useUIModeStore()
    // Start at gamer
    expect(store.mode).toBe('gamer')

    // Cycle to easy
    const next1 = store.cycleMode()
    expect(next1).toBe('easy')
    expect(store.mode).toBe('easy')

    // Cycle back to gamer (wraps after easy since order is [easy, gamer])
    const next2 = store.cycleMode()
    expect(next2).toBe('gamer')
    expect(store.mode).toBe('gamer')
  })

  it('cycleMode from chat wraps to easy', () => {
    const store = useUIModeStore()
    store.setMode('chat')
    const next = store.cycleMode()
    // chat is not in the order array, so idx=-1, next = order[0] = easy
    expect(next).toBe('easy')
  })

  it('syncFromBackend updates mode from backend', () => {
    const store = useUIModeStore()
    store.syncFromBackend('easy')
    expect(store.mode).toBe('easy')
    expect(localStorage.getItem('archipelago-ui-mode')).toBe('easy')
  })

  it('syncFromBackend ignores invalid modes', () => {
    const store = useUIModeStore()
    store.syncFromBackend('invalid' as 'gamer')
    expect(store.mode).toBe('gamer') // unchanged
  })

  it('syncFromBackend ignores undefined', () => {
    const store = useUIModeStore()
    store.syncFromBackend(undefined)
    expect(store.mode).toBe('gamer') // unchanged
  })
})
