// Back/forward integration for modals (kiosk, remote browsers, mobile).
//
// Without this, the browser's Back control (mouse side-button on kiosk,
// gesture on mobile, toolbar button in a remote browser) navigates the
// ROUTER while a modal is open — at best closing the whole screen under a
// dialog, at worst leaving the app. The native-app expectation, and what
// the companion webview already provides, is: Back closes the topmost
// dialog first.
//
// Mechanics: opening a modal pushes one history entry (same URL, a depth
// marker in state — router keys are preserved by spreading the existing
// state). A popstate that lands BELOW our depth means the user pressed
// Back over an open modal: close the topmost one. A UI-side close (X,
// backdrop, Esc) consumes its own entry with history.back() so Back never
// needs pressing twice — guarded by the depth marker so it can never eat
// a router entry. One module-level stack serves every BaseModal instance,
// so stacked modals close one per Back, top first.
import { watch, type Ref } from 'vue'

type Entry = { close: () => void }

const stack: Entry[] = []
// Set when a popstate initiated the close: the history entry is already
// gone, so the close-side cleanup must not call history.back() again.
let poppedClose = false
let listening = false
// Vue can replace one modal with another in the same update. Reuse that
// history entry rather than scheduling a Back that would close the new modal.
let pendingCleanup: { depth: number } | null = null

function modalDepth(state: unknown): number {
  return (state as { __archyModal?: number } | null)?.__archyModal ?? 0
}

function ensureListener() {
  if (listening || typeof window === 'undefined') return
  listening = true
  window.addEventListener('popstate', (e) => {
    // Landed at a depth below the open-modal count → this Back was aimed
    // at the topmost modal. One entry per Back press: close exactly one.
    // (A popstate at or above our depth is someone else's navigation —
    // e.g. our own cleanup back, or a forward — leave it alone.)
    if (modalDepth(e.state) < stack.length) {
      const top = stack[stack.length - 1]
      if (top) {
        poppedClose = true
        top.close()
      }
    }
  })
}

/** Call from a modal component with its visibility and close trigger. */
export function useModalHistory(show: Ref<boolean>, close: () => void) {
  ensureListener()
  const entry: Entry = { close }
  watch(show, (open, was) => {
    if (open === was) return
    if (open) {
      stack.push(entry)
      try {
        // Preserve vue-router's own keys in state — clobbering them breaks
        // its scroll restoration and position tracking.
        const replacement = pendingCleanup !== null && modalDepth(window.history.state) === pendingCleanup.depth
        pendingCleanup = null
        const state = { ...(window.history.state ?? {}), __archyModal: stack.length }
        if (replacement) window.history.replaceState(state, '')
        else window.history.pushState(state, '')
      } catch { /* history can throw in exotic embeds — modal still works */ }
    } else {
      const wasTop = stack[stack.length - 1] === entry
      const i = stack.indexOf(entry)
      if (i >= 0) stack.splice(i, 1)
      if (poppedClose) {
        poppedClose = false
        return
      }
      // UI-side close of the top modal: consume the entry we pushed, but
      // only if it is still the current one (a route change after opening
      // moves history past it — backing out then would eat a real entry).
      if (wasTop && modalDepth(window.history.state) > stack.length) {
        const cleanup = { depth: modalDepth(window.history.state) }
        pendingCleanup = cleanup
        queueMicrotask(() => {
          if (pendingCleanup !== cleanup) return
          pendingCleanup = null
          if (modalDepth(window.history.state) === cleanup.depth && cleanup.depth > stack.length) {
            try { window.history.back() } catch { /* same guard as above */ }
          }
        })
      }
    }
  })
}
