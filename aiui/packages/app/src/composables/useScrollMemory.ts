import { onMounted, onUnmounted, type Ref } from 'vue'

const scrollPositions = new Map<string, number>()

export function useScrollMemory(key: string, elementRef: Ref<HTMLElement | null>) {
  function savePosition() {
    const el = elementRef.value
    if (el) {
      scrollPositions.set(key, el.scrollTop)
    }
  }

  function restorePosition() {
    const el = elementRef.value
    if (!el) return
    const saved = scrollPositions.get(key)
    if (saved !== undefined) {
      requestAnimationFrame(() => {
        el.scrollTop = saved
      })
    }
  }

  onMounted(() => {
    restorePosition()
    const el = elementRef.value
    if (el) {
      el.addEventListener('scroll', savePosition, { passive: true })
    }
  })

  onUnmounted(() => {
    savePosition()
    const el = elementRef.value
    if (el) {
      el.removeEventListener('scroll', savePosition)
    }
  })

  return { savePosition, restorePosition }
}
