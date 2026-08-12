import { ref, onMounted, onUnmounted } from 'vue'

/**
 * Tracks the visual viewport to detect mobile keyboard open/close.
 * Uses the VisualViewport API to compute keyboard height as the difference
 * between window.innerHeight and visualViewport.height.
 *
 * When the keyboard opens, viewportHeight shrinks to the visible area above
 * the keyboard. Bind your container's height to viewportHeight to make the
 * layout resize instead of the browser pushing content offscreen.
 */
export function useVisualViewport() {
  const keyboardHeight = ref(0)
  const isKeyboardOpen = ref(false)
  const viewportHeight = ref(typeof window !== 'undefined' ? window.innerHeight : 0)

  // Store the initial full height so we can compute keyboard offset
  let fullHeight = typeof window !== 'undefined' ? window.innerHeight : 0

  let debounceTimer: ReturnType<typeof setTimeout> | null = null

  function onViewportChangeRaw() {
    if (debounceTimer) clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      const vv = window.visualViewport
      if (!vv) return
      const kbHeight = Math.max(0, fullHeight - vv.height)
      keyboardHeight.value = kbHeight
      isKeyboardOpen.value = kbHeight > 100
      viewportHeight.value = vv.height
    }, 50)
  }

  function onWindowResize() {
    // Update full height when orientation changes or browser chrome resizes
    const vv = window.visualViewport
    if (vv && !isKeyboardOpen.value) {
      fullHeight = vv.height
      viewportHeight.value = vv.height
    }
  }

  onMounted(() => {
    const vv = window.visualViewport
    if (vv) {
      fullHeight = vv.height
      viewportHeight.value = vv.height
      vv.addEventListener('resize', onViewportChangeRaw)
      vv.addEventListener('scroll', onViewportChangeRaw)
    }
    window.addEventListener('resize', onWindowResize)
  })

  onUnmounted(() => {
    const vv = window.visualViewport
    if (vv) {
      vv.removeEventListener('resize', onViewportChangeRaw)
      vv.removeEventListener('scroll', onViewportChangeRaw)
    }
    window.removeEventListener('resize', onWindowResize)
  })

  return { keyboardHeight, isKeyboardOpen, viewportHeight }
}
