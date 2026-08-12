import { ref, onMounted, onUnmounted, type Ref } from 'vue'

export function usePinchZoom(elementRef: Ref<HTMLElement | null>) {
  const scale = ref(1)
  const translateX = ref(0)
  const translateY = ref(0)

  let initialDistance = 0
  let initialScale = 1
  let lastTapTime = 0

  function getDistance(t1: Touch, t2: Touch): number {
    return Math.hypot(t2.clientX - t1.clientX, t2.clientY - t1.clientY)
  }

  function onTouchStart(e: TouchEvent) {
    if (e.touches.length === 2) {
      e.preventDefault()
      initialDistance = getDistance(e.touches[0], e.touches[1])
      initialScale = scale.value
    } else if (e.touches.length === 1) {
      // Double-tap detection
      const now = Date.now()
      if (now - lastTapTime < 300) {
        // Double-tap: reset
        scale.value = 1
        translateX.value = 0
        translateY.value = 0
      }
      lastTapTime = now
    }
  }

  function onTouchMove(e: TouchEvent) {
    if (e.touches.length === 2) {
      e.preventDefault()
      const currentDistance = getDistance(e.touches[0], e.touches[1])
      const newScale = initialScale * (currentDistance / initialDistance)
      scale.value = Math.max(1, Math.min(4, newScale))
    }
  }

  function onTouchEnd() {
    if (scale.value <= 1.05) {
      scale.value = 1
      translateX.value = 0
      translateY.value = 0
    }
  }

  onMounted(() => {
    const el = elementRef.value
    if (!el) return
    el.addEventListener('touchstart', onTouchStart, { passive: false })
    el.addEventListener('touchmove', onTouchMove, { passive: false })
    el.addEventListener('touchend', onTouchEnd)
  })

  onUnmounted(() => {
    const el = elementRef.value
    if (!el) return
    el.removeEventListener('touchstart', onTouchStart)
    el.removeEventListener('touchmove', onTouchMove)
    el.removeEventListener('touchend', onTouchEnd)
  })

  return {
    scale,
    translateX,
    translateY,
    transform: () => `scale(${scale.value}) translate(${translateX.value}px, ${translateY.value}px)`,
  }
}
