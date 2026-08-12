import { ref, computed, onMounted } from 'vue'
import { encodeNpub } from '@/utils/bech32'

/**
 * NIP-07 browser extension interface (nos2x, Alby, etc.)
 */
interface NostrExtension {
  getPublicKey(): Promise<string>
  signEvent(event: UnsignedNostrEvent): Promise<SignedNostrEvent>
  getRelays?(): Promise<Record<string, { read: boolean; write: boolean }>>
  nip04?: {
    encrypt(pubkey: string, plaintext: string): Promise<string>
    decrypt(pubkey: string, ciphertext: string): Promise<string>
  }
}

export interface UnsignedNostrEvent {
  kind: number
  created_at: number
  tags: string[][]
  content: string
}

export interface SignedNostrEvent extends UnsignedNostrEvent {
  id: string
  pubkey: string
  sig: string
}

declare global {
  interface Window {
    nostr?: NostrExtension
  }
}

const pubkey = ref<string | null>(null)
const isAvailable = ref(false)
const isLoading = ref(false)
const error = ref<string | null>(null)

export function useNostrIdentity() {
  const npub = computed(() => {
    if (!pubkey.value) return null
    try {
      return encodeNpub(pubkey.value)
    } catch {
      return null
    }
  })

  const isLoggedIn = computed(() => !!pubkey.value)

  const truncatedNpub = computed(() => {
    if (!npub.value) return null
    return npub.value.slice(0, 12) + '...' + npub.value.slice(-8)
  })

  function checkAvailability() {
    isAvailable.value = typeof window !== 'undefined' && !!window.nostr
  }

  async function login(): Promise<void> {
    error.value = null
    if (!window.nostr) {
      error.value = 'No Nostr extension detected. Install nos2x, Alby, or another NIP-07 extension.'
      return
    }

    isLoading.value = true
    try {
      const pk = await window.nostr.getPublicKey()
      pubkey.value = pk
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to get public key'
    } finally {
      isLoading.value = false
    }
  }

  async function signEvent(event: UnsignedNostrEvent): Promise<SignedNostrEvent | null> {
    error.value = null
    if (!window.nostr) {
      error.value = 'No Nostr extension detected'
      return null
    }

    try {
      return await window.nostr.signEvent(event)
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to sign event'
      return null
    }
  }

  function logout() {
    pubkey.value = null
    error.value = null
  }

  onMounted(() => {
    checkAvailability()
    // Some extensions load async — recheck after a short delay
    setTimeout(checkAvailability, 500)
  })

  return {
    pubkey,
    npub,
    isAvailable,
    isLoggedIn,
    isLoading,
    error,
    truncatedNpub,
    login,
    logout,
    signEvent,
    checkAvailability,
  }
}
