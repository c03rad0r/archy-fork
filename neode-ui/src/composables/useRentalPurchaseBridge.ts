import { ref, shallowRef } from 'vue'
import { validCashuIdentity } from './peerCashuPurchase'
import { rpcClient } from '@/api/rpc-client'
import { installedOriginMatches } from './useMediaRegistrationBridge'
interface FrameContext { appId: () => string; appUrl: () => string; frameWindow: () => Window | null; consentBusy?: () => boolean }
export interface RentalOffer { title: string; terms: { nodeDid: string; contentId: string; sha256: string; priceSats: number; viewingSeconds: number } }
interface Quote { network: 'mainnet' | 'testnet'; mint_url: string; state: string; operation_id: string; envelope_sha256: string; wallet_debit_sats: number; gross_token_sats: number; seller_net_sats: number; expires_at: number; seller_onion: string }
interface Pending { source: Window; origin: string; id: string; offer: RentalOffer; quote?: Quote }
type PlaybackAction = 'status' | 'prepare' | 'start'
interface PlaybackBinding { duration?: number; startedAt?: number; expiresAt?: number }
const readyPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
const positiveInteger = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) > 0
function validatePlayback(raw: unknown, action: PlaybackAction, handle: string, original: PlaybackBinding): { result: Record<string, unknown>; binding: PlaybackBinding } {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid playback response.')
  const data = raw as Record<string, unknown>, binding = { ...original }
  const failure = () => { throw new Error('Playback state or original viewing window changed. Recover this purchase without paying again.') }
  const expires = data.expires_at
  if (!(expires === null || positiveInteger(expires))) return failure()
  if (binding.expiresAt !== undefined && expires !== binding.expiresAt) return failure()
  if (positiveInteger(expires)) binding.expiresAt = expires
  if (action === 'status') {
    if ('playback_url' in data) return failure()
    return { result: { expires_at: expires }, binding }
  }
  const state = data.state
  if (state !== 'started' && 'playback_url' in data) return failure()
  if (state === 'unavailable' && action === 'prepare') return { result: {state, expires_at: expires}, binding }
  const start = data.started_at
  if (!((start === null && expires === null) || (positiveInteger(start) && positiveInteger(expires) && expires > start))) return failure()
  if (binding.startedAt !== undefined && start !== binding.startedAt) return failure()
  if (positiveInteger(start)) binding.startedAt = start
  if (state === 'preparing' || state === 'ready') {
    if (action !== 'prepare' || !positiveInteger(data.viewing_seconds)) return failure()
    if (binding.duration !== undefined && data.viewing_seconds !== binding.duration) return failure()
    binding.duration = data.viewing_seconds
    if (positiveInteger(start) && expires !== start + binding.duration) return failure()
    const base = {state, viewing_seconds: binding.duration, started_at: start, expires_at: expires}
    if (state === 'preparing') {
      if (!positiveInteger(data.total_bytes) || !Number.isSafeInteger(data.completed_bytes) || (data.completed_bytes as number) < 0 || (data.completed_bytes as number) > data.total_bytes) return failure()
      return { result: {...base,completed_bytes:data.completed_bytes,total_bytes:data.total_bytes}, binding }
    }
    if (data.handle !== handle || typeof data.ready_id !== 'string' || !readyPattern.test(data.ready_id)) return failure()
    return { result: {...base,handle,ready_id:data.ready_id}, binding }
  }
  if (!positiveInteger(start) || !positiveInteger(expires) || (binding.duration !== undefined && expires !== start + binding.duration)) return failure()
  if (state === 'expired') return { result: {state,started_at:start,expires_at:expires}, binding }
  if (state !== 'started' || action !== 'start' || data.playback_url !== `/api/rental-playback/${handle}`) return failure()
  return { result: {state,started_at:start,expires_at:expires,playback_url:new URL(data.playback_url as string,window.location.origin).href}, binding }
}
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
  const phase = ref<'review' | 'loading' | 'preparing' | 'confirm' | 'paying'>('review'), error = ref('')
  const progress = ref('')
  const playbackBindings = new Map<string, PlaybackBinding>()
  let stopWait: (() => void) | undefined
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
    stopWait?.(); stopWait = undefined; progress.value = ''
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
    phase.value = confirm ? 'paying' : 'loading'; error.value = ''; progress.value = ''
    try {
      const terms = item.offer.terms
      const deadline = Date.now() + 300000
      let result!: Quote & { completed_bytes?: number; total_bytes?: number }
      for (let attempt = 0; attempt < 150; attempt++) {
      await installed(item)
      if (!current(item)) return
      if (context.consentBusy?.()) throw new Error('Finish the other native confirmation first.')
      if (Date.now() >= deadline) throw new Error('Preparation is still running. Check purchase again; no new payment was approved.')
      result = await rpcClient.call<Quote & { completed_bytes?: number; total_bytes?: number }>({ method: 'content.rental-purchase', params: {
        seller_did: terms.nodeDid, content_id: terms.contentId, expected_sha256: terms.sha256,
        expected_price_sats: terms.priceSats, expected_viewing_seconds: terms.viewingSeconds,
        ...(!confirm && attempt === 0 ? { retry_preparation: true } : {}),
        max_wallet_debit: confirm ? item.quote!.wallet_debit_sats : Number.MAX_SAFE_INTEGER,
        ...(confirm ? { consent: { operation_id: item.quote!.operation_id,
          envelope_sha256: item.quote!.envelope_sha256, wallet_debit_sats: item.quote!.wallet_debit_sats } } : {}),
      }, timeout: Math.min(120000, deadline - Date.now()) })
      if (!current(item)) return
      if (result.state !== 'preparing') break
      if (confirm) throw new Error('Payment was approved for the original operation. Check the original purchase to recover its payment and preparation; do not pay again.')
      if (!Number.isSafeInteger(result.completed_bytes) || !Number.isSafeInteger(result.total_bytes)
        || result.total_bytes! < 1 || result.completed_bytes! < 0 || result.completed_bytes! > result.total_bytes!) throw new Error('Invalid media preparation response. No new payment approved.')
      phase.value = 'preparing'
      progress.value = `Preparing video: ${Math.floor(result.completed_bytes! / result.total_bytes! * 100)}%. Your viewing time has not started.`
      if (attempt === 149) throw new Error('Video preparation is still running. Check purchase again to continue; no new payment was approved.')
      await new Promise<void>(resolve => { const timer = setTimeout(() => { stopWait = undefined; resolve() }, 2000); stopWait = () => { clearTimeout(timer); resolve() } })
      if (!current(item)) return
      }
      progress.value = ''
      if (result.state === 'confirmation_required') {
        if (!validCashuIdentity(result) || !Number.isSafeInteger(result.wallet_debit_sats) || result.wallet_debit_sats < terms.priceSats
          || !/^[0-9a-f]{64}$/.test(result.envelope_sha256) || !result.operation_id) throw new Error('Invalid payment confirmation. No payment approved.')
        item.quote = result; quote.value = result; phase.value = 'confirm'; return
      }
      if (result.state !== 'entitled') throw new Error(result.state === 'cancelled_unspent' ? 'This purchase was cancelled without spending.' : 'Purchase has not completed. Reopen this title to recover it.')
      if (!readyPattern.test(result.operation_id)) throw new Error('Invalid original purchase identifier. Recover the original payment.')
      const playback = await rpcClient.call<{ handle: string; expires_at: number | null }>({ method: 'content.playback-handle', params: { purchase_id: result.operation_id } })
      if (!current(item)) return
      if (!/^[0-9a-f]{64}$/.test(playback.handle) || !(playback.expires_at === null || positiveInteger(playback.expires_at))) throw new Error('Invalid playback handle.')
      playbackBindings.set(`${item.origin}:${playback.handle}`, {duration:terms.viewingSeconds,...(playback.expires_at === null ? {} : {expiresAt:playback.expires_at})})
      reply(item, { playbackProtocol: 2, handle: playback.handle, expires_at: playback.expires_at, operation_id: result.operation_id })
      pending = null; request.value = null; quote.value = null
    } catch (cause) {
      if (current(item)) { error.value = cause instanceof Error ? cause.message : 'Could not complete this purchase. Retry to recover its original result.'; phase.value = 'review' }
    }
  }
  async function cancelUnpaid() {
    const item = pending
    if (!item?.quote || !current(item) || phase.value === 'paying' || phase.value === 'loading' || phase.value === 'preparing') return
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
    if (event.data.action === 'cancel') { if (pending?.id === id && event.data.playbackProtocol === 2) cancel(); return }
    if (['status', 'prepare', 'start'].includes(event.data.action)) {
      const source = event.source as Window, origin = event.origin, handle = event.data.handle, scope = generation
      const action = event.data.action as PlaybackAction, readyId = event.data.ready_id
      if (event.data.playbackProtocol !== 2) { source.postMessage({type:'archipelago-rental-response',id,error:'Playback protocol 2 is required.'},origin); return }
      if (action === 'start' && !supportsRentalPlaybackOrigin(origin, window.location.origin)) { source.postMessage({type:'archipelago-rental-response',id,error:'Open the app from this node dashboard before starting playback.'},origin); return }
      if (action === 'start' && (typeof readyId !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(readyId))) return
      if (action === 'prepare' && event.data.retry !== undefined && typeof event.data.retry !== 'boolean') return
      const selected = () => !disposed && generation === scope && context.appId() === 'indeedhub' && source === context.frameWindow() && frameOrigin() === origin
      if (typeof handle !== 'string' || !/^[0-9a-f]{64}$/.test(handle)) return
      try {
        const value = await rpcClient.call<{ appId: string; appOrigins: string[] }>({ method: 'media.registration.context', params: {} })
        if (!selected()) return
        if (value.appId !== 'indeedhub' || !value.appOrigins.some(expected => installedOriginMatches(origin, expected))) throw new Error('Installed app changed.')
        const result = await rpcClient.call<Record<string, unknown>>({ method: `content.playback-${action}`, params: { handle,
          ...(action === 'prepare' ? { retry: event.data.retry === true } : {}), ...(action === 'start' ? {ready_id: readyId} : {}) }, timeout: 30000 })
        if (!selected()) return
        const key = `${origin}:${handle}`
        const validated = validatePlayback(result, action, handle, playbackBindings.get(key) ?? {})
        playbackBindings.set(key, validated.binding)
        source.postMessage({ type: 'archipelago-rental-response', id, result: validated.result }, origin)
      } catch (cause) {
        if (selected()) source.postMessage({ type: 'archipelago-rental-response', id, error: String(cause) }, origin)
      }
      return
    }
    if (event.data.action !== undefined && event.data.action !== 'request') { (event.source as Window).postMessage({type:'archipelago-rental-response',id,error:'Unsupported rental action.'},event.origin); return }
    if (event.data.playbackProtocol !== 2) { (event.source as Window).postMessage({type:'archipelago-rental-response',id,error:'Update this app to playback protocol 2 before renting. No payment was requested.'},event.origin); return }
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
  return { isBusy: () => pending !== null, request, quote, phase, error, progress, handle, cancelUnpaid, review: () => run(false), approve: () => run(true), cancel,
    dispose: () => { cancel(); disposed = true } }
}
