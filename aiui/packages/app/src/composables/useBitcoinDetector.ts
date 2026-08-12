export interface DetectedBitcoinAddress {
  address: string
  type: 'bech32' | 'bech32m' | 'p2pkh' | 'p2sh' | 'testnet'
}

export interface DetectedLightningInvoice {
  invoice: string
  amount?: number
  description?: string
}

export interface DetectedBolt12Offer {
  offer: string
}

export interface DetectedTxId {
  txid: string
}

// Bitcoin address patterns
const BECH32_REGEX = /\b(bc1q[a-z0-9]{38,62})\b/gi
const BECH32M_REGEX = /\b(bc1p[a-z0-9]{38,62})\b/gi
const P2PKH_REGEX = /\b(1[13456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]{25,34})\b/g
const P2SH_REGEX = /\b(3[13456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz]{25,34})\b/g
const TESTNET_REGEX = /\b(tb1[a-z0-9]{38,62})\b/gi

// Lightning patterns
const BOLT11_REGEX = /\b(lnbc[a-z0-9]+)\b/gi
const BOLT12_REGEX = /\b(lno1[a-z0-9]+)\b/gi

// Transaction ID (64 hex chars)
const TXID_REGEX = /\b([a-f0-9]{64})\b/gi

export function detectBitcoinAddresses(text: string): DetectedBitcoinAddress[] {
  const addresses: DetectedBitcoinAddress[] = []
  const seen = new Set<string>()

  for (const match of text.matchAll(BECH32_REGEX)) {
    if (!seen.has(match[1].toLowerCase())) {
      seen.add(match[1].toLowerCase())
      addresses.push({ address: match[1], type: 'bech32' })
    }
  }
  for (const match of text.matchAll(BECH32M_REGEX)) {
    if (!seen.has(match[1].toLowerCase())) {
      seen.add(match[1].toLowerCase())
      addresses.push({ address: match[1], type: 'bech32m' })
    }
  }
  for (const match of text.matchAll(P2PKH_REGEX)) {
    if (!seen.has(match[1])) {
      seen.add(match[1])
      addresses.push({ address: match[1], type: 'p2pkh' })
    }
  }
  for (const match of text.matchAll(P2SH_REGEX)) {
    if (!seen.has(match[1])) {
      seen.add(match[1])
      addresses.push({ address: match[1], type: 'p2sh' })
    }
  }
  for (const match of text.matchAll(TESTNET_REGEX)) {
    if (!seen.has(match[1].toLowerCase())) {
      seen.add(match[1].toLowerCase())
      addresses.push({ address: match[1], type: 'testnet' })
    }
  }

  return addresses
}

export function detectBolt11Invoices(text: string): DetectedLightningInvoice[] {
  const invoices: DetectedLightningInvoice[] = []
  const seen = new Set<string>()

  for (const match of text.matchAll(BOLT11_REGEX)) {
    const lower = match[1].toLowerCase()
    if (!seen.has(lower)) {
      seen.add(lower)
      invoices.push({ invoice: match[1] })
    }
  }

  return invoices
}

export function detectBolt12Offers(text: string): DetectedBolt12Offer[] {
  const offers: DetectedBolt12Offer[] = []
  const seen = new Set<string>()

  for (const match of text.matchAll(BOLT12_REGEX)) {
    const lower = match[1].toLowerCase()
    if (!seen.has(lower)) {
      seen.add(lower)
      offers.push({ offer: match[1] })
    }
  }

  return offers
}

export function detectTxIds(text: string): DetectedTxId[] {
  const txids: DetectedTxId[] = []
  const seen = new Set<string>()

  for (const match of text.matchAll(TXID_REGEX)) {
    if (!seen.has(match[1])) {
      seen.add(match[1])
      txids.push({ txid: match[1] })
    }
  }

  return txids
}
