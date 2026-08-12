import { ref, readonly } from 'vue'

export type ToastVariant = 'success' | 'error' | 'info'

export interface ToastAction {
  label: string
  onClick: () => void
}

export interface ToastItem {
  id: number
  message: string
  variant: ToastVariant
  dismissing: boolean
  action?: ToastAction
}

const toasts = ref<ToastItem[]>([])
let nextId = 0

function addToast(message: string, variant: ToastVariant = 'info', duration = 3000, action?: ToastAction) {
  const id = nextId++
  toasts.value.push({ id, message, variant, dismissing: false, action })

  // Auto-dismiss
  if (duration > 0) {
    setTimeout(() => dismissToast(id), duration)
  }

  // Cap at 5 visible toasts
  if (toasts.value.length > 5) {
    toasts.value.shift()
  }
}

function dismissToast(id: number) {
  const idx = toasts.value.findIndex(t => t.id === id)
  if (idx === -1) return
  toasts.value[idx]!.dismissing = true
  setTimeout(() => {
    toasts.value = toasts.value.filter(t => t.id !== id)
  }, 300)
}

export function useToast() {
  return {
    toasts: readonly(toasts),
    success: (msg: string) => addToast(msg, 'success'),
    error: (msg: string) => addToast(msg, 'error'),
    info: (msg: string) => addToast(msg, 'info'),
    /** Toast with an action link (e.g. "Finish setup"). Sticks around longer. */
    action: (msg: string, action: ToastAction, opts?: { variant?: ToastVariant; duration?: number }) =>
      addToast(msg, opts?.variant ?? 'success', opts?.duration ?? 15000, action),
    dismiss: dismissToast,
  }
}
