/**
 * Remote Relay — receives companion app input via WebSocket and dispatches
 * keyboard/mouse/scroll events into the browser, enabling the NES controller
 * or companion keyboard to drive the web UI from another device.
 */

import { ref } from 'vue'

// xdotool key name → DOM key mapping
const KEY_MAP: Record<string, string> = {
  Return: 'Enter',
  BackSpace: 'Backspace',
  Escape: 'Escape',
  Tab: 'Tab',
  Delete: 'Delete',
  space: ' ',
  Up: 'ArrowUp',
  Down: 'ArrowDown',
  Left: 'ArrowLeft',
  Right: 'ArrowRight',
  Home: 'Home',
  End: 'End',
  Prior: 'PageUp',
  Next: 'PageDown',
  F1: 'F1', F2: 'F2', F3: 'F3', F4: 'F4', F5: 'F5', F6: 'F6',
  F7: 'F7', F8: 'F8', F9: 'F9', F10: 'F10', F11: 'F11', F12: 'F12',
}

/** Reactive: relay WebSocket is connected to the server */
export const relayConnected = ref(false)

/** Reactive: a companion app is actively sending input (received input in last 30s) */
export const companionActive = ref(false)

/** Reactive: input is being received right now (flickers on each event) */
export const companionInputActive = ref(false)

let ws: WebSocket | null = null
let shouldReconnect = true
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
// Exponential backoff for the relay socket. It's a secondary feature (companion
// input), so when the backend is down it must NOT hammer a fixed-interval
// reconnect — that floods the console/network with failed-WS noise for the whole
// outage. Back off 1s → 30s, reset on a successful open. (Mirrors websocket.ts.)
let relayReconnectAttempts = 0
const RELAY_RECONNECT_BASE_MS = 1000
const RELAY_RECONNECT_MAX_MS = 30_000
let cursorEl: HTMLDivElement | null = null
let companionTimeout: ReturnType<typeof setTimeout> | null = null
let inputFlickerTimeout: ReturnType<typeof setTimeout> | null = null

let cursorX = typeof window !== 'undefined' ? window.innerWidth / 2 : 0
let cursorY = typeof window !== 'undefined' ? window.innerHeight / 2 : 0
let cursorHideTimer: ReturnType<typeof setTimeout> | null = null

/**
 * While the companion is actively driving input, suppress the other visible
 * pointer: the native OS cursor sitting wherever the physical mouse left it
 * reads as a second, dead cursor next to the companion's virtual one
 * (user request 2026-08-16). Restores automatically when the companion goes
 * quiet (same 30s window as `companionActive`).
 */
function setNativeCursorSuppressed(on: boolean) {
  document.documentElement.classList.toggle('companion-input-active', on)
}

function markCompanionActive() {
  companionActive.value = true
  companionInputActive.value = true
  setNativeCursorSuppressed(true)

  if (inputFlickerTimeout) clearTimeout(inputFlickerTimeout)
  inputFlickerTimeout = setTimeout(() => { companionInputActive.value = false }, 200)

  if (companionTimeout) clearTimeout(companionTimeout)
  companionTimeout = setTimeout(() => {
    companionActive.value = false
    setNativeCursorSuppressed(false)
  }, 30_000)
}

function createCursor(): HTMLDivElement {
  if (cursorEl) return cursorEl
  const el = document.createElement('div')
  el.id = 'remote-relay-cursor'
  el.style.cssText = `
    position: fixed; z-index: 999999; pointer-events: none;
    width: 20px; height: 20px; border-radius: 50%;
    background: rgba(247, 147, 26, 0.7);
    border: 2px solid rgba(247, 147, 26, 0.9);
    transform: translate(-50%, -50%);
    transition: opacity 0.3s;
    opacity: 0; display: none;
  `
  document.body.appendChild(el)
  cursorEl = el
  return el
}

function showCursor() {
  const el = createCursor()
  el.style.display = 'block'
  el.style.opacity = '1'
  el.style.left = `${cursorX}px`
  el.style.top = `${cursorY}px`

  if (cursorHideTimer) clearTimeout(cursorHideTimer)
  cursorHideTimer = setTimeout(() => {
    if (cursorEl) cursorEl.style.opacity = '0'
  }, 3000)
}

function moveCursor(dx: number, dy: number) {
  cursorX = Math.max(0, Math.min(window.innerWidth, cursorX + dx))
  cursorY = Math.max(0, Math.min(window.innerHeight, cursorY + dy))
  showCursor()
}

function mapKey(xdotoolKey: string): string {
  return KEY_MAP[xdotoolKey] ?? xdotoolKey
}

/** <input> types that accept free-text entry (so we should type into them). */
const TEXT_INPUT_TYPES = new Set([
  'text', 'search', 'url', 'tel', 'password', 'email', 'number', '',
])

export function isTextField(el: Element | null): el is HTMLInputElement | HTMLTextAreaElement {
  if (!el) return false
  if (el.tagName === 'TEXTAREA') return true
  if (el.tagName === 'INPUT') {
    const type = ((el as HTMLInputElement).type || 'text').toLowerCase()
    return TEXT_INPUT_TYPES.has(type)
  }
  return false
}

/**
 * elementFromPoint that descends through SAME-ORIGIN iframes, so the cursor
 * can target elements *inside* embedded apps (gitea, uptime-kuma, AIUI — any
 * app served same-origin via /app/… or /aiui/). Cross-origin iframes (apps on
 * direct ports) are opaque to the parent by browser security policy, so the
 * deepest reachable element there is the <iframe> itself.
 */
function deepElementFromPoint(x: number, y: number): Element | null {
  let cx = x
  let cy = y
  let el = document.elementFromPoint(cx, cy)
  let guard = 0
  while (el && el.tagName === 'IFRAME' && guard++ < 5) {
    let doc: Document | null = null
    try { doc = (el as HTMLIFrameElement).contentDocument } catch { break }
    if (!doc) break
    const rect = el.getBoundingClientRect()
    cx -= rect.left
    cy -= rect.top
    const inner = doc.elementFromPoint(cx, cy)
    if (!inner || inner === el) break
    el = inner
  }
  return el
}

/**
 * Find the nearest scrollable ancestor of `el` for the given delta, hopping out
 * of same-origin iframes when needed. Synthetic WheelEvents are untrusted and
 * never actually scroll the page, so two-finger scroll must call scrollBy on a
 * real scroll container — this locates it (e.g. the right-hand app frame). (#7)
 */
function findScrollable(el: Element | null, dx: number, dy: number): Element | null {
  let node: Element | null = el
  let guard = 0
  while (node && guard++ < 60) {
    const win = node.ownerDocument?.defaultView
    const style = win?.getComputedStyle(node)
    if (style) {
      const oy = style.overflowY
      const ox = style.overflowX
      const isRoot = node === node.ownerDocument?.scrollingElement
      const canY =
        (oy === 'auto' || oy === 'scroll' || isRoot) &&
        node.scrollHeight > node.clientHeight + 1
      const canX =
        (ox === 'auto' || ox === 'scroll' || isRoot) &&
        node.scrollWidth > node.clientWidth + 1
      if ((dy !== 0 && canY) || (dx !== 0 && canX)) return node
    }
    if (node.parentElement) {
      node = node.parentElement
    } else if (win?.frameElement) {
      node = win.frameElement as Element // same-origin iframe → continue in parent doc
    } else {
      break
    }
  }
  return null
}

/** The actually-focused element, descending through same-origin iframes. */
function deepActiveElement(): Element | null {
  let el: Element | null = document.activeElement
  let guard = 0
  while (el && el.tagName === 'IFRAME' && guard++ < 5) {
    let doc: Document | null = null
    try { doc = (el as HTMLIFrameElement).contentDocument } catch { break }
    if (!doc || !doc.activeElement || doc.activeElement === doc.body) break
    el = doc.activeElement
  }
  return el
}

/**
 * Apply a key to a focused text field. Synthetic KeyboardEvents do NOT mutate
 * input values (browser security), so we edit `.value` at the caret directly
 * and fire an `input` event so Vue v-model / reactive search pick it up.
 * Returns true if the key was consumed as text editing.
 */
export function typeKeyIntoField(el: HTMLInputElement | HTMLTextAreaElement, key: string): boolean {
  const value = el.value
  const start = el.selectionStart ?? value.length
  const end = el.selectionEnd ?? value.length
  const setCaret = (pos: number) => { try { el.selectionStart = el.selectionEnd = pos } catch { /* e.g. number inputs */ } }
  const replaceSelection = (text: string) => {
    el.value = value.slice(0, start) + text + value.slice(end)
    setCaret(start + text.length)
  }

  if (key === 'Backspace') {
    if (start !== end) { el.value = value.slice(0, start) + value.slice(end); setCaret(start) }
    else if (start > 0) { el.value = value.slice(0, start - 1) + value.slice(end); setCaret(start - 1) }
    else return true
  } else if (key === 'Delete') {
    if (start !== end) { el.value = value.slice(0, start) + value.slice(end); setCaret(start) }
    else { el.value = value.slice(0, start) + value.slice(start + 1); setCaret(start) }
  } else if (key === 'ArrowLeft') {
    setCaret(Math.max(0, start - 1))
  } else if (key === 'ArrowRight') {
    setCaret(Math.min(value.length, end + 1))
  } else if (key === 'Home') {
    setCaret(0)
  } else if (key === 'End') {
    setCaret(value.length)
  } else if (key === 'Enter') {
    if (el.tagName === 'TEXTAREA') replaceSelection('\n')
    else return false // let the app's keydown handler act (e.g. search submit)
  } else if (key.length === 1) {
    replaceSelection(key) // printable character
  } else {
    return false // Tab / Escape / F-keys / etc. — not text editing
  }
  el.dispatchEvent(new Event('input', { bubbles: true }))
  return true
}

function handleMessage(data: string) {
  let msg: { t: string; k?: string; x?: number; y?: number; b?: number; p?: number }
  try {
    msg = JSON.parse(data)
  } catch {
    return
  }

  if (msg.t === 'ok') return // server ready, not companion input

  markCompanionActive()

  switch (msg.t) {
    case 'k': {
      if (!msg.k) break
      const key = mapKey(msg.k)
      // Dispatch player-tagged event for arcade/game apps (iframe postMessage or direct listeners)
      const player = msg.p ?? 0 // 0 = untagged/broadcast, 1 = P1, 2 = P2
      document.dispatchEvent(new CustomEvent('arcade-input', {
        detail: { key, player, type: 'down' },
        bubbles: true,
      }))
      // Also post to any iframe that might be listening (containerized apps like BotFights)
      const iframe = document.querySelector('iframe') as HTMLIFrameElement | null
      if (iframe?.contentWindow) {
        iframe.contentWindow.postMessage({ type: 'arcade-input', key, player, action: 'down' }, '*')
      }
      // Deliver the key to the actually-focused element (descending into
      // same-origin iframes) so it reaches embedded-app inputs and search
      // boxes, not just the top-level document.
      const focused = deepActiveElement()
      const keyTarget: EventTarget = focused ?? document
      keyTarget.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      // Synthetic key events never insert text, so edit the field directly.
      if (isTextField(focused)) {
        typeKeyIntoField(focused, key)
      }
      keyTarget.dispatchEvent(new KeyboardEvent('keyup', { key, bubbles: true }))
      break
    }
    case 'm': {
      moveCursor(msg.x ?? 0, msg.y ?? 0)
      break
    }
    case 'c': {
      const target = deepElementFromPoint(cursorX, cursorY)
      if (target) {
        if (cursorEl) {
          cursorEl.style.background = 'rgba(247, 147, 26, 1)'
          setTimeout(() => { if (cursorEl) cursorEl.style.background = 'rgba(247, 147, 26, 0.7)' }, 150)
        }
        const eventInit: MouseEventInit = {
          bubbles: true, cancelable: true, view: window,
          clientX: cursorX, clientY: cursorY,
        }
        target.dispatchEvent(new MouseEvent('mousedown', eventInit))
        target.dispatchEvent(new MouseEvent('mouseup', eventInit))
        target.dispatchEvent(new MouseEvent('click', eventInit))
        // A synthetic click does NOT move keyboard focus the way a real click
        // does, so the app-store search box (and any input) would stay
        // unfocused and untypable. Explicitly focus the nearest focusable
        // element — for same-origin iframe targets this focuses inside the app.
        const focusable = (target.closest?.(
          'input, textarea, select, button, a[href], [contenteditable], [tabindex]',
        ) ?? target) as HTMLElement
        if (typeof focusable.focus === 'function') {
          focusable.focus({ preventScroll: true })
        }
      }
      break
    }
    case 's': {
      // Scroll the element under the virtual cursor (incl. inside same-origin
      // app frames like the right-hand panel), not the top document. A synthetic
      // wheel event won't scroll — call scrollBy on a real scroll container. (#7)
      const dy = (msg.y ?? 0) * 100
      const dx = (msg.x ?? 0) * 100
      const start = deepElementFromPoint(cursorX, cursorY)
      const scroller = findScrollable(start, dx, dy)
      if (scroller) {
        scroller.scrollBy({ left: dx, top: dy })
      } else {
        const win = start?.ownerDocument?.defaultView ?? window
        win.scrollBy(dx, dy)
      }
      break
    }
  }
}

function doConnect() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    return
  }

  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  // The kiosk self-identifies so the backend can mute this subscriber's
  // key/click/scroll messages while the CDP bridge delivers them as trusted
  // browser input (cursor moves still arrive so the on-screen cursor draws).
  // Same three-way predicate as App.vue's isKiosk.
  const isKiosk = localStorage.getItem('kiosk') === 'true'
    || new URLSearchParams(window.location.search).has('kiosk')
    || window.location.pathname === '/kiosk'
  const url = `${protocol}//${window.location.host}/ws/remote-relay${isKiosk ? '?kiosk=1' : ''}`

  ws = new WebSocket(url)

  ws.onopen = () => {
    relayConnected.value = true
    relayReconnectAttempts = 0 // healthy again — reset backoff
    if (import.meta.env.DEV) console.log('[RemoteRelay] Connected')
  }

  ws.onmessage = (event) => {
    handleMessage(event.data)
  }

  ws.onclose = () => {
    relayConnected.value = false
    ws = null
    if (shouldReconnect) {
      const delay = Math.min(
        RELAY_RECONNECT_BASE_MS * 2 ** relayReconnectAttempts,
        RELAY_RECONNECT_MAX_MS,
      )
      relayReconnectAttempts++
      reconnectTimer = setTimeout(doConnect, delay)
    }
  }

  ws.onerror = () => {
    // onclose will handle reconnect
  }
}

/**
 * Ask the companion (phone) to open a URL in its own browser.
 *
 * "Open in external browser" apps can't be usefully opened on the kiosk when a
 * companion is driving it — `window.open` lands on the kiosk, which the phone
 * user never sees. When a companion is active we forward the URL over the relay
 * socket ({"t":"o","url"}); the backend routes it to the phone, which opens it.
 *
 * Returns true if the request was forwarded (caller should NOT open locally),
 * false if there's no active companion (caller should open normally).
 */
export function requestExternalOpen(url: string): boolean {
  if (!url || !/^https?:\/\//i.test(url)) return false
  if (!companionActive.value) return false
  if (!ws || ws.readyState !== WebSocket.OPEN) return false
  try {
    ws.send(JSON.stringify({ t: 'o', url }))
    if (import.meta.env.DEV) console.log('[RemoteRelay] Forwarded external-open to companion:', url)
    return true
  } catch {
    return false
  }
}

/** Start the remote relay listener. Connects to /ws/remote-relay. */
export function startRemoteRelay() {
  shouldReconnect = true
  relayReconnectAttempts = 0
  doConnect()
}

/** Stop the remote relay listener and clean up. */
export function stopRemoteRelay() {
  shouldReconnect = false
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null }
  if (companionTimeout) { clearTimeout(companionTimeout); companionTimeout = null }
  if (inputFlickerTimeout) { clearTimeout(inputFlickerTimeout); inputFlickerTimeout = null }
  if (cursorHideTimer) { clearTimeout(cursorHideTimer); cursorHideTimer = null }
  if (ws) { ws.onclose = null; ws.close(); ws = null }
  if (cursorEl) { cursorEl.remove(); cursorEl = null }
  setNativeCursorSuppressed(false)
  relayConnected.value = false
  companionActive.value = false
  companionInputActive.value = false
}
