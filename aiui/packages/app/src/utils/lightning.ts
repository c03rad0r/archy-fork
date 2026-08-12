/**
 * Lightning Network utilities for deep-linking to external wallets.
 * AIUI is never a wallet — only generates URIs to open external wallets.
 */

/** Parse a BOLT11 invoice to extract amount (if present) and expiry */
export function parseBolt11(invoice: string): {
  amount: number | null
  expiry: number | null
  timestamp: number | null
} {
  const lower = invoice.toLowerCase()
  if (!lower.startsWith('lnbc') && !lower.startsWith('lntb') && !lower.startsWith('lnbcrt')) {
    return { amount: null, expiry: null, timestamp: null }
  }

  // Extract amount from human-readable part (after ln prefix, before '1' separator)
  const hrpMatch = lower.match(/^ln(?:bc|tb|bcrt)(\d+)([munp]?)/)
  let amount: number | null = null
  if (hrpMatch && hrpMatch[1]) {
    const num = parseInt(hrpMatch[1], 10)
    const multiplier = hrpMatch[2]
    // Convert to millisats then to sats
    const btcMultipliers: Record<string, number> = {
      '': 1e8, // BTC to sats
      m: 1e5,  // milli-BTC to sats
      u: 1e2,  // micro-BTC to sats
      n: 0.1,  // nano-BTC to sats
      p: 0.0001, // pico-BTC to sats
    }
    amount = Math.round(num * (btcMultipliers[multiplier] ?? 1e8))
  }

  return { amount, expiry: null, timestamp: null }
}

/** Create a Lightning: URI for deep-linking to wallet apps */
export function createLightningUri(invoice: string): string {
  return `lightning:${invoice}`
}

/** Create a BIP21 bitcoin: URI with optional Lightning invoice */
export function createBip21Uri(
  address: string,
  options?: { amount?: number; label?: string; lightning?: string },
): string {
  const params = new URLSearchParams()
  if (options?.amount) params.set('amount', (options.amount / 1e8).toFixed(8))
  if (options?.label) params.set('label', options.label)
  if (options?.lightning) params.set('lightning', options.lightning)
  const qs = params.toString()
  return `bitcoin:${address}${qs ? '?' + qs : ''}`
}

/** Create an LNURL-pay link */
export function createLnurlPayUri(lnurl: string): string {
  return `lightning:${lnurl}`
}

/** Format satoshi amount for display */
export function formatSats(sats: number): string {
  if (sats >= 1_000_000) {
    return `${(sats / 1_000_000).toFixed(2)}M sats`
  }
  if (sats >= 1_000) {
    return `${(sats / 1_000).toFixed(sats >= 10_000 ? 0 : 1)}k sats`
  }
  return `${sats} sats`
}

/** Check if a string looks like a BOLT11 invoice */
export function isBolt11(text: string): boolean {
  const lower = text.toLowerCase().trim()
  return lower.startsWith('lnbc') || lower.startsWith('lntb') || lower.startsWith('lnbcrt')
}

/** Check if a string looks like an LNURL */
export function isLnurl(text: string): boolean {
  const lower = text.toLowerCase().trim()
  return lower.startsWith('lnurl')
}
