import { defineStore } from 'pinia'
import { ref, reactive } from 'vue'
import { playNavSound } from '@/composables/useNavSounds'

const RECENT_ITEMS_KEY = 'archipelago-spotlight-recent'
const MAX_RECENT_ITEMS = 8

export interface RecentItem {
  id: string
  label: string
  path?: string
  type: 'navigate' | 'learn' | 'action' | 'goal'
  timestamp: number
}

export const useSpotlightStore = defineStore('spotlight', () => {
  const isOpen = ref(false)
  const selectedIndex = ref(0)

  const recentItems = ref<RecentItem[]>([])

  function loadRecentItems() {
    try {
      const raw = localStorage.getItem(RECENT_ITEMS_KEY)
      if (raw) {
        const parsed: unknown = JSON.parse(raw)
        if (!Array.isArray(parsed)) {
          recentItems.value = []
          return
        }
        recentItems.value = parsed
          .filter((item: unknown) =>
            typeof item === 'object' && item !== null &&
            'id' in item && 'label' in item && 'type' in item && 'timestamp' in item
          )
          .slice(0, MAX_RECENT_ITEMS) as RecentItem[]
      } else {
        recentItems.value = []
      }
    } catch {
      recentItems.value = []
    }
  }

  function addRecentItem(item: Omit<RecentItem, 'timestamp'>) {
    const withTimestamp: RecentItem = { ...item, timestamp: Date.now() }
    const filtered = recentItems.value.filter(
      (r) => !(r.id === item.id && r.type === item.type)
    )
    recentItems.value = [withTimestamp, ...filtered].slice(0, MAX_RECENT_ITEMS)
    try {
      localStorage.setItem(RECENT_ITEMS_KEY, JSON.stringify(recentItems.value))
    } catch (e) {
      if (import.meta.env.DEV) console.warn('Failed to save recent items to storage', e)
    }
  }

  function open() {
    isOpen.value = true
    selectedIndex.value = 0
    loadRecentItems()
    playNavSound('action')
  }

  function close() {
    isOpen.value = false
    selectedIndex.value = 0
  }

  function toggle() {
    isOpen.value ? close() : open()
  }

  function setSelectedIndex(index: number) {
    selectedIndex.value = index
  }

  const helpModal = reactive({
    show: false,
    title: '',
    content: '',
    relatedPath: undefined as string | undefined,
  })
  const helpModalRestoreFocusRef = ref<HTMLElement | null>(null)

  function showHelpModal(payload: { title: string; content: string; relatedPath?: string }) {
    helpModalRestoreFocusRef.value = document.activeElement as HTMLElement | null
    helpModal.show = true
    helpModal.title = payload.title
    helpModal.content = payload.content
    helpModal.relatedPath = payload.relatedPath
  }

  function closeHelpModal() {
    helpModalRestoreFocusRef.value?.focus?.()
    helpModalRestoreFocusRef.value = null
    helpModal.show = false
  }

  return {
    isOpen,
    selectedIndex,
    recentItems,
    open,
    close,
    toggle,
    setSelectedIndex,
    addRecentItem,
    loadRecentItems,
    helpModal,
    showHelpModal,
    closeHelpModal,
  }
})
