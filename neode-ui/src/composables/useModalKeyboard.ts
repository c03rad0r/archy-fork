/**
 * Modal keyboard navigation: Escape to close, Arrow keys to move between buttons.
 * Restores focus to the previously active element when closing via Escape.
 */

import { onMounted, onBeforeUnmount, watch, type Ref } from 'vue'

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

export interface UseModalKeyboardOptions {
  restoreFocusRef?: Ref<HTMLElement | null>
}

export function useModalKeyboard(
  containerRef: Ref<HTMLElement | null>,
  isOpen: Ref<boolean>,
  onClose: () => void,
  options?: UseModalKeyboardOptions
) {
  const restoreFocusRef = options?.restoreFocusRef

  // Save the element that had focus when modal opens (before focus moves to modal)
  watch(isOpen, (open) => {
    if (open && restoreFocusRef) {
      restoreFocusRef.value = document.activeElement as HTMLElement | null
    }
  })
  function getFocusables(): HTMLElement[] {
    const el = containerRef.value
    if (!el) return []
    return Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (e) => e.offsetParent !== null
    )
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isOpen.value) return
    if ((e.target as HTMLElement | null)?.closest?.('[data-archipelago-terminal]')) return

    if (e.key === 'Escape') {
      restoreFocusRef?.value?.focus?.()
      onClose()
      e.preventDefault()
      e.stopPropagation()
      return
    }

    if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
      const focusables = getFocusables()
      if (focusables.length === 0) return

      const current = document.activeElement as HTMLElement | null
      const idx = current ? focusables.indexOf(current) : -1

      let nextIdx: number
      if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
        nextIdx = idx < focusables.length - 1 ? idx + 1 : 0
      } else {
        nextIdx = idx > 0 ? idx - 1 : focusables.length - 1
      }

      focusables[nextIdx]?.focus()
      e.preventDefault()
      e.stopPropagation()
    }
  }

  onMounted(() => {
    window.addEventListener('keydown', handleKeydown, true)
  })

  onBeforeUnmount(() => {
    window.removeEventListener('keydown', handleKeydown, true)
  })
}
