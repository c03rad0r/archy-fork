import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { UIMode } from '@/types/api'

const STORAGE_KEY = 'archipelago-ui-mode'

export const useUIModeStore = defineStore('uiMode', () => {
  const mode = ref<UIMode>(loadFromStorage())

  function loadFromStorage(): UIMode {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'gamer' || stored === 'easy' || stored === 'chat') return stored
    return 'gamer'
  }

  function syncFromBackend(backendMode: UIMode | undefined) {
    if (backendMode && ['gamer', 'easy', 'chat'].includes(backendMode)) {
      mode.value = backendMode
      try { localStorage.setItem(STORAGE_KEY, backendMode) } catch { /* localStorage full or unavailable */ }
    }
  }

  function setMode(newMode: UIMode) {
    mode.value = newMode
    try { localStorage.setItem(STORAGE_KEY, newMode) } catch { /* localStorage full or unavailable */ }
  }

  function cycleMode(): UIMode {
    const order: UIMode[] = ['easy', 'gamer']
    const idx = order.indexOf(mode.value)
    const next = order[(idx >= 0 ? idx + 1 : 0) % order.length] as UIMode
    setMode(next)
    return next
  }

  const isGamer = computed(() => mode.value === 'gamer')
  const isEasy = computed(() => mode.value === 'easy')
  const isChat = computed(() => mode.value === 'chat')

  return { mode, setMode, cycleMode, syncFromBackend, isGamer, isEasy, isChat }
})
