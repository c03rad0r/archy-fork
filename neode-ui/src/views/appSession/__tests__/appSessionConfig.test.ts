import { describe, expect, it, beforeEach } from 'vitest'
import { DEFAULT_GITWORKSHOP_REPO_PATH, HOST_FRAME_APPS, NEW_TAB_APPS, directAppUrl, resolveAppUrl } from '../appSessionConfig'
import { GENERATED_HOST_FRAME_APPS, GENERATED_NEW_TAB_APPS } from '../generatedAppSessionConfig'
import { __setSignedCatalogForTests } from '../../discover/curatedApps'

// Mirror of the live signed catalog's embedded manifests (the ports[] auth
// that decides TLS eligibility). Kept minimal — only what the scheme logic
// consults.
const SIGNED = {
  apps: {
    vaultwarden: { version: '1.37.1', manifest: { app: { ports: [{ host: 8082, auth: 'gated' }] } } },
    gitea: { version: '1.23', manifest: { app: { ports: [{ host: 3001, auth: 'open' }, { host: 2222, auth: 'none' }] } } },
    'btcpay-server': { version: '2.4.3', manifest: { app: { ports: [{ host: 23000, auth: 'open' }] } } },
    mempool: { version: '3.3.1', manifest: { app: { ports: [{ host: 4080, auth: 'gated' }] } } },
    filebrowser: { version: '2.27.0', manifest: { app: { ports: [{ host: 8083, auth: 'gated' }] } } },
    // Legacy curated installs — in the community list, NOT in the signed
    // catalog's manifests. Their ports publish plain HTTP: https fails.
    'nginx-proxy-manager': { version: 'latest' },
    tailscale: { version: 'stable' },
    // auth:none ports are container-published too — https would fail.
    cuprate: { version: '0.1.0-preview', manifest: { app: { ports: [{ host: 18090, auth: 'none' }] } } },
  },
}

function stubLocation(value: { hostname: string; protocol: string }) {
  Object.defineProperty(window, 'location', {
    value,
    writable: true,
    configurable: true,
  })
}

describe('appSessionConfig', () => {
  beforeEach(() => {
    __setSignedCatalogForTests(SIGNED as never)
  })
  it('keeps manifest-owned new-tab apps marked on every viewport', () => {
    expect(NEW_TAB_APPS.has('btcpay-server')).toBe(true)
    expect(NEW_TAB_APPS.has('photoprism')).toBe(true)
    expect(GENERATED_NEW_TAB_APPS.has('photoprism')).toBe(true)
  })

  it('keeps frontend-only new-tab overrides for apps without generated metadata', () => {
    expect(NEW_TAB_APPS.has('tailscale')).toBe(true)
    expect(GENERATED_NEW_TAB_APPS.has('tailscale')).toBe(false)
  })

  it('does not force GitWorkshop into a dashboard iframe in Companion', () => {
    expect(GENERATED_HOST_FRAME_APPS.has('archipelago-source')).toBe(false)
    expect(HOST_FRAME_APPS.has('archipelago-source')).toBe(false)
  })

  it('resolves direct app ports against the current browser host', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    expect(resolveAppUrl('mempool')).toBe('http://192.0.2.10:4080')
    expect(resolveAppUrl('indeedhub')).toBe('http://192.0.2.10:7778')
    expect(resolveAppUrl('botfights')).toBe('http://192.0.2.10:9100')
  })

  it('uses manifest-generated launch ports for apps outside the manual override list', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    // searxng's manifest publishes host port 8888 (apps/searxng/
    // manifest.yml) — assert against the manifest-generated value, which is
    // exactly what this test exists to protect.
    expect(resolveAppUrl('searxng')).toBe('http://192.0.2.10:8888')
  })

  it('does not treat service-only tcp ports as web launch surfaces', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    expect(resolveAppUrl('meshtastic')).toBe('')
  })

  it('keeps NetBird on the unified dashboard proxy port', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    // NetBird's dashboard needs a secure context (OIDC PKCE), so it is
    // ALWAYS launched over https — on either page scheme.
    expect(resolveAppUrl('netbird', undefined, 'http://localhost:8086')).toBe('https://192.0.2.10:8087')
  })

  it('uses backend runtime URLs for apps with dynamic launch surfaces', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    expect(resolveAppUrl('filebrowser', undefined, 'http://localhost:8083')).toBe('http://192.0.2.10:8083')
  })

  // The direct-port launch path (new-tab apps on desktop, the companion's
  // native WebView on phones) used to hardcode http:// — so a node reached
  // over HTTPS opened Vaultwarden and friends in cleartext. It must follow
  // the page scheme ONLY for ports the app gate fronts (TLS on the same
  // port); legacy installs without manifests (Nginx Proxy Manager, Tailscale)
  // and auth:none ports stay on http or https would fail to connect.
  it('builds direct app URLs on the page scheme — https page, gate-fronted app', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'https:' })

    expect(directAppUrl('vaultwarden')).toBe('https://192.0.2.10:8082')
    expect(directAppUrl('gitea')).toBe('https://192.0.2.10:3001')
    expect(directAppUrl('btcpay-server')).toBe('https://192.0.2.10:23000')
  })

  it('keeps legacy manifest-less apps on http even on an https page', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'https:' })

    expect(directAppUrl('nginx-proxy-manager')).toBe('http://192.0.2.10:8081')
    expect(directAppUrl('tailscale')).toBe('http://192.0.2.10:8240')
  })

  it('keeps plain-http direct app URLs on a plain-http page', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'http:' })

    expect(directAppUrl('vaultwarden')).toBe('http://192.0.2.10:8082')
    expect(directAppUrl('nginx-proxy-manager')).toBe('http://192.0.2.10:8081')
  })

  it('always launches secure-context apps over https, on either page scheme', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'http:' })

    expect(directAppUrl('netbird')).toBe('https://192.0.2.10:8087')
  })

  it('resolves session app URLs on the page scheme for gate-fronted ports only (https page)', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'https:' })

    expect(resolveAppUrl('mempool')).toBe('https://192.0.2.10:4080')
    expect(resolveAppUrl('filebrowser', undefined, 'http://localhost:8083')).toBe('https://192.0.2.10:8083')
    // A runtime port the gate does NOT front keeps plain http (https would
    // fail to connect outright).
    expect(resolveAppUrl('filebrowser', undefined, 'http://localhost:18083')).toBe('http://192.0.2.10:18083')
    // Cuprate's raw RPC is never a launch surface; use the companion UI.
    expect(resolveAppUrl('cuprate', undefined, 'http://localhost:18090')).toBe('/app/cuprate-ui/')
  })

  it('keeps the pre-catalog Source app on the dashboard origin', () => {
    stubLocation({ hostname: '192.0.2.10', protocol: 'https:' })

    // Source is intentionally absent from SIGNED until owner UAT passes. It
    // must follow the already-working dashboard ingress instead of assuming
    // that the same address also exposes a dedicated high port.
    expect(resolveAppUrl('archipelago-source')).toBe(`/app/archipelago-source${DEFAULT_GITWORKSHOP_REPO_PATH}`)
    expect(resolveAppUrl('archipelago-source', undefined, 'http://localhost:8337'))
      .toBe(`/app/archipelago-source${DEFAULT_GITWORKSHOP_REPO_PATH}`)
    expect(resolveAppUrl('archipelago-source', '/search'))
      .toBe('/app/archipelago-source/search')
  })
})
