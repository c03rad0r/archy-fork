import { ref } from 'vue'

interface CacheEntry {
  verified: boolean
  pubkey: string
  timestamp: number
}

const CACHE_TTL = 24 * 60 * 60 * 1000 // 24 hours
const STORAGE_KEY = 'aiui-nip05-cache'

const verificationCache = ref<Map<string, CacheEntry>>(new Map())

function loadCache() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) {
      const entries = JSON.parse(stored) as [string, CacheEntry][]
      const now = Date.now()
      const valid = entries.filter(([, e]) => now - e.timestamp < CACHE_TTL)
      verificationCache.value = new Map(valid)
    }
  } catch { /* ignore */ }
}

function saveCache() {
  try {
    const entries = Array.from(verificationCache.value.entries())
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries))
  } catch { /* ignore */ }
}

loadCache()

export function useNip05Verification() {
  async function verifyNip05(nip05: string, expectedPubkey: string): Promise<boolean> {
    const cacheKey = `${nip05}:${expectedPubkey}`
    const cached = verificationCache.value.get(cacheKey)
    if (cached && Date.now() - cached.timestamp < CACHE_TTL) {
      return cached.verified
    }

    try {
      const [name, domain] = nip05.split('@')
      if (!name || !domain) return false

      const res = await fetch(`https://${domain}/.well-known/nostr.json?name=${encodeURIComponent(name)}`)
      if (!res.ok) return cacheResult(cacheKey, false, expectedPubkey)

      const data = await res.json()
      const pubkey = data?.names?.[name]
      const verified = pubkey === expectedPubkey

      return cacheResult(cacheKey, verified, expectedPubkey)
    } catch {
      return cacheResult(cacheKey, false, expectedPubkey)
    }
  }

  function cacheResult(key: string, verified: boolean, pubkey: string): boolean {
    verificationCache.value.set(key, { verified, pubkey, timestamp: Date.now() })
    saveCache()
    return verified
  }

  function isVerified(nip05: string, pubkey: string): boolean | null {
    const cached = verificationCache.value.get(`${nip05}:${pubkey}`)
    if (!cached || Date.now() - cached.timestamp >= CACHE_TTL) return null
    return cached.verified
  }

  return {
    verifyNip05,
    isVerified,
  }
}
