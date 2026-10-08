/**
 * Public-demo helpers.
 *
 * The demo build (VITE_DEMO=1) replays the typing splash + intro/onboarding on
 * EVERY fresh boot of the app at '/' (first visit or browser refresh) — see
 * App.vue and RootRedirect.demoRoute. Also exposes the shared demo credentials
 * shown on the login screen.
 */

export const IS_DEMO =
  import.meta.env.VITE_DEMO === '1' || import.meta.env.VITE_DEMO === 'true'

/** Memorable shared password for the public demo (must match the mock backend). */
export const DEMO_PASSWORD = 'entertoexit'

/** Forget any legacy per-day gate marker (pre-2026-07 builds stored one). */
export function clearDemoIntroSeen(): void {
  try {
    localStorage.removeItem('demo_intro_date')
  } catch {
    /* ignore */
  }
}

// ── Demoable apps ───────────────────────────────────────────────────────────
// Only these apps actually do something in the demo (a mock UI or a real
// proxied site). Everything else shows "No demo" on a disabled install button
// and is not launchable.
// IndeeHub's real site sends X-Frame-Options: SAMEORIGIN and its SPA uses
// absolute-root asset paths, so a /app/indeedhub/ path-prefix proxy broke it.
// Instead nginx-demo.conf runs a WHOLE-ORIGIN reverse proxy of
// https://indee.tx1138.com on a dedicated port (framing headers stripped,
// demo sign-in seeded) and the iframe loads http://<demo-host>:<port>/.
const DEMO_PROXY_PORTS: Record<string, number> = {
  indeedhub: 2101,
}

// Apps loaded in the in-app iframe via a same-origin path served by the mock
// backend (per-app mock UIs and placeholder dashboards).
const DEMO_MOCK_UI: Record<string, string> = {
  justworks: '/app/justworks/',
  mempool: '/app/mempool/',
  'mempool-web': '/app/mempool/',
  'bitcoin-knots': '/app/bitcoin-knots/',
  'bitcoin-core': '/app/bitcoin-core/',
  bitcoin: '/app/bitcoin-core/',
  'bitcoin-ui': '/app/bitcoin-ui/',
  electrs: '/app/electrumx/',
  electrumx: '/app/electrumx/',
  'archy-electrs-ui': '/app/electrumx/',
  lnd: '/app/lnd/',
  'lnd-ui': '/app/lnd/',
  'archy-lnd-ui': '/app/lnd/',
  thunderhub: '/app/lnd/',
  fedimint: '/app/fedimint/',
  fedimintd: '/app/fedimint/',
  filebrowser: '/app/filebrowser/',
  // Static placeholder dashboards served by the mock backend (DEMO_APP_PAGES).
  'btcpay-server': '/app/btcpay-server/',
  grafana: '/app/grafana/',
  nextcloud: '/app/nextcloud/',
  jellyfin: '/app/jellyfin/',
  vaultwarden: '/app/vaultwarden/',
  'nostr-rs-relay': '/app/nostr-rs-relay/',
  searxng: '/app/searxng/',
  'uptime-kuma': '/app/uptime-kuma/',
}

/**
 * Whether a demo app opens externally (new tab / in-app browser) because its
 * real site blocks iframing. Nothing does anymore — frame-busting sites are
 * whole-origin proxied (DEMO_PROXY_PORTS) instead. Kept exported so call
 * sites in appLauncher.ts / AppSession.vue compile unchanged.
 */
export function isDemoExternal(_appId: string): boolean {
  return false
}

/** Can this app be launched/installed in the demo? */
export function isDemoApp(appId: string): boolean {
  return appId in DEMO_PROXY_PORTS || appId in DEMO_MOCK_UI
}

/** Resolve the demo launch URL for an app, or null if it isn't demoable. */
export function demoAppUrl(appId: string): string | null {
  const proxyPort = DEMO_PROXY_PORTS[appId]
  if (proxyPort !== undefined && typeof window !== 'undefined') {
    // Same host the demo itself is served from — never a hardcoded host/IP.
    return `${window.location.protocol}//${window.location.hostname}:${proxyPort}/`
  }
  return DEMO_MOCK_UI[appId] ?? null
}
