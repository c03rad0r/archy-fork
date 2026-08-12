import type { AIUIPlugin, PluginContext } from '@aiui/core/types/plugin'

export interface WikipediaResult {
  title: string
  extract: string
  thumbnail?: string
  url: string
}

export async function searchWikipedia(query: string): Promise<WikipediaResult | null> {
  try {
    const searchUrl = `https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(query)}`
    const res = await fetch(searchUrl)
    if (!res.ok) {
      // Try search API as fallback
      const searchRes = await fetch(
        `https://en.wikipedia.org/w/api.php?action=opensearch&search=${encodeURIComponent(query)}&limit=1&format=json&origin=*`
      )
      if (!searchRes.ok) return null
      const data = await searchRes.json()
      if (!data[1]?.[0]) return null
      // Fetch the summary for the first result
      const summaryRes = await fetch(
        `https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(data[1][0])}`
      )
      if (!summaryRes.ok) return null
      const summary = await summaryRes.json()
      return {
        title: summary.title,
        extract: summary.extract ?? '',
        thumbnail: summary.thumbnail?.source,
        url: summary.content_urls?.desktop?.page ?? `https://en.wikipedia.org/wiki/${encodeURIComponent(data[1][0])}`,
      }
    }
    const data = await res.json()
    return {
      title: data.title,
      extract: data.extract ?? '',
      thumbnail: data.thumbnail?.source,
      url: data.content_urls?.desktop?.page ?? `https://en.wikipedia.org/wiki/${encodeURIComponent(query)}`,
    }
  } catch {
    return null
  }
}

export const wikipediaPlugin: AIUIPlugin = {
  id: 'wikipedia',
  name: 'Wikipedia',
  version: '1.0.0',
  type: 'search',
  description: 'Search Wikipedia articles with /wiki command',
  async init(_context: PluginContext) {},
  async destroy() {},
  async isAvailable() { return true },
}
