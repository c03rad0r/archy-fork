/** Durable owner-node state; never infer an unpaid address from a browser timeout. */
export interface OnchainAttempt {
  operation_id: string
  price_sats: number
  phase: 'address_requested' | 'offer_prepared' | 'plan_prepared' | 'plan_lease_dispatched' | 'inputs_leased' | 'address_allocation_dispatched' | 'quoted' | 'template_prepared' | 'lease_dispatched' | 'funding_dispatched' | 'funded' | 'signing_dispatched' | 'signed' | 'broadcast_dispatched' | 'published'
  network: 'mainnet' | 'testnet' | 'signet' | 'regtest' | null
  external_exposure: boolean
  address: string | null
  fee_sats: number | null
  max_fee_sats: number | null
  template_sha256: string | null
  plan_sha256: string | null
  txid: string | null
  paid: boolean
  change_allocation_ambiguous: boolean
  can_switch_method: boolean
  retired_unallocated: boolean
}
const phases = new Set(['address_requested','offer_prepared','plan_prepared','plan_lease_dispatched','inputs_leased','address_allocation_dispatched','quoted','template_prepared','lease_dispatched','funding_dispatched','funded','signing_dispatched','signed','broadcast_dispatched','published'])
export function parseOnchainAttempt(value: unknown): OnchainAttempt {
  if (!value || typeof value !== 'object') throw Error('Original on-chain state is unavailable; do not pay again')
  const v = { plan_sha256: null, ...value } as Record<string,unknown>
  const integer = (n:unknown) => typeof n === 'number' && Number.isSafeInteger(n) && n >= 0
  const hex = (s:unknown) => typeof s === 'string' && /^[0-9a-f]{64}$/.test(s)
  if (typeof v.operation_id !== 'string' || !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(v.operation_id)
    || !integer(v.price_sats) || Number(v.price_sats)<546 || !phases.has(String(v.phase))
    || ![null,'mainnet','testnet','signet','regtest'].includes(v.network as string|null)
    || typeof v.external_exposure !== 'boolean' || typeof v.paid !== 'boolean'
    || typeof v.change_allocation_ambiguous !== 'boolean' || typeof v.retired_unallocated !== 'boolean' || v.can_switch_method !== v.retired_unallocated
    || !(v.address === null || typeof v.address === 'string' && v.address.length>0)
    || !(v.fee_sats === null || integer(v.fee_sats)) || !(v.max_fee_sats === null || integer(v.max_fee_sats))
    || !(v.plan_sha256 === null || hex(v.plan_sha256)) || !(v.template_sha256 === null || hex(v.template_sha256)) || !(v.txid === null || hex(v.txid))) throw Error('Original on-chain state is invalid; do not pay again')
  if (v.external_exposure && !v.address || !v.external_exposure && v.address !== null
    || (v.template_sha256 !== null || v.plan_sha256 !== null) && (v.fee_sats === null || v.max_fee_sats === null || Number(v.fee_sats)>Number(v.max_fee_sats))) throw Error('Original on-chain payment terms changed')
  if (['plan_prepared','plan_lease_dispatched','inputs_leased'].includes(String(v.phase)) && (v.plan_sha256 === null || v.network === null || v.external_exposure || v.address !== null)) throw Error('Original funding plan is incomplete')
  if (v.retired_unallocated && (!['address_requested','offer_prepared','plan_prepared'].includes(String(v.phase)) || v.external_exposure || v.paid || v.address !== null || v.template_sha256 !== null || v.txid !== null || v.change_allocation_ambiguous)) throw Error('Retired on-chain operation contains payment liability')
  return v as unknown as OnchainAttempt
}
