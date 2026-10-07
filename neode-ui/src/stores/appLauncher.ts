import { appHasMediaBridge, ensureNodeAppAvailable, nodeAppIsAvailable } from '@/views/discover/curatedApps'
import { defineStore } from 'pinia'
import { ref, watch, onScopeDispose } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { recordAppLaunch } from '@/utils/appUsage'
import { requestExternalOpen } from '@/api/remote-relay'
import { openInAppOrNewTab, isCompanionApp, type InAppLaunchMeta } from '@/utils/openExternal'
import { directAppUrl, HOST_FRAME_APPS, HTTPS_APP_IDS, resolveAppUrl } from '@/views/appSession/appSessionConfig'
import { appPortIsGateFronted } from '@/views/appSession/appSessionConfig'
import { useAppStore } from '@/stores/app'
import { resolveAppIcon, isAppReadyForLaunch } from '@/views/apps/appsConfig'
import { useToast } from '@/composables/useToast'
import { IS_DEMO, isDemoApp, isDemoExternal, demoAppUrl } from '@/composables/useDemoIntro'
import type { AppCredential, AppCredentialsResponse } from '@/types/api'
import { resolveAppCredentials } from '@/views/apps/appCredentials'
import { useNostrBridge } from '@/views/appSession/useNostrBridge'
import { useMediaRegistrationBridge } from '@/composables/useMediaRegistrationBridge'
import { useRentalPurchaseBridge } from '@/composables/useRentalPurchaseBridge'
import type { SelectedIdentity } from '@/views/appSession/useAppIdentity'

/**
 * Open a URL in a new browser tab — but if a companion (phone) is currently
 * driving this kiosk, hand the URL to the phone instead so it opens in the
 * phone's browser rather than the (often headless / unattended) kiosk display.
 * Falls back to a local `window.open` when no companion is active.
 */
function openExternal(launchUrl: string) {
  // Resolve to an absolute URL so the phone can open it (window.open also
  // handles absolute URLs fine).
  let absolute = launchUrl
  try {
    absolute = new URL(launchUrl, window.location.origin).href
  } catch {
    /* keep as-is */
  }
  if (requestExternalOpen(absolute)) return
  window.open(launchUrl, '_blank', 'noopener,noreferrer')
}

/** Whether a postMessage sender's origin belongs to the app the launcher
 *  actually opened. Same hostname and port are REQUIRED; the SCHEME is
 *  deliberately not compared: a browser with cached HSTS (or any scheme
 *  upgrade) loads a stored http:// app URL as https://, and strict equality
 *  silently dropped every nostr request from the upgraded frame — nostr
 *  sign-in on IndeeHub died exactly there over HTTPS (2026-09-01). */
export function senderMatchesApp(appUrl: string, senderOrigin: string): boolean {
  let expected: URL
  let sender: URL
  try {
    expected = new URL(appUrl, 'http://localhost/')
    sender = new URL(senderOrigin)
  } catch {
    return false
  }
  return sender.hostname === expected.hostname && sender.port === expected.port
}

/** Ports of apps that set X-Frame-Options (can't iframe, must open in new tab) */
const NEW_TAB_PORTS = new Set([
  '23000', // BTCPay — X-Frame-Options: DENY
  '3000',  // Grafana — X-Frame-Options: deny
  '2342',  // PhotoPrism — X-Frame-Options: DENY
  '8123',  // Home Assistant — X-Frame-Options: SAMEORIGIN
  '8082',  // Vaultwarden — X-Frame-Options: SAMEORIGIN
  '8085',  // Nextcloud — X-Frame-Options: SAMEORIGIN
  '3002',  // Uptime Kuma — X-Frame-Options: SAMEORIGIN
  '9001',  // Penpot — not reachable
  // Port 7777 is the Nostr relay; IndeeHub's web UI is exposed on 7778.
])

const NEW_TAB_APP_IDS = new Set([
  'btcpay-server',
  'grafana',
  'photoprism',
  'homeassistant',
  'vaultwarden',
  'nextcloud',
  'portainer',
  'tailscale',
  'nginx-proxy-manager',
  'uptime-kuma',
  'gitea',
  // netbird's dashboard needs a secure context (window.crypto.subtle for OIDC
  // PKCE), so it's served over HTTPS and must open in a real tab — a
  // self-signed-HTTPS iframe is blocked by the browser (you can't accept the
  // cert warning inside an iframe).
  'netbird',
])

/** Apps whose launch may require a platform-owned credential handoff. Keep
 * this list deliberately narrow so ordinary new-tab launches retain their
 * original synchronous user gesture. Portainer is dynamic (first-run only);
 * File Browser and PhotoPrism have stable fallback credentials. */
export const CREDENTIAL_INTERSTITIAL_APPS = new Set([
  'datum',
  'filebrowser',
  'photoprism',
  'portainer',
])

interface LaunchOptions {
  path?: string
  /** The shared interstitial already ran and the user pressed Continue. */
  skipCredentialPrompt?: boolean
}

function mustOpenInNewTab(url: string): boolean {
  try {
    const u = new URL(url)
    return NEW_TAB_PORTS.has(u.port)
  } catch {
    return false
  }
}

function isMobileViewport(): boolean {
  return typeof window !== 'undefined' && window.innerWidth < 768
}

function inferAppIdFromTitle(title?: string): string | null {
  const t = (title || '').toLowerCase()
  if (!t) return null
  if (t.includes('indeehub') || t.includes('indeedhub')) return 'indeedhub'
  if ((t.includes('uptime') && t.includes('kuma')) || t.includes('uptime-kuma')) return 'uptime-kuma'
  if ((t.includes('nginx') && t.includes('proxy') && t.includes('manager')) || t.includes('nginx-proxy-manager')) return 'nginx-proxy-manager'
  if (t.includes('gitea')) return 'gitea'
  return null
}

function normalizeLaunchUrl(urlStr: string, appIdHint?: string | null): string {
  try {
    const u = new URL(urlStr)
    let rewrittenLocalhost = false
    if (u.hostname === 'localhost' || u.hostname === '127.0.0.1') {
      u.hostname = window.location.hostname
      rewrittenLocalhost = true
    }
    const sameHost = u.hostname === window.location.hostname
    const normalizedPath = u.pathname === '/' ? '' : u.pathname
    const rebuilt = (port: string) => `${u.protocol}//${u.hostname}:${port}${normalizedPath}${u.search}${u.hash}`

    if (sameHost && appIdHint === 'indeedhub' && u.port === '7777') {
      return rebuilt('7778')
    }

    if (sameHost && appIdHint === 'uptime-kuma' && u.port === '3001') {
      return rebuilt('3002')
    }

    if (sameHost && appIdHint === 'nginx-proxy-manager' && (u.port === '81' || u.port === '8181')) {
      return rebuilt('8081')
    }

    return rewrittenLocalhost ? u.toString() : urlStr
  } catch {
    return urlStr
  }
}

/** Port → app ID for resolving URLs to AppSession routes */
const PORT_TO_APP_ID: Record<string, string> = {
  '81': 'nginx-proxy-manager',
  '8081': 'nginx-proxy-manager',
  '8181': 'nginx-proxy-manager',
  '3000': 'grafana',
  '3002': 'uptime-kuma',
  '8080': 'endurain',
  '18083': 'lnd',
  '8082': 'vaultwarden',
  '8083': 'filebrowser',
  '8085': 'nextcloud',
  '8096': 'jellyfin',
  '8123': 'homeassistant',
  '8240': 'tailscale',
  '8334': 'bitcoin-knots',
  '8337': 'archipelago-source',
  '8888': 'searxng',
  '9000': 'portainer',
  '8087': 'netbird',
  '8086': 'netbird',
  '11434': 'ollama',
  '2283': 'immich',
  '23000': 'btcpay-server',
  '2342': 'photoprism',
  '4080': 'mempool',
  '8175': 'fedimint',
  '8176': 'fedimint-gateway',
  '7778': 'indeedhub',
  '50002': 'electrumx',
}

/** App identity (catalog icon + display name) for the companion's native
 *  branded loader. Undefined when the app isn't in package-data. */
function launchMeta(appId: string): InAppLaunchMeta | undefined {
  const pkg = useAppStore().data?.['package-data']?.[appId]
  if (!pkg) return undefined
  return {
    iconUrl: resolveAppIcon(appId, pkg),
    name: pkg.manifest?.title || appId,
  }
}

export const useAppLauncherStore = defineStore('appLauncher', () => {
  const isOpen = ref(false)
  const url = ref('')
  const title = ref('')
  const credentialPrompt = ref({
    show: false,
    loading: false,
    appId: '',
    title: '',
    description: '',
    credentials: [] as AppCredential[],
    copied: '',
  })
  let pendingCredentialLaunch: { appId: string; path?: string } | null = null
  let credentialGeneration = 0
  let previousActiveElement: HTMLElement | null = null

  /** Active app in the store-driven session (no route change) */
  const panelAppId = ref<string | null>(null)
  const mediaAppId = ref<string | null>(null)
  /** Optional deep-link path inside the active app (e.g. /tx/<hash> for mempool) */
  const panelPath = ref<string | null>(null)
  let launchGeneration = 0

  function openSessionNow(appId: string, opts: LaunchOptions = {}) {
    const pkg = useAppStore().data?.['package-data']?.[appId]
    if (pkg?.['ui-ready'] === false) {
      useToast().info(`${pkg.manifest?.title || appId} is not ready to open yet`)
      return
    }
    recordAppLaunch(appId)
    const mobile = isMobileViewport()

    // Companion app: ordinary apps open in the native in-app WebView for the
    // phone controls and better performance. Apps with manifest-declared host
    // integrations stay in the dashboard frame so their parent bridge remains
    // connected (for example GitWorkshop's consent-gated NIP-07 provider).
    if (!IS_DEMO && isCompanionApp() && !HOST_FRAME_APPS.has(appId) && !appHasMediaBridge(appId)) {
      const runtimeUrl = useAppStore().data?.['package-data']?.[appId]?.installed?.['interface-addresses']?.main?.['lan-address'] || undefined
      const launchUrl = directAppUrl(appId) || resolveAppUrl(appId, opts.path, runtimeUrl)
      if (launchUrl) {
        openInAppOrNewTab(launchUrl, launchMeta(appId))
        return
      }
    }

    // Demo: apps backed by a real external site that blocks iframing open
    // externally; everything else demoable renders in the in-app session.
    if (IS_DEMO && isDemoExternal(appId)) {
      const ext = demoAppUrl(appId)
      if (ext) {
        if (mobile) openInAppOrNewTab(ext, launchMeta(appId))
        else openExternal(ext)
        return
      }
    }
    // Tab-only apps (set X-Frame-Options, can't be iframed). No interstitial:
    // desktop opens a new browser tab; mobile opens the in-app WebView (Android
    // companion) or a new browser tab (PWA) — see openInAppOrNewTab.
    // In the demo, demoable apps are served same-origin by the mock backend
    // (no framing headers), so they always render in the in-app session.
    if (NEW_TAB_APP_IDS.has(appId) && !(IS_DEMO && isDemoApp(appId))) {
      const launchUrl = directAppUrl(appId)
      if (launchUrl) {
        if (mobile) openInAppOrNewTab(launchUrl, launchMeta(appId))
        else openExternal(launchUrl)
        return
      }
    }

    // Iframeable apps always use the store-driven session so the underlying
    // page never changes: panel mode renders beside the page, overlay and
    // fullscreen modes render above it (AppSession styles per display mode).
    // Closing always returns the user exactly where they launched from.
    if (appHasMediaBridge(appId)) mediaAppId.value = appId
    panelPath.value = opts.path ?? null
    panelAppId.value = appId
  }

  /** One launch gate for Home, My Apps, Discover, Spotlight and details.
   * Previously each Apps view owned a private modal, so Home skipped the
   * Portainer first-run token entirely. */
  function openSession(appId: string, opts: LaunchOptions = {}) {
    const generation = ++launchGeneration
    if (appId.startsWith('node-demo-') && !nodeAppIsAvailable(appId)) {
      useToast().info('Loading this node’s demo app…')
      void ensureNodeAppAvailable(appId).then(available => {
        if (generation !== launchGeneration) return
        if (!available) {
          useToast().error('This node’s demo catalog is unavailable. Please retry.')
          return
        }
        openSession(appId, opts)
      })
      return
    }
    // Home/goal/deep-link launchers do not pass through AppCard.canLaunch.
    // Apply the same readiness gate here so a container that has just entered
    // `running` cannot race nginx and show a transient 502 to the user.
    const pkg = useAppStore().data?.['package-data']?.[appId]
    if (pkg && (pkg['ui-ready'] === false || (pkg.state === 'running' && !isAppReadyForLaunch(pkg)))) {
      useToast().info(`${pkg.manifest?.title || appId} is still starting — try again in a moment`)
      return
    }
    if (!opts.skipCredentialPrompt && CREDENTIAL_INTERSTITIAL_APPS.has(appId)) {
      void prepareCredentialLaunch(appId, opts.path)
      return
    }
    openSessionNow(appId, opts)
  }

  async function prepareCredentialLaunch(appId: string, path?: string) {
    const generation = ++credentialGeneration
    const appName = useAppStore().data?.['package-data']?.[appId]?.manifest?.title || appId
    pendingCredentialLaunch = { appId, path }
    credentialPrompt.value = {
      show: true,
      loading: true,
      appId,
      title: `Checking ${appName}`,
      description: 'Checking whether this app needs a first-run token or login details…',
      credentials: [],
      copied: '',
    }

    let result: AppCredentialsResponse | null
    try {
      // Portainer's token is lifecycle-dependent, so this must be live on
      // every launch. Caching a pre-initialisation null (or an already-used
      // token) recreates the skipped/stale interstitial bug.
      result = await rpcClient.call<AppCredentialsResponse>({
        method: 'package.credentials',
        params: { app_id: appId },
        timeout: 5000,
      })
    } catch {
      result = null
    }
    if (generation !== credentialGeneration) return

    const resolved = resolveAppCredentials(appId, result)
    if (!resolved) {
      credentialPrompt.value.show = false
      pendingCredentialLaunch = null
      openSessionNow(appId, { path, skipCredentialPrompt: true })
      return
    }
    credentialPrompt.value = {
      show: true,
      loading: false,
      appId,
      title: resolved.title || `${appName} credentials`,
      description: resolved.description || 'Use these credentials when the app asks you to sign in.',
      credentials: resolved.credentials,
      copied: '',
    }
  }

  function cancelCredentialLaunch() {
    credentialGeneration += 1
    pendingCredentialLaunch = null
    credentialPrompt.value.show = false
    credentialPrompt.value.loading = false
  }

  function continueCredentialLaunch() {
    const pending = pendingCredentialLaunch
    credentialGeneration += 1
    pendingCredentialLaunch = null
    credentialPrompt.value.show = false
    credentialPrompt.value.loading = false
    if (pending) openSessionNow(pending.appId, { path: pending.path, skipCredentialPrompt: true })
  }

  async function copyCredential(label: string, value: string) {
    try {
      await navigator.clipboard.writeText(value)
    } catch {
      const textarea = document.createElement('textarea')
      textarea.value = value
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand('copy')
      document.body.removeChild(textarea)
    }
    credentialPrompt.value.copied = label
  }

  function closePanel() {
    launchGeneration += 1
    panelAppId.value = null
    panelPath.value = null
  }

  /** Legacy: open app in iframe overlay (kept for backward compat) */
  function open(payload: { url: string; title: string; openInNewTab?: boolean }) {
    const titleHintId = inferAppIdFromTitle(payload.title)
    let launchUrl = normalizeLaunchUrl(payload.url, titleHintId)
    const resolvedId = resolveAppIdFromUrl(launchUrl) || titleHintId

    const pkg = resolvedId ? useAppStore().data?.['package-data']?.[resolvedId] : undefined
    if (pkg?.['ui-ready'] === false) {
      useToast().info(`${pkg.manifest?.title || resolvedId} is not ready to open yet`)
      return
    }

    // Scheme discipline for everything launched on this host. Ports fronted
    // by the node's app gate (manifest auth gated/open) serve TLS on the same
    // port — on an HTTPS connection those must open over https. Ports that
    // are NOT gate-fronted (legacy curated installs like Nginx Proxy Manager,
    // Tailscale; `auth: none` publishes) are plain HTTP and https would fail
    // to connect outright, so they keep http. External hosts keep their own
    // scheme.
    try {
      const u = new URL(launchUrl, window.location.origin)
      const sameHost = u.hostname === window.location.hostname
      const alwaysHttps = !!resolvedId && HTTPS_APP_IDS.has(resolvedId)
      const httpsPage = window.location.protocol === 'https:'
      const gateFronted = !!resolvedId && appPortIsGateFronted(resolvedId, u.port)
      if (u.protocol === 'http:' && sameHost && (alwaysHttps || (httpsPage && gateFronted))) {
        // Pure prefix swap — never re-serialize the URL (URL.href would add
        // a trailing slash and change the string the caller handed over).
        launchUrl = launchUrl.replace(/^http:\/\//i, 'https://')
      }
    } catch { /* leave as-is */ }

    if (!isMobileViewport() && payload.openInNewTab) {
      if (resolvedId) recordAppLaunch(resolvedId)
      openExternal(launchUrl)
      return
    }

    // Force selected apps to open directly in new tab on desktop only. On
    // phones, route through the app session/webview so app icons behave like
    // native launchers and keep the user inside Archipelago.
    if (!isMobileViewport() && resolvedId && NEW_TAB_APP_IDS.has(resolvedId)) {
      recordAppLaunch(resolvedId)
      openExternal(launchUrl)
      return
    }

    // Open the in-app session if we can resolve an app ID from the URL,
    // carrying through any deep-link path (e.g. /tx/<hash>).
    if (resolvedId) {
      let deepPath: string | undefined
      try {
        const u = new URL(launchUrl, window.location.origin)
        let p = `${u.pathname}${u.search}${u.hash}`
        // /app/<id>/-style URLs carry the mount prefix — strip it so only the
        // app-internal path is treated as the deep link.
        const prefix = `/app/${resolvedId}`
        if (p.startsWith(prefix)) p = p.slice(prefix.length) || '/'
        if (p && p !== '/') deepPath = p
      } catch { /* no deep link */ }
      openSession(resolvedId, { path: deepPath })
      return
    }

    // Unknown apps that block iframes — open directly in new tab
    if (!isMobileViewport() && mustOpenInNewTab(launchUrl)) {
      openExternal(launchUrl)
      return
    }

    // Companion app: never fall through to the iframe overlay — hand the URL
    // to the native in-app WebView instead (see openSession).
    if (!IS_DEMO && isCompanionApp()) {
      openInAppOrNewTab(launchUrl, resolvedId ? launchMeta(resolvedId) : undefined)
      return
    }

    previousActiveElement = (document.activeElement as HTMLElement) || null
    url.value = launchUrl
    title.value = payload.title
    isOpen.value = true
  }

  /** Resolve an app ID from a URL (port or known external) */
  function resolveAppIdFromUrl(urlStr: string): string | null {
    try {
      const u = new URL(urlStr)
      // Check /app/{id}/ path-style routes first (HTTPS proxy mode)
      const m = u.pathname.match(/^\/app\/([a-z0-9._-]+)(?:\/|$)/i)
      if (m?.[1]) return m[1].toLowerCase()
      // Dynamic/sideloaded apps have no entry in the static port map.
      if (u.hostname === window.location.hostname && u.port) {
        for (const [id, pkg] of Object.entries(useAppStore().data?.['package-data'] || {})) {
          const address = pkg.installed?.['interface-addresses']?.main?.['lan-address']
          if (!address) continue
          try {
            const runtime = new URL(address)
            if (runtime.port === u.port && ['localhost', '127.0.0.1', window.location.hostname].includes(runtime.hostname)) return id
          } catch { /* malformed runtime address is not a launch target */ }
        }
      }
      // Check port-based apps
      const appId = PORT_TO_APP_ID[u.port]
      if (appId) return appId
      // Check external URLs
      const EXTERNAL_APP_HOSTS: Record<string, string> = {
        'botfights.net': 'botfights',
        'nostrudel.ninja': 'nostrudel',
      }
      return EXTERNAL_APP_HOSTS[u.hostname] || null
    } catch { return null }
  }

  function close() {
    bridge.cancelPending()
    registrationBridge.cancel(); rentalBridge.cancel()
    const toRestore = previousActiveElement
    previousActiveElement = null
    isOpen.value = false
    url.value = ''
    title.value = ''
    // Explicitly remove NIP-07 listener as safety net — if user navigates away
    // without close() triggering the isOpen watcher, the listener would leak
    window.removeEventListener('message', handleNativeRequest)
    if (toRestore && typeof toRestore.focus === 'function') {
      requestAnimationFrame(() => {
        toRestore.focus()
      })
    }
  }

  // Reuse the same bounded queue and consent lifecycle as AppSession. Keep
  // the legacy URL-based identity storage key so existing selections survive.
  let nostrFrame: Window | null = null
  function overlayIdentity(): SelectedIdentity | null {
    try {
      const key = 'archipelago_app_identity_' + url.value.replace(/[^a-z0-9]/gi, '_')
      const stored = JSON.parse(localStorage.getItem(key) || 'null')
      if (!stored || typeof stored.id !== 'string' || !stored.id) return null
      return { ...stored, name: typeof stored.name === 'string' ? stored.name : stored.id }
    } catch { return null }
  }
  const nativeIdentityBusy = ref(false)
  const registrationBridge = useMediaRegistrationBridge({
    consentBusy: (): boolean => rentalBridge.isBusy(),
    appId: () => resolveAppIdFromUrl(url.value) || inferAppIdFromTitle(title.value) || 'unknown-app',
    appUrl: () => url.value, frameWindow: () => isOpen.value ? nostrFrame : null,
  })
  const rentalBridge = useRentalPurchaseBridge({
    consentBusy: (): boolean => bridge.showConsent.value || nativeIdentityBusy.value || registrationBridge.isBusy(),
    appId: () => resolveAppIdFromUrl(url.value) || inferAppIdFromTitle(title.value) || 'unknown-app',
    appUrl: () => url.value, frameWindow: () => isOpen.value ? nostrFrame : null,
  })
  const bridge = useNostrBridge(overlayIdentity, {
    appId: () => resolveAppIdFromUrl(url.value) || inferAppIdFromTitle(title.value) || 'unknown-app',
    appName: () => title.value || 'App',
    appUrl: () => url.value,
    frameWindow: () => isOpen.value ? nostrFrame : null,
  })
  const { handleNostrRequest, showConsent, consentRequest, consentPhase,
    consentError, approveConsent, denyConsent } = bridge

  function handleNativeRequest(event: MessageEvent) {
    handleNostrRequest(event)
    void registrationBridge.handle(event); void rentalBridge.handle(event)
  }

  function setNostrFrame(frame: Window | null) {
    if (frame === nostrFrame) return
    bridge.cancelPending()
    registrationBridge.cancel(); rentalBridge.cancel()
    nostrFrame = frame
  }
  watch(url, () => { bridge.cancelPending(); registrationBridge.cancel(); rentalBridge.cancel() }, { flush: 'sync' })
  onScopeDispose(() => {
    window.removeEventListener('message', handleNativeRequest)
    bridge.dispose()
    registrationBridge.dispose(); rentalBridge.dispose()
    nostrFrame = null
  })

  // Listen for NIP-07 requests only while an app is open
  watch(isOpen, (open) => {
    if (open) {
      window.addEventListener('message', handleNativeRequest)
    } else {
      window.removeEventListener('message', handleNativeRequest)
      bridge.cancelPending()
    registrationBridge.cancel(); rentalBridge.cancel()
    }
  }, { flush: 'sync' })

  return {
    isOpen,
    url,
    title,
    open,
    openSession,
    close,
    closePanel,
    panelAppId,
    mediaAppId,
    panelPath,
    credentialPrompt,
    cancelCredentialLaunch,
    continueCredentialLaunch,
    copyCredential,
    showConsent,
    consentRequest,
    consentPhase,
    consentError,
    approveConsent,
    denyConsent,
    setNativeIdentityBusy: (busy: boolean) => { nativeIdentityBusy.value = busy },
    rentalRequest: rentalBridge.request, rentalQuote: rentalBridge.quote, rentalPhase: rentalBridge.phase, rentalProgress: rentalBridge.progress,
    cancelUnpaidRental: rentalBridge.cancelUnpaid, rentalError: rentalBridge.error, reviewRental: rentalBridge.review, approveRental: rentalBridge.approve, cancelRental: rentalBridge.cancel,
    registrationRequest: registrationBridge.request,
    registrationPhase: registrationBridge.phase,
    registrationError: registrationBridge.error,
    approveRegistration: registrationBridge.approve,
    approveRegistrationResolution: registrationBridge.approveResolution,
    cancelRegistration: registrationBridge.cancel,
    setNostrFrame,
  }
})
