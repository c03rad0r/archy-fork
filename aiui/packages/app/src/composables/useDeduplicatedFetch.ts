const inflight = new Map<string, Promise<Response>>()

export function useDeduplicatedFetch() {
  async function dedupFetch(url: string, options?: RequestInit): Promise<Response> {
    // Generate a cache key from URL + body
    const bodyStr = options?.body ? String(options.body) : ''
    const key = `${options?.method ?? 'GET'}:${url}:${bodyStr}`

    const existing = inflight.get(key)
    if (existing) return existing.then(r => r.clone())

    const controller = new AbortController()
    const mergedOptions = { ...options, signal: controller.signal }

    const promise = fetch(url, mergedOptions).finally(() => {
      inflight.delete(key)
    })

    inflight.set(key, promise)

    return promise
  }

  function cancelAll() {
    inflight.clear()
  }

  return { dedupFetch, cancelAll }
}
