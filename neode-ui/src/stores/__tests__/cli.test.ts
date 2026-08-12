import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('@/composables/useNavSounds', () => ({
  playNavSound: vi.fn(),
}))

import { useCLIStore } from '../cli'
import { playNavSound } from '@/composables/useNavSounds'

const mockedPlayNavSound = vi.mocked(playNavSound)

describe('useCLIStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('starts closed', () => {
    const store = useCLIStore()
    expect(store.isOpen).toBe(false)
  })

  it('open sets isOpen to true and plays sound', () => {
    const store = useCLIStore()
    store.open()
    expect(store.isOpen).toBe(true)
    expect(mockedPlayNavSound).toHaveBeenCalledWith('action')
  })

  it('close sets isOpen to false without sound', () => {
    const store = useCLIStore()
    store.open()
    vi.clearAllMocks()
    store.close()
    expect(store.isOpen).toBe(false)
    expect(mockedPlayNavSound).not.toHaveBeenCalled()
  })

  it('toggle opens and plays sound when closed', () => {
    const store = useCLIStore()
    store.toggle()
    expect(store.isOpen).toBe(true)
    expect(mockedPlayNavSound).toHaveBeenCalledWith('action')
  })

  it('toggle closes without sound when open', () => {
    const store = useCLIStore()
    store.open()
    vi.clearAllMocks()
    store.toggle()
    expect(store.isOpen).toBe(false)
    expect(mockedPlayNavSound).not.toHaveBeenCalled()
  })

  it('multiple toggles alternate state', () => {
    const store = useCLIStore()
    store.toggle()
    expect(store.isOpen).toBe(true)
    store.toggle()
    expect(store.isOpen).toBe(false)
    store.toggle()
    expect(store.isOpen).toBe(true)
  })
})
