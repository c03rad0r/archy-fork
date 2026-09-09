import { beforeEach, describe, expect, it } from 'vitest'
import { consentKey, hasRememberedConsent, rememberConsent } from '../nostrConsent'

describe('NIP-07 consent storage', () => {
  beforeEach(() => localStorage.clear())

  it('binds remembered approval to origin, app, identity and method', () => {
    const key = consentKey('https://node.example', 'archipelago-source', 'identity-a', 'signEvent')
    rememberConsent(key)

    expect(hasRememberedConsent(key)).toBe(true)
    expect(hasRememberedConsent(consentKey('https://node.example', 'archipelago-source', 'identity-b', 'signEvent'))).toBe(false)
    expect(hasRememberedConsent(consentKey('https://node.example', 'archipelago-source', 'identity-a', 'nip44.decrypt'))).toBe(false)
    expect(hasRememberedConsent(consentKey('https://other-node.example', 'archipelago-source', 'identity-a', 'signEvent'))).toBe(false)
  })
})
