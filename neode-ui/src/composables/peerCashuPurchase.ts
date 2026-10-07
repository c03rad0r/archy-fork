/** Supplemental UI recovery marker; immutable terms and settlement live on the node. */
export type CashuQuote = { network: 'mainnet' | 'testnet'; mint_url: string; state: 'confirmation_required'; operation_id: string; envelope_sha256: string; gross_token_sats: number; seller_net_sats: number; wallet_debit_sats: number; expires_at: number }
export type CashuAttempt = { version: 1; peer: string; contentId: string; quote: CashuQuote; dispatched: boolean }
export function validCashuIdentity(value: { network?: unknown; mint_url?: unknown }): boolean {
  if (value.network !== 'mainnet' && value.network !== 'testnet') return false
  if (typeof value.mint_url !== 'string' || value.mint_url.length > 2048) return false
  try { const url = new URL(value.mint_url); return ['http:', 'https:'].includes(url.protocol) && Boolean(url.hostname) && !url.username && !url.password && !url.hash } catch { return false }
}
export function parseCashuQuote(value: unknown): CashuQuote {
  const v = value as Partial<CashuQuote> | null
  if (!v || !validCashuIdentity(v) || v.state !== 'confirmation_required' || !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(v.operation_id || '')
    || !/^[0-9a-f]{64}$/.test(v.envelope_sha256 || '')
    || ![v.gross_token_sats, v.seller_net_sats, v.wallet_debit_sats, v.expires_at].every(n => Number.isSafeInteger(n) && Number(n) > 0)
    || v.wallet_debit_sats! < v.gross_token_sats! || v.gross_token_sats! < v.seller_net_sats!) throw new Error('The node returned an invalid payment quote. No new payment was confirmed.')
  return v as CashuQuote
}
export function cashuAttemptKey(peer: string, id: string) { return `peer-file-cashu:${encodeURIComponent(peer)}:${encodeURIComponent(id)}` }
export function readCashuAttempt(peer: string, id: string): CashuAttempt | null {
  const raw = localStorage.getItem(cashuAttemptKey(peer, id)); if (!raw) return null
  const v = JSON.parse(raw) as CashuAttempt
  if (v.version !== 1 || v.peer !== peer || v.contentId !== id || typeof v.dispatched !== 'boolean') throw new Error('Saved Cashu purchase needs recovery; do not pay again.')
  parseCashuQuote(v.quote); return v
}
export function keepCashuAttempt(peer: string, id: string, quote: CashuQuote, dispatched: boolean) {
  parseCashuQuote(quote)
  const old = readCashuAttempt(peer, id)
  if (old && (old.quote.operation_id !== quote.operation_id || old.quote.envelope_sha256 !== quote.envelope_sha256 || old.quote.wallet_debit_sats !== quote.wallet_debit_sats || old.quote.network !== quote.network || old.quote.mint_url !== quote.mint_url)) throw new Error('Recover or cancel the original purchase before replacing it.')
  localStorage.setItem(cashuAttemptKey(peer, id), JSON.stringify({ version: 1, peer, contentId: id, quote, dispatched }))
}
export function clearCashuAttempt(peer: string, id: string) { localStorage.removeItem(cashuAttemptKey(peer, id)) }

/** Keep one bounded private browser copy before replacing an unreadable marker.
 * The caller invokes this only after a valid node recovery response, never on
 * network failure or to authorize fresh spending. */
export function archiveMalformedCashuAttempt(peer: string, id: string) {
  try { readCashuAttempt(peer, id); return } catch { /* preserve before replacement */ }
  const key = cashuAttemptKey(peer, id)
  const raw = localStorage.getItem(key)
  if (raw === null) return
  if (new TextEncoder().encode(raw).byteLength > 65536) throw new Error('The saved recovery marker is too large to archive safely. It remains intact; no new payment was confirmed.')
  const archiveKey = `${key}:unreadable`
  const previous = localStorage.getItem(archiveKey)
  if (previous !== null && previous !== raw) throw new Error('An earlier recovery marker is already archived. Original records remain intact; no new payment was confirmed.')
  localStorage.setItem(archiveKey, raw)
  localStorage.removeItem(key)
}
export function keepAuthoritativeCashuQuote(peer: string, id: string, quote: CashuQuote) {
  parseCashuQuote(quote)
  archiveMalformedCashuAttempt(peer, id)
  keepCashuAttempt(peer, id, quote, false)
}
