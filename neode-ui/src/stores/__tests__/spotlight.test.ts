import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Mock the nav sounds module
vi.mock('@/composables/useNavSounds', () => ({
  playNavSound: vi.fn(),
}))

import { useSpotlightStore } from '../spotlight'
import { playNavSound } from '@/composables/useNavSounds'

const mockedPlayNavSound = vi.mocked(playNavSound)

describe('useSpotlightStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    localStorage.clear()
  })

  it('starts closed with default state', () => {
    const store = useSpotlightStore()
    expect(store.isOpen).toBe(false)
    expect(store.selectedIndex).toBe(0)
    expect(store.recentItems).toEqual([])
  })

  it('open sets isOpen to true and plays sound', () => {
    const store = useSpotlightStore()

    store.open()

    expect(store.isOpen).toBe(true)
    expect(store.selectedIndex).toBe(0)
    expect(mockedPlayNavSound).toHaveBeenCalledWith('action')
  })

  it('close sets isOpen to false and resets index', () => {
    const store = useSpotlightStore()
    store.open()

    store.close()

    expect(store.isOpen).toBe(false)
    expect(store.selectedIndex).toBe(0)
  })

  it('toggle opens when closed', () => {
    const store = useSpotlightStore()

    store.toggle()

    expect(store.isOpen).toBe(true)
  })

  it('toggle closes when open', () => {
    const store = useSpotlightStore()
    store.open()

    store.toggle()

    expect(store.isOpen).toBe(false)
  })

  it('setSelectedIndex updates the selected index', () => {
    const store = useSpotlightStore()

    store.setSelectedIndex(3)

    expect(store.selectedIndex).toBe(3)
  })

  describe('recent items', () => {
    it('addRecentItem adds item with timestamp', () => {
      const store = useSpotlightStore()

      store.addRecentItem({ id: 'home', label: 'Home', path: '/dashboard', type: 'navigate' })

      expect(store.recentItems).toHaveLength(1)
      expect(store.recentItems[0]!.id).toBe('home')
      expect(store.recentItems[0]!.label).toBe('Home')
      expect(store.recentItems[0]!.timestamp).toBeGreaterThan(0)
    })

    it('addRecentItem deduplicates by id and type', () => {
      const store = useSpotlightStore()

      store.addRecentItem({ id: 'home', label: 'Home', path: '/dashboard', type: 'navigate' })
      store.addRecentItem({ id: 'home', label: 'Home Updated', path: '/dashboard', type: 'navigate' })

      expect(store.recentItems).toHaveLength(1)
      expect(store.recentItems[0]!.label).toBe('Home Updated')
    })

    it('addRecentItem keeps different types with same id', () => {
      const store = useSpotlightStore()

      store.addRecentItem({ id: 'bitcoin', label: 'Bitcoin (navigate)', type: 'navigate' })
      store.addRecentItem({ id: 'bitcoin', label: 'Bitcoin (action)', type: 'action' })

      expect(store.recentItems).toHaveLength(2)
    })

    it('addRecentItem caps at 8 items', () => {
      const store = useSpotlightStore()

      for (let i = 0; i < 10; i++) {
        store.addRecentItem({ id: `item-${i}`, label: `Item ${i}`, type: 'navigate' })
      }

      expect(store.recentItems).toHaveLength(8)
      // Most recent should be first
      expect(store.recentItems[0]!.id).toBe('item-9')
    })

    it('addRecentItem persists to localStorage', () => {
      const store = useSpotlightStore()

      store.addRecentItem({ id: 'apps', label: 'Apps', path: '/apps', type: 'navigate' })

      const stored = JSON.parse(localStorage.getItem('archipelago-spotlight-recent')!)
      expect(stored).toHaveLength(1)
      expect(stored[0].id).toBe('apps')
    })

    it('loadRecentItems reads from localStorage', () => {
      const saved = [
        { id: 'home', label: 'Home', path: '/dashboard', type: 'navigate', timestamp: 1000 },
        { id: 'apps', label: 'Apps', path: '/apps', type: 'navigate', timestamp: 2000 },
      ]
      localStorage.setItem('archipelago-spotlight-recent', JSON.stringify(saved))

      const store = useSpotlightStore()
      store.loadRecentItems()

      expect(store.recentItems).toHaveLength(2)
      expect(store.recentItems[0]!.id).toBe('home')
    })

    it('loadRecentItems handles corrupt localStorage', () => {
      localStorage.setItem('archipelago-spotlight-recent', 'not-json{{{')

      const store = useSpotlightStore()
      store.loadRecentItems()

      expect(store.recentItems).toEqual([])
    })

    it('loadRecentItems handles empty localStorage', () => {
      const store = useSpotlightStore()
      store.loadRecentItems()

      expect(store.recentItems).toEqual([])
    })

    it('open calls loadRecentItems', () => {
      const saved = [
        { id: 'test', label: 'Test', type: 'navigate', timestamp: 1000 },
      ]
      localStorage.setItem('archipelago-spotlight-recent', JSON.stringify(saved))

      const store = useSpotlightStore()
      store.open()

      expect(store.recentItems).toHaveLength(1)
    })
  })

  describe('help modal', () => {
    it('showHelpModal opens the modal with content', () => {
      const store = useSpotlightStore()

      store.showHelpModal({
        title: 'What is Bitcoin?',
        content: 'A peer-to-peer electronic cash system.',
        relatedPath: '/apps/bitcoin',
      })

      expect(store.helpModal.show).toBe(true)
      expect(store.helpModal.title).toBe('What is Bitcoin?')
      expect(store.helpModal.content).toBe('A peer-to-peer electronic cash system.')
      expect(store.helpModal.relatedPath).toBe('/apps/bitcoin')
    })

    it('closeHelpModal closes the modal', () => {
      const store = useSpotlightStore()
      store.showHelpModal({ title: 'Test', content: 'Content' })

      store.closeHelpModal()

      expect(store.helpModal.show).toBe(false)
    })
  })
})
