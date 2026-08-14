/**
 * Auto-tab memory — apps observed to refuse iframe embedding.
 *
 * The gate strips frame-blocking headers for gated apps (1.8.1+), but three
 * failure modes survive any header fix: JS frame-busting, apps that must be
 * the top-level origin (OAuth/WebAuthn), and SameSite=Strict session cookies.
 * Cross-origin embed failure is not reliably detectable up front from the
 * browser, so the app session's load-timeout is the detector — and this
 * module is the memory: once an app is seen blocked while RUNNING, it is
 * remembered here and every later launch opens it as a tab app directly from
 * the click (a user gesture, so never popup-blocked). The dead grey pane
 * happens at most once per app.
 *
 * Entries expire after 7 days so a node update that fixes embedding (for
 * example a gate improvement) gets re-probed instead of being remembered as
 * broken forever; a successful iframe load also clears the entry immediately.
 */

const KEY = 'archipelago_auto_new_tab_apps'
const EXPIRY_MS = 7 * 24 * 60 * 60 * 1000

function read(): Record<string, number> {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) || '{}')
    if (raw && typeof raw === 'object' && !Array.isArray(raw)) return raw
  } catch { /* corrupt/unavailable storage reads as empty */ }
  return {}
}

function write(entries: Record<string, number>) {
  try { localStorage.setItem(KEY, JSON.stringify(entries)) } catch { /* full/denied */ }
}

export function isAutoTabApp(id: string): boolean {
  const at = read()[id]
  return typeof at === 'number' && Date.now() - at < EXPIRY_MS
}

export function rememberAutoTabApp(id: string): void {
  const entries = read()
  entries[id] = Date.now()
  write(entries)
}

export function forgetAutoTabApp(id: string): void {
  const entries = read()
  if (id in entries) {
    delete entries[id]
    write(entries)
  }
}
