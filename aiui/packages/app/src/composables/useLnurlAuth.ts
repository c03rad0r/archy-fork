import { ref } from 'vue'

const lnurlAuthUrl = ref<string | null>(null)
const isAuthenticated = ref(false)
const lightningIdentity = ref<string | null>(null)

const STORAGE_KEY = 'aiui-lnurl-auth-identity'

function loadIdentity() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) {
      lightningIdentity.value = stored
      isAuthenticated.value = true
    }
  } catch { /* ignore */ }
}

loadIdentity()

export function useLnurlAuth() {
  function generateLnurlAuthUrl(): string {
    // Generate a challenge for LNURL-auth
    // In production, this would come from the server
    const challenge = Array.from(crypto.getRandomValues(new Uint8Array(32)))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('')

    const url = `lnurl-auth://auth?tag=login&k1=${challenge}`
    lnurlAuthUrl.value = url
    return url
  }

  function setIdentity(pubkey: string) {
    lightningIdentity.value = pubkey
    isAuthenticated.value = true
    localStorage.setItem(STORAGE_KEY, pubkey)
  }

  function clearIdentity() {
    lightningIdentity.value = null
    isAuthenticated.value = false
    localStorage.removeItem(STORAGE_KEY)
    lnurlAuthUrl.value = null
  }

  return {
    lnurlAuthUrl,
    isAuthenticated,
    lightningIdentity,
    generateLnurlAuthUrl,
    setIdentity,
    clearIdentity,
  }
}
