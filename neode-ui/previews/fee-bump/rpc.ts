// Standalone preview only. No network calls and no wallet access.
export let method = 'cpfp'
let submitted = false
let sweepFee = 618
export function choose(value: string) { method = value; submitted = false }
export const rpcClient = { async call(request: { method: string; params?: Record<string, unknown> }) {
  if (request.method === 'lnd.bump-status') return submitted
    ? { status: 'mempool', message: 'Preview: fee bump accepted. No real transaction was sent.', bump_txid: 'b'.repeat(64), actual_sweep_fee_sats: sweepFee, quote: { method } }
    : { status: 'none' }
  if (request.method === 'lnd.bump-submit') { submitted = true; return { status: 'registered', message: 'Preview: bump registered. No real transaction was sent.' } }
  if (request.method !== 'lnd.bump-quote') throw new Error('Wallet access is disabled in this preview')
  const rate = Number(request.params?.sat_per_vbyte || 3)
  const budget = Math.max(rate * 254 - 144, method === 'rbf' ? 763 : 112)
  sweepFee = budget
  return { quote_id: 'preview', txid: request.params?.txid, expires_at: Math.floor(Date.now()/1000)+60,
    method, recipient_sats: 161650, current_fee_sats: method === 'rbf' ? 794 : 144,
    additional_fee_sats: budget - (method === 'rbf' ? 650 : 0), total_fee_sats: 144 + budget,
    budget_sats: budget, rate_sat_vb: rate }
} }
