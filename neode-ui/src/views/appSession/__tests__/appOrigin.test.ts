import { afterEach, describe, expect, it, vi } from 'vitest'
import { appOrigin, matchPageScheme } from '../appSessionConfig'

// An HTTPS dashboard cannot embed an HTTP app frame — browsers block it as
// mixed content — so the app origin has to follow the page's scheme. Plain-HTTP
// nodes must be completely unaffected, which is what most of these pin.

function setLocation(protocol: string, hostname: string) {
  Object.defineProperty(window, 'location', {
    value: { protocol, hostname },
    writable: true,
    configurable: true,
  })
}

afterEach(() => vi.unstubAllGlobals())

describe('appOrigin', () => {
  it('stays on http for an http dashboard', () => {
    setLocation('http:', 'test-node.local')
    expect(appOrigin(8334)).toBe('http://test-node.local:8334')
  })

  it('follows an https dashboard onto the app port', () => {
    setLocation('https:', 'test-node.local')
    expect(appOrigin(8334)).toBe('https://test-node.local:8334')
  })

  it('keeps the hostname the user actually typed, not a fixed name', () => {
    setLocation('https:', '100.64.0.5')
    expect(appOrigin(3000)).toBe('https://100.64.0.5:3000')
  })
})

describe('matchPageScheme', () => {
  it('leaves backend-reported http URLs alone on an http page', () => {
    setLocation('http:', 'node')
    expect(matchPageScheme('http://node:8080/app')).toBe('http://node:8080/app')
  })

  it('upgrades a backend-reported http URL on an https page', () => {
    setLocation('https:', 'node')
    expect(matchPageScheme('http://node:8080/app')).toBe('https://node:8080/app')
  })

  it('does not touch anything but the scheme', () => {
    setLocation('https:', 'node')
    expect(matchPageScheme('http://node:8080/a/b?c=1#d')).toBe('https://node:8080/a/b?c=1#d')
  })

  it('leaves an already-https URL untouched', () => {
    setLocation('https:', 'node')
    expect(matchPageScheme('https://node:8080/app')).toBe('https://node:8080/app')
  })
})
