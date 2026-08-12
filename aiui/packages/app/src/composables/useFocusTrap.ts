import { onMounted, onUnmounted, type Ref } from 'vue'

export function useFocusTrap(containerRef: Ref<HTMLElement | null>) {
  function getFocusableElements(): HTMLElement[] {
    const container = containerRef.value
    if (!container) return []
    return Array.from(
      container.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    )
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // Bubble up for close handlers
      return
    }

    if (e.key !== 'Tab') return

    const focusable = getFocusableElements()
    if (focusable.length === 0) return

    const first = focusable[0]
    const last = focusable[focusable.length - 1]

    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault()
        last.focus()
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }

  onMounted(() => {
    document.addEventListener('keydown', onKeyDown)
    // Auto-focus first element
    const focusable = getFocusableElements()
    if (focusable.length > 0) {
      focusable[0].focus()
    }
  })

  onUnmounted(() => {
    document.removeEventListener('keydown', onKeyDown)
  })
}
