const CONSENT_KEY = 'archipelago_nostr_consent_v2'

function readRemembered(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(CONSENT_KEY) || '[]')
    return new Set(Array.isArray(parsed) ? parsed.filter(item => typeof item === 'string') : [])
  } catch {
    return new Set()
  }
}

/** Remembered NIP-07 access is scoped to the exact app, identity and method. */
export function consentKey(
  origin: string,
  appId: string,
  identityId: string,
  method: string,
): string {
  return JSON.stringify(['v2', origin, appId, identityId, method])
}

export function hasRememberedConsent(key: string): boolean {
  return readRemembered().has(key)
}

export function rememberConsent(key: string): void {
  const remembered = readRemembered()
  remembered.add(key)
  try { localStorage.setItem(CONSENT_KEY, JSON.stringify([...remembered])) } catch { /* unavailable/full */ }
}
