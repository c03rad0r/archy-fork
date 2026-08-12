import { ref, onMounted, onUnmounted } from 'vue'

export function useSwipeNavigation(options: {
  onSwipeLeft?: () => void
  onSwipeRight?: () => void
  threshold?: number
  velocityThreshold?: number
}) {
  const { threshold = 80, velocityThreshold = 0.3 } = options
  const isSwiping = ref(false)
  const swipeOffset = ref(0)

  let startX = 0
  let startY = 0
  let startTime = 0
  let tracking = false

  function onTouchStart(e: TouchEvent) {
    startX = e.touches[0].clientX
    startY = e.touches[0].clientY
    startTime = Date.now()
    tracking = true
    swipeOffset.value = 0
  }

  function onTouchMove(e: TouchEvent) {
    if (!tracking) return
    const dx = e.touches[0].clientX - startX
    const dy = e.touches[0].clientY - startY

    // Only track horizontal swipes
    if (Math.abs(dy) > Math.abs(dx)) {
      tracking = false
      return
    }

    swipeOffset.value = dx
    isSwiping.value = Math.abs(dx) > 20
  }

  function onTouchEnd() {
    if (!tracking) {
      isSwiping.value = false
      swipeOffset.value = 0
      return
    }

    const elapsed = Date.now() - startTime
    const velocity = Math.abs(swipeOffset.value) / elapsed

    if (Math.abs(swipeOffset.value) > threshold || velocity > velocityThreshold) {
      if (swipeOffset.value > 0) {
        options.onSwipeRight?.()
      } else {
        options.onSwipeLeft?.()
      }
    }

    tracking = false
    isSwiping.value = false
    swipeOffset.value = 0
  }

  onMounted(() => {
    document.addEventListener('touchstart', onTouchStart, { passive: true })
    document.addEventListener('touchmove', onTouchMove, { passive: true })
    document.addEventListener('touchend', onTouchEnd)
  })

  onUnmounted(() => {
    document.removeEventListener('touchstart', onTouchStart)
    document.removeEventListener('touchmove', onTouchMove)
    document.removeEventListener('touchend', onTouchEnd)
  })

  return { isSwiping, swipeOffset }
}
