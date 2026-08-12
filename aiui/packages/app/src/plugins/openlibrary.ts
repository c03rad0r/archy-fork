import type { AIUIPlugin, PluginContext } from '@aiui/core/types/plugin'

export interface OpenLibraryBook {
  title: string
  author: string
  year?: number
  coverId?: number
  key: string
}

export async function searchOpenLibrary(query: string): Promise<OpenLibraryBook[]> {
  try {
    const res = await fetch(
      `https://openlibrary.org/search.json?q=${encodeURIComponent(query)}&limit=10&fields=title,author_name,first_publish_year,cover_i,key`
    )
    if (!res.ok) return []
    const data = await res.json()
    return (data.docs ?? []).map((doc: Record<string, unknown>) => ({
      title: doc.title as string,
      author: (doc.author_name as string[])?.[0] ?? 'Unknown',
      year: doc.first_publish_year as number | undefined,
      coverId: doc.cover_i as number | undefined,
      key: doc.key as string,
    }))
  } catch {
    return []
  }
}

export function getOpenLibraryCoverUrl(coverId: number, size: 'S' | 'M' | 'L' = 'M'): string {
  return `https://covers.openlibrary.org/b/id/${coverId}-${size}.jpg`
}

export const openLibraryPlugin: AIUIPlugin = {
  id: 'openlibrary',
  name: 'Open Library',
  version: '1.0.0',
  type: 'search',
  description: 'Search books from Open Library',
  async init(_context: PluginContext) {},
  async destroy() {},
  async isAvailable() { return true },
}
