/**
 * Open a URL in the device's real browser.
 *
 * In a normal mobile/desktop browser this is just `window.open(_blank)`. Inside
 * the Android companion app the page runs in a WebView where `window.open` is
 * unreliable (noopener/noreferrer can suppress onCreateWindow), so the native
 * shell injects a `window.ArchipelagoNative.openExternal(url)` bridge that hands
 * the URL to an ACTION_VIEW intent. We prefer the bridge when present and fall
 * back to `window.open` otherwise — so the working mobile-browser path is
 * untouched.
 */
interface ArchipelagoNativeBridge {
  openExternal?: (url: string) => void
  openInApp?: (url: string) => void
  /** Richer launch (companion ≥0.5.26): catalog icon + display name drive the
   *  native branded loader instead of the site favicon. */
  openInAppEx?: (url: string, iconUrl: string, name: string) => void
  /** Install only the connected node's CA through Android's system prompt. */
  installNodeCertificate?: () => void
}

/** Optional app identity for the native loading screen. */
export interface InAppLaunchMeta {
  iconUrl?: string
  name?: string
}

function nativeBridge(): ArchipelagoNativeBridge | undefined {
  return (window as unknown as { ArchipelagoNative?: ArchipelagoNativeBridge }).ArchipelagoNative
}

/**
 * True when running inside the Android companion app (native WebView shell).
 * The shell injects `window.ArchipelagoNative`; a plain mobile browser / PWA
 * never has it.
 */
export function isCompanionApp(): boolean {
  const native = nativeBridge()
  return !!native && typeof native.openInApp === 'function'
}

export function openExternalUrl(url: string): void {
  if (!url) return
  const native = nativeBridge()
  if (native && typeof native.openExternal === 'function') {
    native.openExternal(url)
    return
  }
  window.open(url, '_blank', 'noopener,noreferrer')
}

/**
 * Launch an app that can't be embedded in an iframe (X-Frame-Options) from a
 * mobile surface — with NO "this app opens in a tab" interstitial.
 *
 * - Android companion: hand it to the in-app WebView (`openInApp`) so it stays
 *   inside Archipelago with the native back/forward/reload/close controls.
 * - Plain mobile browser (PWA): open directly in a new browser tab.
 */
export function openInAppOrNewTab(url: string, meta?: InAppLaunchMeta): void {
  if (!url) return
  const native = nativeBridge()
  // Native WebView.loadUrl requires a complete URL. Browser APIs accept
  // relative paths, so this only surfaced when a same-origin app was launched
  // by the companion bridge.
  let nativeUrl = url
  if (!/^[a-z][a-z\d+.-]*:/i.test(url)) {
    try {
      nativeUrl = new URL(url, window.location.origin).href
    } catch { /* keep as-is */ }
  }
  if (native && typeof native.openInAppEx === 'function' && (meta?.iconUrl || meta?.name)) {
    // Absolutize the icon path so the native shell can fetch it directly.
    const icon = meta.iconUrl ? new URL(meta.iconUrl, window.location.origin).href : ''
    native.openInAppEx(nativeUrl, icon, meta.name ?? '')
    return
  }
  if (native && typeof native.openInApp === 'function') {
    native.openInApp(nativeUrl)
    return
  }
  window.open(url, '_blank', 'noopener,noreferrer')
}

/** Use Android's credential installer when the dashboard runs in the
 * companion. Returns false in an ordinary browser so the caller can preserve
 * the normal file-download behavior. */
export function installCertificateInCompanion(): boolean {
  const native = nativeBridge()
  if (!native || typeof native.installNodeCertificate !== 'function') return false
  native.installNodeCertificate()
  return true
}
