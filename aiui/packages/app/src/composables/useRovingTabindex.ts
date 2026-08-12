import { ref, onMounted, onUnmounted, type Ref } from 'vue'

export function useRovingTabindex(containerRef: Ref<HTMLElement | null>, selector = '[role="listitem"], [data-roving]') {
  const currentIndex = ref(0)

  function getItems(): HTMLElement[] {
    const container = containerRef.value
    if (!container) return []
    return Array.from(container.querySelectorAll<HTMLElement>(selector))
  }

  function updateTabindex() {
    const items = getItems()
    items.forEach((item, i) => {
      item.setAttribute('tabindex', i === currentIndex.value ? '0' : '-1')
    })
  }

  function onKeyDown(e: KeyboardEvent) {
    const items = getItems()
    if (items.length === 0) return

    let handled = false

    switch (e.key) {
      case 'ArrowDown':
      case 'ArrowRight':
        currentIndex.value = (currentIndex.value + 1) % items.length
        handled = true
        break
      case 'ArrowUp':
      case 'ArrowLeft':
        currentIndex.value = (currentIndex.value - 1 + items.length) % items.length
        handled = true
        break
      case 'Home':
        currentIndex.value = 0
        handled = true
        break
      case 'End':
        currentIndex.value = items.length - 1
        handled = true
        break
    }

    if (handled) {
      e.preventDefault()
      updateTabindex()
      items[currentIndex.value]?.focus()
    }
  }

  onMounted(() => {
    const container = containerRef.value
    if (container) {
      container.addEventListener('keydown', onKeyDown)
      updateTabindex()
    }
  })

  onUnmounted(() => {
    const container = containerRef.value
    if (container) {
      container.removeEventListener('keydown', onKeyDown)
    }
  })

  return { currentIndex, updateTabindex }
}
