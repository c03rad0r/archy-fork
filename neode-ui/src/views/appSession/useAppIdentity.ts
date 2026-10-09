/** Composable for managing app identity selection and NIP-07 identity injection */

import { type Ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { IS_DEMO } from '@/composables/useDemoIntro'

const IDENTITY_KEY = 'archipelago_app_identity_'

export interface SelectedIdentity {
  id: string
  name: string
  did: string
  pubkey: string
  nostr_pubkey?: string
  nostr_npub?: string
}

function isIdentityAwareApp(id: string): boolean {
  return id === 'indeedhub' || id === 'nostrudel' || id === 'archipelago-source'
}

export function useAppIdentity(
  appId: Ref<string>,
  iframeRef: Ref<HTMLIFrameElement | null>,
  showIdentityPicker: Ref<boolean>,
) {
  function getStoredIdentity(): SelectedIdentity | null {
    try {
      const stored = localStorage.getItem(IDENTITY_KEY + appId.value)
      return stored ? JSON.parse(stored) as SelectedIdentity : null
    } catch { return null }
  }

  function storeIdentity(identity: SelectedIdentity) {
    try { localStorage.setItem(IDENTITY_KEY + appId.value, JSON.stringify(identity)) } catch {}
  }

  /** Forget only this app's automatic identity choice. Saved node identities
   * and their keys remain intact for a later explicit sign-in. */
  function clearRememberedIdentity() {
    try { localStorage.removeItem(IDENTITY_KEY + appId.value) } catch {}
    showIdentityPicker.value = false
  }

  async function sendIdentity(identity: SelectedIdentity) {
    try {
      const challenge = `archipelago-identity:${Date.now()}`
      const sigRes = await rpcClient.call<{ signature: string }>({ method: 'identity.sign', params: { id: identity.id, message: challenge } })
      iframeRef.value?.contentWindow?.postMessage({
        type: 'archipelago:identity', did: identity.did, name: identity.name,
        pubkey: identity.pubkey, nostr_pubkey: identity.nostr_pubkey || null,
        nostr_npub: identity.nostr_npub || null, challenge, signature: sigRes.signature
      }, '*')
    } catch {}
  }

  function onIdentitySelected(identity: SelectedIdentity) {
    showIdentityPicker.value = false
    storeIdentity(identity)
    sendIdentity(identity)
  }

  /** Called on iframe load to inject identity if the app supports it */
  function onIframeLoadIdentity() {
    // Public demo: never interrupt the visitor with the identity picker —
    // the embedded IndeeHub is already signed in via the seeded throwaway
    // demo account (see docker/indee-demo-signin.js). Real-node behavior is
    // untouched (IS_DEMO is compile-time false there).
    if (IS_DEMO) return
    if (isIdentityAwareApp(appId.value)) {
      const stored = getStoredIdentity()
      if (stored) sendIdentity(stored)
      else showIdentityPicker.value = true
    }
  }

  /** Handle identity request messages from iframe */
  function handleIdentityRequest(force = false) {
    if (IS_DEMO) return
    const stored = getStoredIdentity()
    if (stored && !force) sendIdentity(stored)
    else showIdentityPicker.value = true
  }

  function cancelIdentitySelection() {
    showIdentityPicker.value = false
    iframeRef.value?.contentWindow?.postMessage({ type: 'archipelago:identity-cancelled' }, '*')
  }

  return {
    getStoredIdentity,
    sendIdentity,
    onIdentitySelected,
    onIframeLoadIdentity,
    handleIdentityRequest,
    clearRememberedIdentity,
    cancelIdentitySelection,
  }
}
