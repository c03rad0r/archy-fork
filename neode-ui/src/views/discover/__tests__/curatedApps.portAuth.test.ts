import { afterEach, describe, expect, it } from 'vitest'
import { __setSignedCatalogForTests, portAuth, portIsGateFronted, type SignedAppCatalog } from '../curatedApps'

/** Catalog fragments mirroring the live signed catalog's port declarations
 *  (releases/app-catalog.json, 2026-09-01). */
const catalog = (apps: SignedAppCatalog['apps']): SignedAppCatalog => ({ apps })

const FULL = catalog({
  'archy-mempool-web': {
    version: '3.0.1',
    manifest: { app: { id: 'archy-mempool-web', ports: [{ host: 4080, container: 8080, auth: 'gated' }] } },
  },
  'mempool': {
    version: '3.0.0',
    manifest: { app: { id: 'mempool', ports: [{ host: 4080, container: 8080, auth: 'gated' }] } },
  },
  'lnd-ui': {
    version: '1.0.0',
    manifest: { app: { id: 'lnd-ui', ports: [{ host: 18083, container: 18083, auth: 'gated' }] } },
  },
  'bitcoin-ui': {
    version: '1.0.0',
    manifest: { app: { id: 'bitcoin-ui', ports: [{ host: 8334, container: 8334, auth: 'gated' }] } },
  },
  'bitcoin-knots': {
    version: '29.3',
    manifest: { app: { id: 'bitcoin-knots', ports: [{ host: 8332, container: 8332, auth: 'none' }] } },
  },
  'electrs-ui': {
    version: '1.0.0',
    manifest: { app: { id: 'electrs-ui', ports: [{ host: 50002, container: 50002, auth: 'gated' }] } },
  },
})

afterEach(() => __setSignedCatalogForTests(null))

describe('portAuth', () => {
  it('resolves the UI port through the launch alias, not just the app id', () => {
    __setSignedCatalogForTests(FULL)
    // 'mempool-web' has no catalog entry of its own; archy-mempool-web owns 4080.
    expect(portIsGateFronted('mempool-web', 4080)).toBe(true)
    // 'bitcoin-knots' declares 8332 (auth none) but its UI port 8334 is owned
    // by bitcoin-ui — the alias must find it, or the new-tab button hands
    // out an http:// URL on an HTTPS dashboard (2026-09-01 report).
    expect(portIsGateFronted('bitcoin-knots', 8334)).toBe(true)
    expect(portIsGateFronted('lnd', 18083)).toBe(true)
    expect(portIsGateFronted('electrs', 50002)).toBe(true)
  })

  it('keeps a port the app itself publishes as plain HTTP off the gate', () => {
    __setSignedCatalogForTests(FULL)
    expect(portAuth('bitcoin-knots', 8332)).toBe('none')
    expect(portIsGateFronted('bitcoin-knots', 8332)).toBe(false)
  })

  it('answers null for unknown apps and ports (never assume TLS)', () => {
    __setSignedCatalogForTests(FULL)
    expect(portAuth('never-installed-app', 1234)).toBeNull()
    expect(portIsGateFronted('bitcoin-ui', 9999)).toBe(false)
  })

  it('falls back to a unanimous port-wide scan for unknown ids', () => {
    __setSignedCatalogForTests(FULL)
    // No alias for this id, but every declarer of 4080 says gated.
    expect(portIsGateFronted('some-future-alias', 4080)).toBe(true)
  })

  it('refuses the port-wide scan when declarers disagree (no TLS guess)', () => {
    __setSignedCatalogForTests(catalog({
      'app-a': { version: '1', manifest: { app: { ports: [{ host: 7000, auth: 'gated' }] } } },
      'app-b': { version: '1', manifest: { app: { ports: [{ host: 7000, auth: 'none' }] } } },
    }))
    expect(portAuth('unknown-app', 7000)).toBeNull()
  })

  it('returns null without a warmed catalog (cache miss, not a guess)', () => {
    expect(portAuth('mempool-web', 4080)).toBeNull()
  })
})
