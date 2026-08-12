import { ref } from 'vue'
import type { FavoriteType } from '@/stores/favorites'
import { getApiKey } from '@/utils/key-vault'
import { apiFetch } from '@/utils/api-fetch'

const CACHE_KEY = 'aiui-similar-content'
const CACHE_DURATION = 7 * 24 * 60 * 60 * 1000 // 7 days

interface SimilarItem {
  title: string
  type: FavoriteType
  reason: string
}

interface CachedSuggestion {
  itemId: string
  suggestions: SimilarItem[]
  cachedAt: number
}

function loadCache(): CachedSuggestion[] {
  try {
    const stored = localStorage.getItem(CACHE_KEY)
    if (stored) return JSON.parse(stored)
  } catch { /* ignore */ }
  return []
}

function saveCache(cache: CachedSuggestion[]) {
  localStorage.setItem(CACHE_KEY, JSON.stringify(cache))
}

export function useSimilarContent() {
  const suggestions = ref<SimilarItem[]>([])
  const isLoading = ref(false)

  async function fetchSimilar(itemId: string, title: string, type: FavoriteType) {
    // Check cache first
    const cache = loadCache()
    const now = Date.now()
    const cached = cache.find(c => c.itemId === itemId && (now - c.cachedAt) < CACHE_DURATION)
    if (cached) {
      suggestions.value = cached.suggestions
      return
    }

    isLoading.value = true
    suggestions.value = []

    try {
      const apiKey = await getApiKey('claude')
      if (!apiKey) {
        isLoading.value = false
        return
      }

      const typeLabel = type === 'tv' ? 'TV series' : type
      const prompt = `List exactly 3 ${typeLabel}s similar to "${title}". For each, respond with ONLY a JSON array like: [{"title":"Name","reason":"one sentence why"}]. No other text.`

      const base = import.meta.env.BASE_URL || '/'
      const res = await apiFetch(`${base}api/claude/v1/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'x-api-key': apiKey },
        body: JSON.stringify({
          model: 'claude-haiku-4-5-20251001',
          max_tokens: 300,
          messages: [{ role: 'user', content: prompt }],
        }),
      })

      if (!res.ok) {
        isLoading.value = false
        return
      }

      const data = await res.json()
      const text = data.content?.[0]?.text ?? ''

      // Extract JSON array from response
      const match = text.match(/\[[\s\S]*\]/)
      if (match) {
        let parsed: { title: string; reason: string }[]
        try {
          parsed = JSON.parse(match[0]) as { title: string; reason: string }[]
        } catch { return }
        const items: SimilarItem[] = parsed.slice(0, 3).map(p => ({
          title: p.title,
          type,
          reason: p.reason,
        }))
        suggestions.value = items

        // Cache result
        const updatedCache = cache.filter(c => c.itemId !== itemId)
        updatedCache.push({ itemId, suggestions: items, cachedAt: now })
        // Keep cache small
        if (updatedCache.length > 100) updatedCache.splice(0, updatedCache.length - 100)
        saveCache(updatedCache)
      }
    } catch { /* ignore errors */ } finally {
      isLoading.value = false
    }
  }

  return {
    suggestions,
    isLoading,
    fetchSimilar,
  }
}
