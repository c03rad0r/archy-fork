/** Consent-gated NIP-07 bridge between the dashboard and an iframe app. */

import { ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import type { SelectedIdentity } from './useAppIdentity'
import {
  consentKey,
  hasRememberedConsent,
  rememberConsent,
} from './nostrConsent'

interface BridgeOptions {
  appId: () => string
  appName: () => string
  appUrl: () => string
  frameWindow: () => Window | null
}

export interface BridgeConsentRequest {
  appName: string
  method: string
  identityLabel: string
  eventKind?: number
  content?: string
  resolve: (remember: boolean) => void
  reject: () => void
}

const CONSENT_METHODS = new Set([
  'getPublicKey', 'signEvent',
  'nip04.encrypt', 'nip04.decrypt',
  'nip44.encrypt', 'nip44.decrypt',
])

function senderMatches(expectedUrl: string, senderOrigin: string): boolean {
  try {
    const expected = new URL(expectedUrl, window.location.origin)
    const sender = new URL(senderOrigin)
    return expected.hostname === sender.hostname && expected.port === sender.port
  } catch {
    return false
  }
}

export function useNostrBridge(
  getStoredIdentity: () => SelectedIdentity | null,
  options: BridgeOptions,
) {
  const consentRequest = ref<BridgeConsentRequest | null>(null)
  const showConsent = ref(false)
  const consentPhase = ref<'review' | 'signing' | 'success' | 'error'>('review')
  const consentError = ref('')
  let consentApprovedAt = 0
  let consentGeneration = 0
  let approvedGeneration = 0
  let sessionGeneration = 0
  let disposed = false
  let draining = false
  let presentationComplete: Promise<void> = Promise.resolve()
  let finishErrorPresentation: (() => void) | undefined
  type RequestScope = { appId: string; identityId: string | null; session: number }
  const requests: Array<{ event: MessageEvent; resolve: () => void; scope: RequestScope }> = []

  function requestConsent(
    method: string,
    identityLabel: string,
    eventKind?: number,
    content?: string,
  ): Promise<boolean> {
    return new Promise((resolve, reject) => {
      consentGeneration += 1
      consentRequest.value = {
        appName: options.appName(), method, identityLabel, eventKind, content,
        resolve, reject: () => reject(new Error('Signing request denied.')),
      }
      consentPhase.value = 'review'
      consentError.value = ''
      showConsent.value = true
    })
  }

  function approveConsent(remember: boolean) {
    if (consentPhase.value !== 'review' || !consentRequest.value) return
    consentRequest.value?.resolve(remember)
    consentApprovedAt = Date.now()
    approvedGeneration = consentGeneration
    consentPhase.value = 'signing'
  }

  function denyConsent() {
    consentGeneration += 1
    consentRequest.value?.reject()
    finishErrorPresentation?.()
    finishErrorPresentation = undefined
    consentRequest.value = null
    showConsent.value = false
    consentPhase.value = 'review'
    consentError.value = ''
  }

  async function finishConsentSuccess() {
    const generation = approvedGeneration
    const remaining = Math.max(0, 350 - (Date.now() - consentApprovedAt))
    if (remaining) await new Promise(resolve => setTimeout(resolve, remaining))
    if (generation !== consentGeneration || !showConsent.value) return
    consentPhase.value = 'success'
    await new Promise(resolve => setTimeout(resolve, 325))
    if (generation !== consentGeneration) return
    consentRequest.value = null
    showConsent.value = false
    consentPhase.value = 'review'
  }

  function finishConsentError(error: unknown) {
    consentError.value = error instanceof Error ? error.message : 'The node could not complete this request.'
    consentPhase.value = 'error'
    presentationComplete = new Promise(resolve => { finishErrorPresentation = resolve })
  }

  async function processNostrRequest(event: MessageEvent, scope: RequestScope) {
    if (!event.data || event.data.type !== 'nostr-request') return
    const { id, method, params } = event.data
    const source = event.source as Window | null
    if (
      !source ||
      source !== options.frameWindow() ||
      !senderMatches(options.appUrl(), event.origin)
    ) return

    if (disposed) {
      source.postMessage({ type: 'nostr-response', id, error: 'App session closed.' }, event.origin)
      return
    }
    if (scope.appId !== options.appId() || scope.identityId !== (getStoredIdentity()?.id || null)
      || scope.session !== sessionGeneration) {
      source.postMessage({ type: 'nostr-response', id, error: 'App or identity changed. Please retry.' }, event.origin)
      return
    }
    const requestedSession = sessionGeneration
    const requestedApp = options.appId()
    const storedIdentity = getStoredIdentity()
    const identityId = storedIdentity?.id || null
    const identityScope = identityId || 'node-default'
    const identityLabel = storedIdentity?.name || 'Node default identity'
    const origin = event.origin
    let prompted = false
    let promptGeneration: number | undefined
    const stillCurrent = () => !disposed && requestedSession === sessionGeneration && requestedApp === options.appId()
      && source === options.frameWindow() && senderMatches(options.appUrl(), origin)
      && (getStoredIdentity()?.id || null) === identityId
      && (promptGeneration === undefined || promptGeneration === consentGeneration)

    try {
      if (CONSENT_METHODS.has(method)) {
        const key = consentKey(origin, options.appId(), identityScope, method)
        if (!hasRememberedConsent(key)) {
          prompted = true
          const consent = requestConsent(
            method,
            identityLabel,
            method === 'signEvent' ? params?.event?.kind : undefined,
            method === 'signEvent' ? params?.event?.content : undefined,
          )
          promptGeneration = consentGeneration
          const remember = await consent
          if (!stillCurrent()) throw new Error('App or identity changed. Please retry.')
          if (remember) rememberConsent(key)
        }
      }

      if (!stillCurrent()) throw new Error('App or identity changed. Please retry.')
      let result: unknown
      if (method === 'getPublicKey') {
        if (storedIdentity?.nostr_pubkey) {
          result = storedIdentity.nostr_pubkey
        } else if (identityId) {
          const res = await rpcClient.call<{ nostr_pubkey: string }>({ method: 'identity.get', params: { id: identityId } })
          result = res.nostr_pubkey
        } else {
          const res = await rpcClient.call<{ nostr_pubkey: string }>({ method: 'node.nostr-pubkey' })
          result = res.nostr_pubkey
        }
      } else if (method === 'signEvent') {
        result = identityId
          ? await rpcClient.call<unknown>({ method: 'identity.nostr-sign', params: { id: identityId, event: params.event } })
          : await rpcClient.call<unknown>({ method: 'node.nostr-sign', params: { event: params.event } })
      } else if (method === 'getRelays') {
        result = {}
      } else if (method === 'nip04.encrypt') {
        result = (await rpcClient.call<{ ciphertext: string }>({ method: 'identity.nostr-encrypt-nip04', params: { id: identityId || undefined, pubkey: params.pubkey, plaintext: params.plaintext } })).ciphertext
      } else if (method === 'nip04.decrypt') {
        result = (await rpcClient.call<{ plaintext: string }>({ method: 'identity.nostr-decrypt-nip04', params: { id: identityId || undefined, pubkey: params.pubkey, ciphertext: params.ciphertext } })).plaintext
      } else if (method === 'nip44.encrypt') {
        result = (await rpcClient.call<{ ciphertext: string }>({ method: 'identity.nostr-encrypt-nip44', params: { id: identityId || undefined, pubkey: params.pubkey, plaintext: params.plaintext } })).ciphertext
      } else if (method === 'nip44.decrypt') {
        result = (await rpcClient.call<{ plaintext: string }>({ method: 'identity.nostr-decrypt-nip44', params: { id: identityId || undefined, pubkey: params.pubkey, ciphertext: params.ciphertext } })).plaintext
      } else {
        throw new Error(`Unsupported NIP-07 method: ${method}`)
      }
      if (!stillCurrent()) throw new Error('App or identity changed. Please retry.')
      source.postMessage({ type: 'nostr-response', id, result }, origin)
      if (prompted) presentationComplete = finishConsentSuccess()
    } catch (err) {
      source.postMessage({
        type: 'nostr-response', id,
        error: err instanceof Error ? err.message : 'Unknown error',
      }, origin)
      if (prompted && showConsent.value) finishConsentError(err)
    }
  }

  async function drainRequests() {
    if (draining) return
    draining = true
    try {
      while (requests.length) {
        const next = requests.shift()!
        try { await processNostrRequest(next.event, next.scope) } catch {
          // A removed/navigated frame can reject postMessage; drain the remaining requests.
        } finally { next.resolve() }
        // Preserve the approved success animation and never replace a prompt.
        await presentationComplete
      }
    } finally { draining = false }
  }

  function handleNostrRequest(event: MessageEvent): Promise<void> {
    if (!event.data || event.data.type !== 'nostr-request'
      || !event.source || event.source !== options.frameWindow()
      || !senderMatches(options.appUrl(), event.origin)) return Promise.resolve()
    if (disposed || requests.length >= 16) {
      (event.source as Window).postMessage({ type: 'nostr-response', id: event.data.id,
        error: disposed ? 'App session closed.' : 'Too many signing requests. Please retry.' }, event.origin)
      return Promise.resolve()
    }
    return new Promise(resolve => {
      requests.push({ event, resolve, scope: { appId: options.appId(),
        identityId: getStoredIdentity()?.id || null, session: sessionGeneration } })
      void drainRequests()
    })
  }

  function cancelPending() {
    sessionGeneration += 1
    denyConsent()
    for (const next of requests.splice(0)) {
      try {
        (next.event.source as Window)?.postMessage({ type: 'nostr-response', id: next.event.data.id,
          error: 'App session closed.' }, next.event.origin)
      } catch { /* frame already removed */ }
      next.resolve()
    }
  }

  function dispose() {
    disposed = true
    cancelPending()
  }

  return {
    handleNostrRequest,
    cancelPending,
    dispose,
    showConsent,
    consentRequest,
    consentPhase,
    consentError,
    approveConsent,
    denyConsent,
  }
}
