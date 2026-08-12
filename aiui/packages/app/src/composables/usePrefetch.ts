const prefetchCache = new Map<string, { data: unknown; expiresAt: number }>()
const CACHE_DURATION = 5 * 60 * 1000 // 5 minutes

export function usePrefetch() {
  function getCached<T>(key: string): T | null {
    const entry = prefetchCache.get(key)
    if (!entry) return null
    if (Date.now() > entry.expiresAt) {
      prefetchCache.delete(key)
      return null
    }
    return entry.data as T
  }

  function setCached(key: string, data: unknown) {
    prefetchCache.set(key, { data, expiresAt: Date.now() + CACHE_DURATION })
  }

  async function prefetch(key: string, fetcher: () => Promise<unknown>) {
    if (getCached(key)) return
    try {
      const data = await fetcher()
      setCached(key, data)
    } catch { /* ignore prefetch errors */ }
  }

  function clearCache() {
    prefetchCache.clear()
  }

  return { getCached, setCached, prefetch, clearCache }
}
