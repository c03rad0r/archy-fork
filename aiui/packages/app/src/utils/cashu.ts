/**
 * Cashu ecash token parsing and display utilities.
 * AIUI is never a wallet — only parses, displays, and deep-links to external wallets.
 */

interface CashuProof {
  amount: number
  id: string
  secret: string
  C: string
}

interface CashuTokenEntry {
  mint: string
  proofs: CashuProof[]
}

interface CashuTokenData {
  token: CashuTokenEntry[]
  unit?: string
  memo?: string
}

export interface ParsedCashuToken {
  mint: string
  amount: number
  unit: string
  memo?: string
  raw: string
}

/** Check if a string looks like a Cashu token */
export function isCashuToken(text: string): boolean {
  return text.trim().startsWith('cashuA')
}

/** Extract Cashu tokens from a text string */
export function extractCashuTokens(text: string): string[] {
  const regex = /cashuA[A-Za-z0-9_-]+/g
  return [...text.matchAll(regex)].map(m => m[0])
}

/** Parse a Cashu token string (cashuA...) into structured data */
export function parseCashuToken(token: string): ParsedCashuToken | null {
  if (!isCashuToken(token)) return null

  try {
    // Remove 'cashuA' prefix and decode base64url
    const encoded = token.slice(6)
    const padded = encoded.replace(/-/g, '+').replace(/_/g, '/')
    const json = atob(padded)
    const data = JSON.parse(json) as CashuTokenData

    if (!data.token || !Array.isArray(data.token) || data.token.length === 0) {
      return null
    }

    const entry = data.token[0]
    const totalAmount = entry.proofs.reduce((sum, p) => sum + p.amount, 0)

    return {
      mint: entry.mint,
      amount: totalAmount,
      unit: data.unit ?? 'sat',
      memo: data.memo,
      raw: token,
    }
  } catch {
    return null
  }
}

/** Format a mint URL for display (truncate) */
export function formatMintUrl(url: string): string {
  try {
    const parsed = new URL(url)
    const host = parsed.hostname
    if (host.length > 30) {
      return host.slice(0, 15) + '...' + host.slice(-12)
    }
    return host
  } catch {
    if (url.length > 30) {
      return url.slice(0, 15) + '...' + url.slice(-12)
    }
    return url
  }
}

/** Format amount with unit */
export function formatCashuAmount(amount: number, unit: string): string {
  if (unit === 'sat' || unit === 'sats') {
    if (amount >= 1_000_000) return `${(amount / 1_000_000).toFixed(2)}M sats`
    if (amount >= 1_000) return `${(amount / 1_000).toFixed(amount >= 10_000 ? 0 : 1)}k sats`
    return `${amount} sats`
  }
  return `${amount} ${unit}`
}
