import { apiFetch } from '@/utils/api-fetch'

export interface WebSearchResult {
  title: string
  url: string
  content?: string
  imgSrc?: string
}

// Every other endpoint in this app is built from BASE_URL; this one was
// hardcoded to a leading slash, so embedded (BASE_URL `/aiui/`) it asked
// the HOST for `/api/web-search` instead of `/aiui/api/web-search`. Only
// the `/aiui/`-scoped location proxies SearXNG, so the request hit the
// node's own API gate and came back 403 — and the CSP, which allows the
// AIUI-scoped path, refused the connection on top. Web search could never
// have worked embedded, no matter what SearXNG was doing.
const BASE = import.meta.env.BASE_URL || '/'

export async function searchWeb(query: string): Promise<WebSearchResult[]> {
  if (!query.trim()) return []
  try {
    const params = new URLSearchParams({ q: query.trim() })
    const res = await apiFetch(`${BASE}api/web-search?${params}`, { signal: AbortSignal.timeout(10000) })
    if (!res.ok) {
      const body = await res.text().catch(() => '')
      console.warn('[AIUI web-search]', res.status, body)
      return []
    }
    const data = (await res.json()) as { results?: Array<WebSearchResult & { imgSrc?: string }>; error?: string }
    if (data.error) {
      console.warn('[AIUI web-search]', data.error)
      return []
    }
    const results = data.results ?? []
    console.log('[AIUI web-search]', query.slice(0, 50), '→', results.length, 'results')
    return results
  } catch (err) {
    console.warn('[AIUI web-search] failed:', err)
    return []
  }
}
