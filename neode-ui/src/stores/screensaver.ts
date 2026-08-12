import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

const INACTIVITY_MS = 3 * 60 * 1000 // 3 minutes

export const useScreensaverStore = defineStore('screensaver', () => {
  const isActive = ref(false)
  const activationCount = ref(0)
  const suppressionReasons = ref<Set<string>>(new Set())
  let inactivityTimer: ReturnType<typeof setTimeout> | null = null

  /** True when the current activation is the ASCII variant (every 3rd time) */
  const isAsciiMode = computed(() => activationCount.value > 0 && activationCount.value % 3 === 0)
  const isSuppressed = computed(() => suppressionReasons.value.size > 0)

  function activate() {
    if (isSuppressed.value) return
    activationCount.value++
    isActive.value = true
    clearInactivityTimer()
  }

  function deactivate() {
    isActive.value = false
    resetInactivityTimer()
  }

  function resetInactivityTimer() {
    clearInactivityTimer()
    if (isSuppressed.value) return
    inactivityTimer = setTimeout(() => {
      inactivityTimer = null
      if (isSuppressed.value) return
      isActive.value = true
    }, INACTIVITY_MS)
  }

  function clearInactivityTimer() {
    if (inactivityTimer) {
      clearTimeout(inactivityTimer)
      inactivityTimer = null
    }
  }

  function suppress(reason: string) {
    suppressionReasons.value = new Set(suppressionReasons.value).add(reason)
    clearInactivityTimer()
    isActive.value = false
  }

  function resume(reason: string) {
    if (!suppressionReasons.value.has(reason)) return
    const next = new Set(suppressionReasons.value)
    next.delete(reason)
    suppressionReasons.value = next
    if (next.size === 0) resetInactivityTimer()
  }

  return {
    isActive,
    isAsciiMode,
    isSuppressed,
    activationCount,
    activate,
    deactivate,
    resetInactivityTimer,
    clearInactivityTimer,
    suppress,
    resume,
  }
})
