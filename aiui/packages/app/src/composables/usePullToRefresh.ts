import { ref, onMounted, onUnmounted, type Ref } from 'vue'
import { useHaptics } from './useHaptics'

export function usePullToRefresh(
  elementRef: Ref<HTMLElement | null>,
  onRefresh: () => Promise<void> | void
) {
  const isPulling = ref(false)
  const isRefreshing = ref(false)
  const pullDistance = ref(0)

  const haptics = useHaptics()
  const TRIGGER_DISTANCE = 80

  let startY = 0
  let tracking = false

  function onTouchStart(e: TouchEvent) {
    const el = elementRef.value
    if (!el || el.scrollTop > 0) return
    startY = e.touches[0].clientY
    tracking = true
  }

  function onTouchMove(e: TouchEvent) {
    if (!tracking || isRefreshing.value) return
    const dy = e.touches[0].clientY - startY
    if (dy < 0) {
      tracking = false
      return
    }
    pullDistance.value = Math.min(dy * 0.5, 120)
    isPulling.value = pullDistance.value > 20
  }

  async function onTouchEnd() {
    if (!tracking) return
    tracking = false

    if (pullDistance.value >= TRIGGER_DISTANCE) {
      haptics.pullRefresh()
      isRefreshing.value = true
      await onRefresh()
      isRefreshing.value = false
    }

    isPulling.value = false
    pullDistance.value = 0
  }

  onMounted(() => {
    const el = elementRef.value
    if (!el) return
    el.addEventListener('touchstart', onTouchStart, { passive: true })
    el.addEventListener('touchmove', onTouchMove, { passive: true })
    el.addEventListener('touchend', onTouchEnd)
  })

  onUnmounted(() => {
    const el = elementRef.value
    if (!el) return
    el.removeEventListener('touchstart', onTouchStart)
    el.removeEventListener('touchmove', onTouchMove)
    el.removeEventListener('touchend', onTouchEnd)
  })

  return { isPulling, isRefreshing, pullDistance }
}
