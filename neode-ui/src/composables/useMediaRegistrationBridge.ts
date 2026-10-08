import { ref, shallowRef } from 'vue'
import { secureUuid } from '@/utils/secureUuid'
import { rpcClient } from '@/api/rpc-client'
import { appPortIsGateFronted } from '@/views/appSession/appSessionConfig'

export const REGISTRATION_SCOPE = 'archipelago.media-registration.approval.v1'
export interface RegistrationIntent {
  version: 1; requestId: string; nonce: string; appAudience: string; nodeDid: string
  producer: string; projectId: string; priceSats: number; viewingSeconds: number
  createdAt: number; expiresAt: number
}
export interface CloudSelection { relative_path: string; payment_methods: string[] }
export interface RegistrationRequest { intent: RegistrationIntent; selection?: CloudSelection; resolution?: boolean }
interface SavedApproval { version: 1; intent: RegistrationIntent; selection: CloudSelection; approvalId: string; event: Record<string, unknown> }
const approvalKey = (intent: RegistrationIntent) => `archipelago:media-approval:${intent.requestId}`
function savedApproval(intent: RegistrationIntent): SavedApproval | null {
  const raw = localStorage.getItem(approvalKey(intent))
  if (raw === null) return null
  if (raw.length > 32768) throw new Error('Saved Cloud approval is damaged. Preserve this operation for recovery.')
  const saved = JSON.parse(raw) as SavedApproval
  if (saved.version !== 1 || !exact(saved.intent, intent) || !saved.selection || !saved.approvalId || !saved.event) {
    throw new Error('Saved Cloud approval differs from this operation. It has not been replaced.')
  }
  return saved
}
interface InstallationContext { appId: string; appAudience: string; nodeDid: string; appOrigins: string[] }
interface FrameContext { appId: () => string; appUrl: () => string; frameWindow: () => Window | null; consentBusy?: () => boolean }
interface Pending { generation: number; mode?: 'resolve'; source: Window; origin: string; requestId: string; intent: RegistrationIntent; selection?: CloudSelection; approvalId?: string; approvedEvent?: Record<string, unknown> }
export function approvalContent(intent: RegistrationIntent, selection: CloudSelection) {
  return { action: 'Register this Cloud video for an IndeeHub project', scope: REGISTRATION_SCOPE,
    intent, selection: { cloudFile: selection.relative_path, paymentMethods: selection.payment_methods } }
}
function exact(left: unknown, right: unknown): boolean {
  if (left === right) return true
  if (!left || !right || typeof left !== 'object' || typeof right !== 'object') return false
  if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right)
    && left.length === right.length && left.every((v, i) => exact(v, right[i]))
  const a = left as Record<string, unknown>, b = right as Record<string, unknown>
  return Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(k => Object.prototype.hasOwnProperty.call(b, k) && exact(a[k], b[k]))
}
function validatedIntent(value: unknown): RegistrationIntent {
  const intent = value as RegistrationIntent
  if (!intent || intent.version !== 1 || !/^[0-9a-f-]{36}$/.test(intent.requestId)
    || !/^[0-9a-f]{64}$/.test(intent.producer) || !/^[0-9a-f]{64}$/.test(intent.nonce)
    || typeof intent.nodeDid !== 'string' || !intent.nodeDid.startsWith('did:key:')
    || typeof intent.appAudience !== 'string' || !intent.appAudience || intent.appAudience.length > 128
    || typeof intent.projectId !== 'string' || !intent.projectId || intent.projectId.length > 128
    || !Number.isSafeInteger(intent.priceSats) || intent.priceSats < 0
    || !Number.isSafeInteger(intent.viewingSeconds) || intent.viewingSeconds < 1
    || !Number.isSafeInteger(intent.createdAt) || !Number.isSafeInteger(intent.expiresAt)
    || intent.expiresAt <= intent.createdAt || intent.expiresAt - intent.createdAt > 600) throw new Error('Invalid node registration intent.')
  return structuredClone(intent)
}
export function installedOriginMatches(actual: string, expected: string): boolean {
  try {
    const a = new URL(actual), e = new URL(expected)
    if (a.origin === e.origin) return true
    // Runtime interface scans may report loopback. Only map that exact configured
    // scheme/port to the dashboard host, never to an arbitrary app-supplied host.
    const sameNode = a.hostname === window.location.hostname
    const host = ['localhost', '127.0.0.1', '[::1]'].includes(e.hostname) || a.hostname === e.hostname
    const scheme = a.protocol === e.protocol || (a.protocol === 'https:' && e.protocol === 'http:'
      && window.location.protocol === 'https:' && appPortIsGateFronted('indeedhub', a.port))
    return host && sameNode && scheme && a.port === e.port
  } catch { return false }
}
export function useMediaRegistrationBridge(context: FrameContext) {
  const request = shallowRef<RegistrationRequest | null>(null)
  const phase = ref<'select' | 'resolve' | 'signing' | 'preparing'>('select')
  const error = ref('')
  let pending: Pending | null = null, disposed = false, generation = 0, validating = 0
  function current(item: Pending): boolean {
    if (disposed || item.generation !== generation || item.source !== context.frameWindow() || context.appId() !== 'indeedhub') return false
    try { return new URL(context.appUrl(), window.location.origin).origin === item.origin } catch { return false }
  }
  async function validateInstallation(item: Pending) {
    const installed = await rpcClient.call<InstallationContext>({ method: 'media.registration.context', params: {} })
    if (!current(item)) throw new Error('The app frame changed. Resume from the current app.')
    if (installed.appId !== 'indeedhub' || installed.appAudience !== item.intent.appAudience
      || installed.nodeDid !== item.intent.nodeDid || !Array.isArray(installed.appOrigins)
      || !installed.appOrigins.some(expected => installedOriginMatches(item.origin, expected))) {
      throw new Error('This request does not match the installed IndeeHub identity and browser origin.')
    }
  }
  function reply(item: Pending, result?: unknown, failure?: string) {
    if (!current(item)) return
    item.source.postMessage({ type: 'archipelago-media-registration-response', id: item.requestId,
      ...(failure ? { error: failure } : { result }) }, item.origin)
  }
  function cancel() {
    if (pending) reply(pending, undefined, phase.value === 'preparing'
      ? 'Preparation may still be running. Resume this same registration.' : 'Cloud selection cancelled.')
    generation++
    pending = null; request.value = null; error.value = ''; phase.value = 'select'
  }
  function approve(selection: CloudSelection) {
    if (!pending || phase.value !== 'select' || !current(pending)) return
    if (!selection.relative_path || selection.relative_path.startsWith('/')
      || selection.relative_path.split('/').some(p => !p || p === '.' || p === '..')
      || !/\.(mp4|m4v|webm|mov)$/i.test(selection.relative_path)
      || !exact(selection.payment_methods, ['cashu'])) { error.value = 'Choose a supported Cloud video.'; return }
    try {
      const old = savedApproval(pending.intent)
      if (old && !exact(old.selection, selection)) throw new Error('This operation already approved another file. Resume its original selection.')
      const saved: SavedApproval = old ?? { version: 1, intent: pending.intent, selection: structuredClone(selection),
        approvalId: secureUuid(), event: { kind: 27236, created_at: Math.floor(Date.now() / 1000),
          tags: [['d', REGISTRATION_SCOPE]], content: JSON.stringify(approvalContent(pending.intent, selection), null, 2) } }
      // Persist owner approval before replying. Storage failure cannot silently
      // turn a later retry into a newly approved file or a replacement operation.
      localStorage.setItem(approvalKey(pending.intent), JSON.stringify(saved))
      pending.selection = saved.selection; pending.approvalId = saved.approvalId; pending.approvedEvent = saved.event
      request.value = { intent: pending.intent, selection: saved.selection }; phase.value = 'signing'
      reply(pending, { selection: saved.selection, approvalId: saved.approvalId, event: saved.event })
    } catch (cause) { error.value = cause instanceof Error ? cause.message : 'Cloud approval could not be saved.' }
  }
  function approveResolution() {
    if (!pending || pending.mode !== 'resolve' || phase.value !== 'resolve' || !current(pending)) return
    const approved = { intent: pending.intent, approvalId: secureUuid(), event: {
      kind: 27237, created_at: Math.floor(Date.now() / 1000), tags: [['d', 'archipelago.media-registration.resolution.v1']],
      content: JSON.stringify({ action: 'Recover prepared video or retire this expired incomplete registration',
        scope: 'archipelago.media-registration.resolution.v1', intent: pending.intent }, null, 2),
    } }
    try {
      localStorage.setItem(approvalKey(pending.intent) + ':resolution', JSON.stringify(approved))
      pending.approvalId = approved.approvalId; pending.approvedEvent = approved.event; phase.value = 'signing'
      reply(pending, { approvalId: approved.approvalId, event: approved.event })
    } catch (cause) { error.value = cause instanceof Error ? cause.message : 'Resolution approval could not be saved.' }
  }
  async function handle(event: MessageEvent) {
    if (disposed || context.appId() !== 'indeedhub' || event.source !== context.frameWindow()) return
    let origin: string
    try { origin = new URL(context.appUrl(), window.location.origin).origin } catch { return }
    if (event.origin !== origin || !event.data || event.data.type !== 'archipelago-media-registration-request') return
    const source = event.source as Window
    const id = event.data.id
    if (typeof id !== 'string' || !/^[0-9a-f-]{36}$/.test(id)) return
    if (context.consentBusy?.()) { source.postMessage({ type: 'archipelago-media-registration-response', id, error: 'Finish the other native confirmation first.' }, origin); return }
    let size = Infinity
    try { size = JSON.stringify(event.data).length } catch { return }
    if (size > 32768) return
    const item: Pending = { generation, source, origin, requestId: id, intent: event.data.intent }
    validating++
    try {
      if (event.data.action === 'complete') {
        item.intent = validatedIntent(event.data.intent)
        if (pending && (phase.value === 'preparing' || !exact(pending.intent, item.intent))) {
          throw new Error('Wait for this registration to finish before clearing its saved approval.')
        }
        // App sends this only after its authenticated backend confirms receipt
        // consumption. Exact-scope removal is idempotent if its reply is lost.
        savedApproval(item.intent)
        localStorage.removeItem(approvalKey(item.intent))
        localStorage.removeItem(approvalKey(item.intent) + ':resolution')
        if (pending && exact(pending.intent, item.intent)) { pending = null; request.value = null; phase.value = 'select' }
        reply(item, { completed: true, requestId: item.intent.requestId }); return
      }
      if (event.data.action === 'resolve') {
        item.intent = validatedIntent(event.data.intent); item.mode = 'resolve'
        if (pending && (phase.value === 'preparing' || !exact(pending.intent, item.intent))) throw new Error('Wait for the current registration operation.')
        await validateInstallation(item)
        if (pending && (phase.value === 'preparing' || !exact(pending.intent, item.intent))) throw new Error('Wait for the current registration operation.')
        const raw = localStorage.getItem(approvalKey(item.intent) + ':resolution')
        let saved: { intent: RegistrationIntent; approvalId: string; event: Record<string, unknown> } | null = null
        if (raw !== null) {
          if (raw.length > 32768) throw new Error('Saved resolution is damaged; preserve the operation.')
          saved = JSON.parse(raw)
          if (!saved || !exact(saved.intent, item.intent) || !saved.approvalId || !saved.event) throw new Error('Saved resolution changed the original intent.')
        }
        pending = item; request.value = { intent: item.intent, resolution: true }; error.value = ''
        if (saved) {
          item.approvalId = saved.approvalId; item.approvedEvent = saved.event; phase.value = 'signing'
          reply(item, { approvalId: saved.approvalId, event: saved.event })
        } else { phase.value = 'resolve' }
        return
      }
      if (event.data.action === 'select' || event.data.action === 'resume') {
        if (pending && (phase.value !== 'signing' || !exact(pending.intent, event.data.intent))) {
          throw new Error('Finish or cancel the current Cloud registration first.')
        }
        item.intent = validatedIntent(event.data.intent)
        const saved = savedApproval(item.intent)
        if (!saved && event.data.action === 'resume') throw new Error('The original owner approval is unavailable. Do not replace this pending operation.')
        if (!saved && item.intent.expiresAt <= Math.floor(Date.now() / 1000)) throw new Error('This registration intent expired. Refresh its terms before selecting a new video.')
        // Verify the installer-owned scope before displaying any Cloud data.
        // This matters for the standalone broker, whose app name is caller-supplied.
        await validateInstallation(item)
        if (pending && (phase.value !== 'signing' || !exact(pending.intent, event.data.intent))) {
          throw new Error('Finish or cancel the current Cloud registration first.')
        }
        pending = item; error.value = ''
        if (saved) {
          item.selection = saved.selection; item.approvalId = saved.approvalId; item.approvedEvent = saved.event
          request.value = { intent: item.intent, selection: saved.selection }; phase.value = 'signing'
          reply(item, { selection: saved.selection, approvalId: saved.approvalId, event: saved.event })
        } else { request.value = { intent: item.intent }; phase.value = 'select' }
        return
      }
      const resolving = event.data.action === 'resolve-submit'
      if ((!resolving && event.data.action !== 'submit') || !pending || phase.value !== 'signing'
        || resolving !== (pending.mode === 'resolve') || !current(pending) || event.data.approvalId !== pending.approvalId
        || !exact(event.data.intent, pending.intent) || (!resolving && !exact(event.data.selection, pending.selection))) {
        throw new Error('Approve this exact Cloud file and project before registering it.')
      }
      const approved = pending
      const signed = event.data.producerEvent
      if (!signed || signed.pubkey !== approved.intent.producer
        || !exact({ kind: signed.kind, created_at: signed.created_at, tags: signed.tags, content: signed.content }, approved.approvedEvent)) {
        throw new Error('The producer signature does not match the original owner-approved event.')
      }
      // The producer event is verified by the node. The host never claims that
      // request-body identity alone is an authenticated producer.
      phase.value = 'preparing'; error.value = ''; approved.requestId = id
      const result = await rpcClient.call({ method: resolving ? 'media.registration.resolve' : 'media.registration.prepare', params: {
        intent: approved.intent, ...(!resolving ? { selection: approved.selection } : {}), producerEvent: event.data.producerEvent,
      }, timeout: 600_000 })
      if (pending !== approved) return
      reply(approved, result); pending = null; request.value = null; phase.value = 'select'
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'Registration was not confirmed. Resume the same operation.'
      reply(item, undefined, message)
      if (pending?.requestId === id) { error.value = message; phase.value = 'signing' }
    } finally { validating-- }
  }
  function dispose() { cancel(); disposed = true }
  return { isBusy: () => pending !== null || validating > 0, request, phase, error, handle, approve, approveResolution, cancel, dispose }
}
