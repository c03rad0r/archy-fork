/**
 * Promise-based in-app replacement for window.confirm(). Native browser
 * modals block the JS event loop while open, which freezes the companion
 * remote-input relay (and every other async path) — a remote user could
 * raise a confirm they can never dismiss, since the synthetic events that
 * would dismiss it are queued behind the very dialog they need to close.
 *
 * State is module-level so the single <AppConfirmModal /> mounted in
 * App.vue serves every caller:
 *
 *   const ok = await appConfirm('Clear everything? This cannot be undone.')
 *   const ok = await appConfirm({ message: '…', confirmLabel: 'Delete', danger: true })
 */
import { ref } from 'vue'

export interface AppConfirmOptions {
  title?: string
  message: string
  confirmLabel?: string
  cancelLabel?: string
  /** Styles the confirm button as destructive (orange/warning). */
  danger?: boolean
}

interface ActiveConfirm extends AppConfirmOptions {
  _resolve: (value: boolean) => void
}

export const confirmState = ref<ActiveConfirm | null>(null)

export function appConfirm(options: AppConfirmOptions | string): Promise<boolean> {
  const opts = typeof options === 'string' ? { message: options } : options
  // A second confirm while one is open cancels the first — the browser
  // primitive this replaces could only ever show one at a time.
  confirmState.value?._resolve(false)
  return new Promise<boolean>((resolve) => {
    confirmState.value = { ...opts, _resolve: resolve }
  })
}

export function resolveConfirm(result: boolean) {
  confirmState.value?._resolve(result)
  confirmState.value = null
}
