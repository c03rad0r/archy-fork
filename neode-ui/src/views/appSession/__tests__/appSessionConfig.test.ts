import { describe, expect, it } from 'vitest'
import { NEW_TAB_APPS, resolveAppUrl } from '../appSessionConfig'
import { GENERATED_NEW_TAB_APPS } from '../generatedAppSessionConfig'

describe('appSessionConfig', () => {
  it('keeps manifest-owned new-tab apps marked on every viewport', () => {
    expect(NEW_TAB_APPS.has('btcpay-server')).toBe(true)
    expect(NEW_TAB_APPS.has('photoprism')).toBe(true)
    expect(GENERATED_NEW_TAB_APPS.has('photoprism')).toBe(true)
  })

  it('keeps frontend-only new-tab overrides for apps without generated metadata', () => {
    expect(NEW_TAB_APPS.has('tailscale')).toBe(true)
    expect(GENERATED_NEW_TAB_APPS.has('tailscale')).toBe(false)
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

    // did-wallet's manifest publishes host port 8088 (apps/did-wallet/
    // manifest.yml) — assert against the manifest-generated value, which is
    // exactly what this test exists to protect.
    expect(resolveAppUrl('did-wallet')).toBe('http://192.0.2.10:8088')
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

    expect(resolveAppUrl('netbird', undefined, 'http://localhost:8086')).toBe('http://192.0.2.10:8087')
  })

  it('uses backend runtime URLs for apps with dynamic launch surfaces', () => {
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })

    expect(resolveAppUrl('filebrowser', undefined, 'http://localhost:18083')).toBe('http://192.0.2.10:18083')
  })
})
