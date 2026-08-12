/** Composable for NIP-07 Nostr signing bridge between parent and iframe */

import { rpcClient } from '@/api/rpc-client'
import type { SelectedIdentity } from './useAppIdentity'

export function useNostrBridge(
  getStoredIdentity: () => SelectedIdentity | null,
  getAppUrl: () => string,
) {
  async function handleNostrRequest(event: MessageEvent) {
    const { id, method, params } = event.data
    const source = event.source as Window | null
    if (!source) return
    const storedIdentity = getStoredIdentity()
    const identityId = storedIdentity?.id || null
    if (import.meta.env.DEV) console.log(`[NIP-07] ${method} identityId=${identityId} storedPubkey=${storedIdentity?.nostr_pubkey?.slice(0, 12) || 'none'}`)

    try {
      let result: unknown
      if (method === 'getPublicKey') {
        // Use stored nostr_pubkey directly if available (avoids RPC call that may 401)
        if (storedIdentity?.nostr_pubkey) {
          result = storedIdentity.nostr_pubkey
          if (import.meta.env.DEV) console.log('[NIP-07] getPublicKey from stored identity:', (result as string).slice(0, 12))
        } else if (identityId) {
          const res = await rpcClient.call<{ nostr_pubkey: string }>({ method: 'identity.get', params: { id: identityId } })
          result = res.nostr_pubkey
        } else {
          const res = await rpcClient.call<{ nostr_pubkey: string }>({ method: 'node.nostr-pubkey' })
          result = res.nostr_pubkey
        }
      } else if (method === 'signEvent') {
        if (import.meta.env.DEV) console.log(`[NIP-07] signEvent kind=${params.event?.kind} using identity=${identityId || 'node-default'}`)
        if (identityId) {
          result = await rpcClient.call<unknown>({ method: 'identity.nostr-sign', params: { id: identityId, event: params.event } })
        } else {
          result = await rpcClient.call<unknown>({ method: 'node.nostr-sign', params: { event: params.event } })
        }
        if (import.meta.env.DEV) console.log('[NIP-07] signEvent OK')
      } else if (method === 'getRelays') { result = {} }
      else if (method === 'nip04.encrypt') { result = (await rpcClient.call<{ ciphertext: string }>({ method: 'identity.nostr-encrypt-nip04', params: { id: identityId || undefined, pubkey: params.pubkey, plaintext: params.plaintext } })).ciphertext }
      else if (method === 'nip04.decrypt') { result = (await rpcClient.call<{ plaintext: string }>({ method: 'identity.nostr-decrypt-nip04', params: { id: identityId || undefined, pubkey: params.pubkey, ciphertext: params.ciphertext } })).plaintext }
      else if (method === 'nip44.encrypt') { result = (await rpcClient.call<{ ciphertext: string }>({ method: 'identity.nostr-encrypt-nip44', params: { id: identityId || undefined, pubkey: params.pubkey, plaintext: params.plaintext } })).ciphertext }
      else if (method === 'nip44.decrypt') { result = (await rpcClient.call<{ plaintext: string }>({ method: 'identity.nostr-decrypt-nip44', params: { id: identityId || undefined, pubkey: params.pubkey, ciphertext: params.ciphertext } })).plaintext }
      else { throw new Error(`Unsupported NIP-07 method: ${method}`) }
      const url = getAppUrl()
      const targetOrigin = url ? new URL(url).origin : '*'
      source.postMessage({ type: 'nostr-response', id, result }, targetOrigin)
    } catch (err) {
      if (import.meta.env.DEV) console.error(`[NIP-07] ${method} FAILED:`, err instanceof Error ? err.message : err)
      const url = getAppUrl()
      const targetOrigin = url ? new URL(url).origin : '*'
      source.postMessage({ type: 'nostr-response', id, error: err instanceof Error ? err.message : 'Unknown error' }, targetOrigin)
    }
  }

  return { handleNostrRequest }
}
