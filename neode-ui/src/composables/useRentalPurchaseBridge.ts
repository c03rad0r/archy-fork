import { ref, shallowRef } from 'vue'
import { validCashuIdentity } from './peerCashuPurchase'
import { rpcClient } from '@/api/rpc-client'
import { installedOriginMatches } from './useMediaRegistrationBridge'
interface FrameContext { appId: () => string; appUrl: () => string; frameWindow: () => Window | null; consentBusy?: () => boolean }
export interface RentalOffer { title: string; terms: { nodeDid: string; contentId: string; sha256: string; priceSats: number; viewingSeconds: number } }
interface Quote { network: 'mainnet' | 'testnet'; mint_url: string; state: string; operation_id: string; envelope_sha256: string; wallet_debit_sats: number; gross_token_sats: number; seller_net_sats: number; expires_at: number; seller_onion: string }
interface Pending { source: Window; origin: string; id: string; offer: RentalOffer; quote?: Quote }
export function supportsRentalPlaybackOrigin(appOrigin: string, dashboardOrigin: string): boolean {
  try {
    const app = new URL(appOrigin), dashboard = new URL(dashboardOrigin)
    // Host-only SameSite=Lax owner cookies support same-host app ports, but
    // not an app on a separate onion/domain or a mixed-scheme frame.
    return ['http:', 'https:'].includes(app.protocol) && app.protocol === dashboard.protocol
      && app.hostname === dashboard.hostname
  } catch { return false }
}
export function useRentalPurchaseBridge(context: FrameContext) {
  const request = shallowRef<RentalOffer | null>(null), quote = shallowRef<Quote | null>(null)
  const phase = ref<'review' | 'loading' | 'confirm' | 'paying'>('review'), error = ref('')
  let pending: Pending | null = null, disposed = false, generation = 0
  const frameOrigin = () => { try { return new URL(context.appUrl(), window.location.origin).origin } catch { return '' } }
  const current = (item: Pending) => !disposed && pending === item && context.appId() === 'indeedhub'
    && context.frameWindow() === item.source && frameOrigin() === item.origin
  function reply(item: Pending, result?: unknown, failure?: string) {
    if (current(item)) item.source.postMessage({ type: 'archipelago-rental-response', id: item.id,
      ...(failure ? { error: failure } : { result }) }, item.origin)
  }
  function cancel() {
    if (pending) reply(pending, undefined, phase.value === 'paying'
      ? 'Payment may be processing. Reopen this title to recover the same purchase.' : 'Rental confirmation closed. No new payment was approved.')
    generation++
    pending = null; request.value = null; quote.value = null; error.value = ''; phase.value = 'review'
  }
  async function installed(item: Pending) {
    const value = await rpcClient.call<{ appId: string; appOrigins: string[] }>({ method: 'media.registration.context', params: {} })
    if (!current(item) || value.appId !== 'indeedhub' || !value.appOrigins.some(origin => installedOriginMatches(item.origin, origin))) throw new Error('The installed app or its frame changed.')
  }
  async function run(confirm: boolean) {
    const item = pending
    if (!item || !current(item) || (confirm ? phase.value !== 'confirm' : phase.value !== 'review')) return
    if (!supportsRentalPlaybackOrigin(item.origin, window.location.origin)) { error.value = 'Open the app from the same node dashboard address before paying.'; return }
    if (context.consentBusy?.()) { error.value = 'Finish the other native confirmation first.'; return }
    phase.value = confirm ? 'paying' : 'loading'; error.value = ''
    try {
      await installed(item)
      if (!current(item)) return
      if (context.consentBusy?.()) throw new Error('Finish the other native confirmation first.')
      const terms = item.offer.terms
      const result = await rpcClient.call<Quote>({ method: 'content.rental-purchase', params: {
        seller_did: terms.nodeDid, content_id: terms.contentId, expected_sha256: terms.sha256,
        expected_price_sats: terms.priceSats, expected_viewing_seconds: terms.viewingSeconds,
        max_wallet_debit: confirm ? item.quote!.wallet_debit_sats : Number.MAX_SAFE_INTEGER,
        ...(confirm ? { consent: { operation_id: item.quote!.operation_id,
          envelope_sha256: item.quote!.envelope_sha256, wallet_debit_sats: item.quote!.wallet_debit_sats } } : {}),
      }, timeout: 120000 })
      if (!current(item)) return
      if (result.state === 'confirmation_required') {
        if (!validCashuIdentity(result) || !Number.isSafeInteger(result.wallet_debit_sats) || result.wallet_debit_sats < terms.priceSats
          || !/^[0-9a-f]{64}$/.test(result.envelope_sha256) || !result.operation_id) throw new Error('Invalid payment confirmation. No payment approved.')
        item.quote = result; quote.value = result; phase.value = 'confirm'; return
      }
      if (result.state !== 'entitled') throw new Error(result.state === 'cancelled_unspent' ? 'This purchase was cancelled without spending.' : 'Purchase has not completed. Reopen this title to recover it.')
      const playback = await rpcClient.call<{ playback_url: string; expires_at: number | null }>({ method: 'content.playback-handle', params: { purchase_id: result.operation_id } })
      if (!current(item)) return
      if (!/^\/api\/rental-playback\/[0-9a-f]{64}$/.test(playback.playback_url)) throw new Error('Invalid playback address.')
      reply(item, { ...playback, playback_url: new URL(playback.playback_url, window.location.origin).href, operation_id: result.operation_id })
      pending = null; request.value = null; quote.value = null
    } catch (cause) {
      if (current(item)) { error.value = cause instanceof Error ? cause.message : 'Could not complete this purchase. Retry to recover its original result.'; phase.value = 'review' }
    }
  }
  async function cancelUnpaid() {
    const item = pending
    if (!item?.quote || !current(item) || phase.value === 'paying' || phase.value === 'loading') return
    phase.value = 'loading'; error.value = ''
    try {
      await installed(item)
      if (!current(item)) return
      const result = await rpcClient.call<{ state: string }>({ method: 'content.cancel-purchase',
        params: { onion: item.quote.seller_onion, operation_id: item.quote.operation_id }, timeout: 120000 })
      if (!current(item)) return
      if (result.state !== 'cancelled_unspent') throw new Error('Cancellation has not been confirmed. Recover this original purchase before trying another payment.')
      item.quote = undefined; quote.value = null; phase.value = 'review'
      error.value = 'The unpaid quote was cancelled. Check purchase to request fresh terms.'
    } catch (cause) { if (current(item)) { error.value = String(cause); phase.value = 'review' } }
  }
  async function handle(event: MessageEvent) {
    if (disposed || context.appId() !== 'indeedhub' || event.source !== context.frameWindow()
      || event.origin !== frameOrigin() || event.data?.type !== 'archipelago-rental-request') return
    const { id, offer } = event.data
    if (typeof id !== 'string' || !/^[0-9a-f-]{36}$/.test(id)) return
    if (event.data.action === 'status') {
      const source = event.source as Window, origin = event.origin, handle = event.data.handle, scope = generation
      const selected = () => !disposed && generation === scope && context.appId() === 'indeedhub' && source === context.frameWindow() && frameOrigin() === origin
      if (typeof handle !== 'string' || !/^[0-9a-f]{64}$/.test(handle)) return
      try {
        const value = await rpcClient.call<{ appId: string; appOrigins: string[] }>({ method: 'media.registration.context', params: {} })
        if (!selected()) return
        if (value.appId !== 'indeedhub' || !value.appOrigins.some(expected => installedOriginMatches(origin, expected))) throw new Error('Installed app changed.')
        const result = await rpcClient.call<{ expires_at: number | null }>({ method: 'content.playback-status', params: { handle } })
        if (selected()) source.postMessage({ type: 'archipelago-rental-response', id, result }, origin)
      } catch (cause) {
        if (selected()) source.postMessage({ type: 'archipelago-rental-response', id, error: String(cause) }, origin)
      }
      return
    }
    if (!supportsRentalPlaybackOrigin(event.origin, window.location.origin)) { (event.source as Window).postMessage({ type: 'archipelago-rental-response', id, error: 'Open IndeeHub from this node’s dashboard using the same LAN hostname and HTTP/HTTPS scheme before renting. This app address cannot receive the playback session cookie; no payment was requested.' }, event.origin); return }
    if (context.consentBusy?.()) { (event.source as Window).postMessage({ type: 'archipelago-rental-response', id, error: 'Finish the other native confirmation first.' }, event.origin); return }
    if (pending) { (event.source as Window).postMessage({ type: 'archipelago-rental-response', id, error: 'Finish or close the current rental confirmation first.' }, event.origin); return }
    const terms = offer?.terms
    if (!terms || typeof offer.title !== 'string' || offer.title.length > 240
      || typeof terms.nodeDid !== 'string' || !terms.nodeDid.startsWith('did:key:')
      || typeof terms.contentId !== 'string' || !/^registered_[a-zA-Z0-9_-]+$/.test(terms.contentId)
      || !/^[0-9a-f]{64}$/.test(terms.sha256) || !Number.isSafeInteger(terms.priceSats) || terms.priceSats < 0
      || !Number.isSafeInteger(terms.viewingSeconds) || terms.viewingSeconds < 1) {
      (event.source as Window).postMessage({ type: 'archipelago-rental-response', id, error: 'Invalid rental terms.' }, event.origin); return
    }
    const item: Pending = { source: event.source as Window, origin: event.origin, id, offer: structuredClone(offer) }
    pending = item
    try { await installed(item); if (current(item)) { request.value = item.offer; phase.value = 'review' } }
    catch (cause) { reply(item, undefined, String(cause)); if (pending === item) pending = null }
  }
  return { isBusy: () => pending !== null, request, quote, phase, error, handle, cancelUnpaid, review: () => run(false), approve: () => run(true), cancel,
    dispose: () => { cancel(); disposed = true } }
}
