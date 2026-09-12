/** Static configuration maps for app session routing and display */

import { portIsGateFronted } from '../discover/curatedApps'
import {
  GENERATED_APP_PORTS,
  GENERATED_APP_TITLES,
  GENERATED_HOST_FRAME_APPS,
  GENERATED_NEW_TAB_APPS,
} from './generatedAppSessionConfig'
import { IS_DEMO, demoAppUrl } from '@/composables/useDemoIntro'

export type DisplayMode = 'panel' | 'overlay' | 'fullscreen'

export const DISPLAY_MODE_KEY = 'archipelago_app_display_mode'

/** Per-app default display mode. Used when the user hasn't explicitly picked
 *  a mode for that app (an explicit pick is remembered per app and wins).
 *  Apps not listed default to 'panel'. */
export const APP_DEFAULT_DISPLAY_MODE: Record<string, DisplayMode> = {
}

/** Initial display mode for an app session: per-app user choice → per-app
 *  default → panel. Strictly per-app — deliberately NO global fallback, so
 *  one app's mode change can never affect how another app opens. */
export function initialDisplayMode(id: string): DisplayMode {
  const perApp = localStorage.getItem(`${DISPLAY_MODE_KEY}:${id}`) as DisplayMode | null
  if (perApp === 'panel' || perApp === 'overlay' || perApp === 'fullscreen') return perApp
  return APP_DEFAULT_DISPLAY_MODE[id] ?? 'panel'
}

/** Container apps: manifest-generated launch ports plus overrides for companions and aliases. */
export const APP_PORTS: Record<string, number> = {
  ...GENERATED_APP_PORTS,
  'bitcoin-knots': 8334,
  'bitcoin-core':  8334,
  'bitcoin-ui':    8334,
  'electrumx':     50002,
  'electrs':       50002,
  'archy-electrs-ui': 50002,
  'mempool-electrs': 50002,
  'lnd':           18083,
  'archy-lnd-ui':  18083,
  'mempool-web':   4080,
  'ollama':        11434,
  'immich':        2283,
  'immich_server': 2283,
  'nginx-proxy-manager': 8081,
  'netbird':       8087,
  'tailscale':     8240,
  'fedimintd':     8175,
  'fedimint-gateway': 8176,
  'endurain':      8080,
}

/** Apps that need nginx proxy for iframe embedding.
 *  IndeeHub web UI is on 7778. Port 7777 is the Nostr relay. */
export const PROXY_APPS: Record<string, string> = {
  'archipelago-source': '/app/archipelago-source/',
  'gitea': '/app/gitea/',
  'nginx-proxy-manager': '/app/nginx-proxy-manager/',
  'uptime-kuma': '/app/uptime-kuma/',
}

/** The repository shown when GitWorkshop is opened from the app launcher. */
export const DEFAULT_GITWORKSHOP_REPO_PATH =
  '/npub1w3sqdkrhn0gyuvsex32effzgnfpyde6qrrc4u467flg5e9txh4wsfn5vjg/relay.ngit.dev/archy'

/** App launches use direct ports. Do not route through /app/... path proxies. */
export const HTTPS_PROXY_PATHS: Record<string, string> = {
}

/**
 * First-party apps that are deliberately being node-tested before their
 * manifest reaches the release-signed catalog. Keep this list narrow: it is
 * only a scheme-routing fallback, and does not make an app installable or
 * trusted. Once the signed catalog carries the app, portIsGateFronted is the
 * normal source of truth.
 */
const PRE_CATALOG_GATED_PORTS: Record<string, number> = {
  'archipelago-source': 8337,
}

export function appPortIsGateFronted(appId: string, port: number | string): boolean {
  return portIsGateFronted(appId, port) || PRE_CATALOG_GATED_PORTS[appId] === Number(port)
}

/** External HTTPS apps -- always loaded directly */
export const EXTERNAL_URLS: Record<string, string> = {
  'nostrudel': 'https://nostrudel.ninja',
}

export const APP_TITLES: Record<string, string> = {
  ...GENERATED_APP_TITLES,
  'bitcoin-knots': 'Bitcoin Knots', 'bitcoin-core': 'Bitcoin Core',
  'btcpay-server': 'BTCPay Server', 'indeedhub': 'Indeehub',
  'botfights': 'BotFights', 'gitea': 'Gitea',
  'homeassistant': 'Home Assistant', 'uptime-kuma': 'Uptime Kuma',
  'nginx-proxy-manager': 'Nginx Proxy Manager',
  'nostrudel': 'noStrudel',
}

/** Apps that set X-Frame-Options and MUST open in a new tab (can't iframe) */
export const NEW_TAB_APPS = new Set([
  ...GENERATED_NEW_TAB_APPS,
  'nginx-proxy-manager',
  'tailscale',
])

/** Apps that consume an integration supplied by the dashboard parent frame.
 * The Android companion normally promotes sessions into a top-level native
 * WebView; doing that to one of these apps would sever its postMessage bridge. */
export const HOST_FRAME_APPS = new Set([
  ...GENERATED_HOST_FRAME_APPS,
])

/** Sites known to block iframes -- skip the timeout and go straight to fallback */
export const IFRAME_BLOCKED_APPS = new Set<string>([])

/** Resolve app URL using direct port mapping (source of truth) */
export function resolveAppUrl(id: string, routeQueryPath?: string, runtimeUrl?: string): string {
  // Demo: route to the app's mock UI or real external site (mempool.space,
  // indee.tx1138.com). Carry through a deep-link path (e.g. /tx/<hash> for
  // mempool). Non-demoable apps fall through to a generic notice page.
  if (IS_DEMO) {
    const base = demoAppUrl(id)
    if (base) {
      if (!routeQueryPath) return base
      // Join without a double slash (/app/mempool/ + /tx/x → /app/mempool/tx/x)
      return base.replace(/\/+$/, '') + (routeQueryPath.startsWith('/') ? routeQueryPath : '/' + routeQueryPath)
    }
    return `/app/${id}/`
  }

  // External HTTPS apps
  const ext = EXTERNAL_URLS[id]
  if (ext) return ext

  // GitWorkshop is deliberately mounted below the dashboard origin. This is
  // the only launch shape that survives every supported ingress (LAN,
  // Tailscale, FIPS, Tor and reverse proxies) without assuming that a second
  // high port is reachable through the same address.
  if (id === 'archipelago-source') {
    const base = PROXY_APPS['archipelago-source']!
    const path = routeQueryPath || DEFAULT_GITWORKSHOP_REPO_PATH
    return base.replace(/\/+$/, '') + (path.startsWith('/') ? path : `/${path}`)
  }

  // Bitcoin UI is a host-network companion on :8334. Do not launch it via
  // /app/bitcoin-ui/: the static UI is built for root and renders a blank
  // shell when proxied under a path prefix on some nodes.
  if (id === 'bitcoin-knots' || id === 'bitcoin-core' || id === 'bitcoin-ui') {
    if (import.meta.env.DEV) return '/app/bitcoin-ui/'
    return appOrigin(8334, id)
  }

  if (runtimeUrl && id !== 'netbird') {
    let base = runtimeUrl.replace(/localhost/i, window.location.hostname)
    // The backend reports runtime URLs as http:// because that is how the app
    // binds locally. On an HTTPS dashboard that is mixed content and the
    // frame is blocked outright — but ONLY upgrade when the gate fronts the
    // port (it serves TLS there); a container-published plain-HTTP port
    // would fail to connect over https at all.
    try {
      const port = new URL(base).port
      if (appPortIsGateFronted(id, port)) base = matchPageScheme(base)
    } catch { /* keep as-is */ }
    if (routeQueryPath) base += routeQueryPath
    return base
  }

  // Local apps launch by host port.
  const port = APP_PORTS[id]
  if (!port) return ''

  let base = appOrigin(port, id)
  if (routeQueryPath) base += routeQueryPath
  return base
}

/**
 * An app's origin on this host, on the SAME scheme as the page when the
 * app gate fronts the port (TLS on the same port), plain http otherwise.
 *
 * An HTTPS dashboard cannot embed an HTTP frame at all — browsers block it as
 * mixed content before any cookie question arises — and it is also what makes
 * the two origins schemefully cross-site, so the session cookie is withheld.
 * Following the page's scheme fixes both at once and keeps plain HTTP working
 * exactly as before on nodes that serve the dashboard over HTTP.
 *
 * On HTTPS this requires the app port to actually serve TLS with a certificate
 * the browser trusts — see scripts/setup-node-ca.sh and Settings → System →
 * Node certificate. A certificate warning cannot be accepted inside an iframe,
 * so an untrusted app port renders nothing rather than prompting.
 */
export function appOrigin(port: number, appId?: string): string {
  const https = appId
    ? HTTPS_APP_IDS.has(appId) || (appPortIsGateFronted(appId, port) && pageScheme() === 'https:')
    : pageScheme() === 'https:'
  return `${https ? 'https' : 'http'}://${window.location.hostname}:${port}`
}

/** Rewrite a URL's scheme to the page's, leaving everything else alone. */
export function matchPageScheme(url: string): string {
  if (pageScheme() !== 'https:') return url
  return url.replace(/^http:\/\//i, 'https://')
}

/**
 * The page's scheme, defaulting to http.
 *
 * A real browser always has location.protocol; this defends the non-browser
 * cases (tests, SSR-ish contexts) where it can be absent. Defaulting to http
 * is the safe direction — it preserves today's behaviour rather than inventing
 * an https URL for a port that may not serve TLS.
 */
function pageScheme(): string {
  const p = window.location?.protocol
  return p === 'https:' || p === 'http:' ? p : 'http:'
}

/** Apps served over HTTPS (self-signed) rather than plain HTTP, regardless of
 *  the page's scheme. */
export const HTTPS_APP_IDS = new Set(['netbird'])

/** App ID -> direct launch port for the paths that bypass the in-app session:
 *  new-tab apps and the companion's native WebView. Every port here is owned
 *  by the app gate (manifest `auth: gated`/`open` + `bind: 127.0.0.1`), which
 *  serves TLS on the same port whenever the node has a certificate. */
export const DIRECT_APP_PORTS: Record<string, string> = {
  'btcpay-server': '23000',
  grafana: '3000',
  photoprism: '2342',
  homeassistant: '8123',
  vaultwarden: '8082',
  nextcloud: '8085',
  portainer: '9000',
  tailscale: '8240',
  'nginx-proxy-manager': '8081',
  'uptime-kuma': '3002',
  gitea: '3001',
  // Without this, directAppUrl('netbird') returns null and netbird falls
  // through to the iframe (and never gets its https URL) — issue #15.
  netbird: '8087',
}

/** Direct-port launch URL for an app, on the page's scheme.
 *
 *  These are the apps that open OUTSIDE the dashboard's own origin — a new
 *  browser tab on the desktop, or the companion's in-app WebView on a phone.
 *  The URL is handed to a context with no dashboard chrome, so it must carry
 *  the scheme the remote browser actually reached the node on: on an HTTPS
 *  connection, `http://host:port` is at best a silent downgrade to cleartext
 *  and at worst blocked outright as mixed content. Every port in
 *  DIRECT_APP_PORTS is served by the app gate with TLS on the same port
 *  (see appgate/tls.rs), so following the page scheme is always answerable.
 *  Plain-HTTP dashboards keep today's behaviour exactly. */
export function directAppUrl(appId: string): string | null {
  const port = DIRECT_APP_PORTS[appId]
  if (!port || typeof window === 'undefined') return null
  const scheme = HTTPS_APP_IDS.has(appId)
    || (portIsGateFronted(appId, port) && pageScheme() === 'https:')
    ? 'https'
    : 'http'
  return `${scheme}://${window.location.hostname}:${port}`
}

/** Resolve a human-readable title for an app */
export function resolveAppTitle(id: string): string {
  return APP_TITLES[id] || id.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}
