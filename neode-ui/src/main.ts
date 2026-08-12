import { createApp } from 'vue'
import { createPinia } from 'pinia'
import './style.css'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import { displayVersion } from '@/utils/version'

// Clipboard polyfill for HTTP (non-secure) contexts where navigator.clipboard is unavailable
if (!navigator.clipboard) {
  Object.defineProperty(navigator, 'clipboard', {
    value: {
      async writeText(text: string) {
        const ta = document.createElement('textarea')
        ta.value = text
        ta.style.cssText = 'position:fixed;opacity:0'
        document.body.appendChild(ta)
        ta.select()
        document.execCommand('copy')
        document.body.removeChild(ta)
      },
      async readText() { return '' },
    },
  })
}
const app = createApp(App)
const pinia = createPinia()

app.use(pinia)
app.use(router)
app.use(i18n)

// Global version formatter — normalizes version labels to a single "v" prefix
// (some sources already carry one, which produced "vv1.2.3"). Use `$ver(x)` in
// templates instead of hard-coding a `v` prefix.
app.config.globalProperties.$ver = displayVersion

// Keep a small ring buffer of the most recent errors on `window.__archyErrors`
// so a crash that only reproduces inside a specific runtime (e.g. the Android
// companion WebView, where there's no easy console) can be retrieved after the
// fact — read it from chrome://inspect, or we can surface it in a debug view.
interface ArchyErrorEntry { when: string; source: string; message: string; info?: string; stack?: string }
const errorLog: ArchyErrorEntry[] = []
;(window as unknown as { __archyErrors?: ArchyErrorEntry[] }).__archyErrors = errorLog

// On-screen, screenshot-able error overlay. Built with plain DOM (not a Vue
// component) so it still appears when the crash is in Vue itself or in a runtime
// with no dev console (the Android companion WebView — chrome://inspect isn't
// always reachable). The user can read/screenshot the real error and the stack
// straight off the device, and "Copy" puts the full ring buffer on the clipboard.
function showErrorOverlay() {
  const id = 'archy-error-overlay'
  let box = document.getElementById(id)
  if (!box) {
    box = document.createElement('div')
    box.id = id
    box.style.cssText = [
      'position:fixed', 'left:8px', 'right:8px', 'bottom:8px', 'z-index:2147483647',
      'max-height:45vh', 'overflow:auto', 'background:rgba(20,0,0,0.92)', 'color:#ffd7d7',
      'font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace', 'padding:10px 12px',
      'border:1px solid #ff5a5a', 'border-radius:10px', 'white-space:pre-wrap',
      'word-break:break-word', '-webkit-user-select:text', 'user-select:text',
    ].join(';')
    document.body.appendChild(box)
  }
  const latest = errorLog[errorLog.length - 1]
  const dump = errorLog
    .map((e) => `[${e.source}] ${e.message}${e.info ? ` (${e.info})` : ''}${e.stack ? `\n${e.stack.split('\n').slice(0, 4).join('\n')}` : ''}`)
    .join('\n──\n')
  box.innerHTML = ''
  const bar = document.createElement('div')
  bar.style.cssText = 'display:flex;gap:8px;justify-content:space-between;align-items:center;margin-bottom:6px'
  const title = document.createElement('b')
  title.textContent = `⚠ App error (${errorLog.length}) — screenshot or Copy`
  title.style.color = '#ff8a8a'
  const btns = document.createElement('div')
  const mkBtn = (label: string, fn: () => void) => {
    const b = document.createElement('button')
    b.textContent = label
    b.style.cssText = 'margin-left:6px;background:#ff5a5a;color:#1a0000;border:0;border-radius:6px;padding:3px 9px;font-weight:700'
    b.onclick = fn
    return b
  }
  btns.appendChild(mkBtn('Copy', () => { navigator.clipboard?.writeText(JSON.stringify(errorLog, null, 2)) }))
  btns.appendChild(mkBtn('Dismiss', () => { box?.remove() }))
  bar.appendChild(title); bar.appendChild(btns)
  const body = document.createElement('div')
  body.textContent = latest ? dump : 'no error captured'
  box.appendChild(bar); box.appendChild(body)
}

function recordError(source: string, err: unknown, info?: string) {
  const e = err as { message?: string; stack?: string } | undefined
  const message = (e?.message ?? String(err)) || 'unknown error'
  const entry: ArchyErrorEntry = { when: new Date().toISOString(), source, message, info, stack: e?.stack }
  errorLog.push(entry)
  if (errorLog.length > 25) errorLog.shift()
  // Log SILENTLY: a global handler error is almost always something we should
  // fix at the source, not interrupt the user for. Keep the full record on the
  // console + the window.__archyErrors ring buffer, and make the screenshot-able
  // overlay available ON DEMAND (window.__archyShowErrors(), or the debug view)
  // — but do NOT auto-pop a red toast / overlay over the UI. Components that
  // need to tell the user about a *specific, actionable* failure still call
  // toast.error() directly; this catch-all stays out of the way.
  console.error(`[${source}]`, err, info ?? '')
}

// Expose the on-demand error overlay + ring buffer so a crash that only repros
// in a runtime without a console (Android companion WebView) is still
// retrievable: call `window.__archyShowErrors()` to screenshot/Copy them.
;(window as unknown as { __archyShowErrors?: () => void }).__archyShowErrors = () => {
  try { showErrorOverlay() } catch { /* overlay is best-effort */ }
}

app.config.errorHandler = (err, _instance, info) => recordError('Vue Error', err, info)

// Keep --visual-viewport-height in sync with the REAL visible viewport.
// Mobile browsers (notably Brave Android with its custom bottom toolbar)
// misreport dvh, leaving 100dvh layouts taller than the visible area — the
// bottom of every page then hides behind the browser chrome and can never be
// scrolled into view. window.innerHeight shares the reference frame of
// position:fixed elements (the mobile tab bar), so sizing the dashboard off
// it keeps scroll containers and the fixed bars consistent on every browser.
// CSS uses var(--visual-viewport-height, 100dvh) so this is a progressive fix.
function syncViewportHeightVar() {
  try {
    document.documentElement.style.setProperty('--visual-viewport-height', `${window.innerHeight}px`)
  } catch { /* non-browser environment */ }
}
syncViewportHeightVar()
window.addEventListener('resize', syncViewportHeightVar)
window.addEventListener('orientationchange', syncViewportHeightVar)
window.visualViewport?.addEventListener('resize', syncViewportHeightVar)

// After a frontend deploy the browser's cached index.html still points at the
// OLD hashed chunks (e.g. AppSession-Cq93o4ao.js), which 404 — Vite then throws
// "Failed to fetch dynamically imported module". The fix is to reload once so
// the browser pulls the fresh index + chunk map. Guard with sessionStorage so a
// genuinely-broken chunk can't trap us in a reload loop.
function isStaleChunkError(err: unknown): boolean {
  const msg = (err as { message?: string })?.message ?? String(err ?? '')
  return /Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed|ChunkLoadError|dynamically imported module/i.test(msg)
}
function reloadOnceForStaleChunk(err: unknown): boolean {
  if (!isStaleChunkError(err)) return false
  try {
    const KEY = 'archy-chunk-reload-at'
    const last = Number(sessionStorage.getItem(KEY) || '0')
    // Only reload if we haven't already tried in the last 10s (loop guard).
    if (Date.now() - last < 10_000) return false
    sessionStorage.setItem(KEY, String(Date.now()))
  } catch { /* sessionStorage unavailable — reload anyway, once is better than stuck */ }
  // Cache-bust the navigation so the stale index isn't served again.
  location.reload()
  return true
}

// Known-benign environmental noise — expected on some deployments and not
// actionable by the user or us, so it shouldn't even occupy a ring-buffer slot
// (which would push out real errors). The PWA service worker can't register
// over a self-signed cert (it needs a trusted cert or localhost); on those
// nodes the SW/offline cache simply doesn't run, which is fine. Logged at debug
// only. (A trusted cert is the real fix — tracked separately, #56.)
function isBenignEnvironmentError(err: unknown): boolean {
  const msg = (err as { message?: string })?.message ?? String(err ?? '')
  return /Failed to register a ServiceWorker|ServiceWorker.*(SSL|certificate|SecurityError)|An SSL certificate error occurred when fetching the script/i.test(msg)
}

// Vue's errorHandler only catches errors raised synchronously inside Vue's
// lifecycle/reactivity. Async rejections and plain runtime errors (e.g. a JS
// API missing in an older WebView) slip past it, so catch those too.
window.addEventListener('error', (ev) => {
  if (reloadOnceForStaleChunk(ev.error ?? ev.message)) return
  if (isBenignEnvironmentError(ev.error ?? ev.message)) { console.debug('[benign]', ev.message); return }
  recordError('window.error', ev.error ?? ev.message)
})
window.addEventListener('unhandledrejection', (ev) => {
  if (reloadOnceForStaleChunk(ev.reason)) return
  if (isBenignEnvironmentError(ev.reason)) { console.debug('[benign]', ev.reason); return }
  recordError('unhandledrejection', ev.reason)
})

app.mount('#app')
