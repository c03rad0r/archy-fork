import type { WebSearchResult } from '@aiui/core/types/message'
import { apiFetch } from '@/utils/api-fetch'

export async function fetchRssFromUrls(urls: string[]): Promise<WebSearchResult[]> {
  const safe = urls.filter((u) => typeof u === 'string' && /^https?:\/\//i.test(u.trim())).slice(0, 8)
  if (safe.length === 0) return []

  try {
    const params = new URLSearchParams()
    safe.forEach((u) => params.append('url', u))
    const res = await apiFetch(`/api/rss-articles?${params}`, { signal: AbortSignal.timeout(15000) })
    if (!res.ok) return []
    const data = (await res.json()) as { articles?: Array<{ title?: string; url?: string; content?: string; imgSrc?: string }> }
    const articles = data.articles ?? []
    return articles
      .filter((a) => a.title && a.url)
      .map((a) => ({
        title: a.title ?? '',
        url: a.url ?? '',
        content: a.content,
        imgSrc: a.imgSrc,
      }))
  } catch (err) {
    console.warn('[AIUI rss]', err)
    return []
  }
}
