import { ref, computed } from 'vue'
import { useFavoritesStore, type FavoriteItem, type FavoriteType } from '@/stores/favorites'

// M13.1 — "For You" feed
export function useForYouFeed() {
  const favoritesStore = useFavoritesStore()

  const forYouItems = computed(() => {
    const items = favoritesStore.sortedItems
    if (items.length === 0) return []

    // Build frequency map by type
    const typeFreq = new Map<FavoriteType, number>()
    for (const item of items) {
      typeFreq.set(item.type, (typeFreq.get(item.type) ?? 0) + 1)
    }

    // Sort types by frequency (most favorited first)
    const sortedTypes = [...typeFreq.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([type]) => type)

    // Return items prioritized by favorite frequency, most recent first
    return [...items].sort((a, b) => {
      const aRank = sortedTypes.indexOf(a.type)
      const bRank = sortedTypes.indexOf(b.type)
      if (aRank !== bRank) return aRank - bRank
      return b.savedAt - a.savedAt
    })
  })

  return { forYouItems }
}

// M13.2 — Content tagging
const TAGS_STORAGE_KEY = 'aiui-content-tags'

export interface ContentTag {
  itemId: string
  tags: string[]
}

const contentTags = ref<Map<string, string[]>>(new Map())

function loadTags() {
  try {
    const stored = localStorage.getItem(TAGS_STORAGE_KEY)
    if (stored) {
      const entries = JSON.parse(stored) as [string, string[]][]
      contentTags.value = new Map(entries)
    }
  } catch { /* ignore */ }
}

function saveTags() {
  localStorage.setItem(TAGS_STORAGE_KEY, JSON.stringify([...contentTags.value.entries()]))
}

loadTags()

export function useContentTags() {
  function getTagsForItem(itemId: string): string[] {
    return contentTags.value.get(itemId) ?? []
  }

  function addTag(itemId: string, tag: string) {
    const tags = getTagsForItem(itemId)
    if (!tags.includes(tag)) {
      contentTags.value.set(itemId, [...tags, tag])
      saveTags()
    }
  }

  function removeTag(itemId: string, tag: string) {
    const tags = getTagsForItem(itemId).filter(t => t !== tag)
    if (tags.length === 0) {
      contentTags.value.delete(itemId)
    } else {
      contentTags.value.set(itemId, tags)
    }
    saveTags()
  }

  const allTags = computed(() => {
    const tagSet = new Set<string>()
    for (const tags of contentTags.value.values()) {
      for (const tag of tags) tagSet.add(tag)
    }
    return [...tagSet].sort()
  })

  const tagCloud = computed(() => {
    const counts = new Map<string, number>()
    for (const tags of contentTags.value.values()) {
      for (const tag of tags) {
        counts.set(tag, (counts.get(tag) ?? 0) + 1)
      }
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([tag, count]) => ({ tag, count }))
  })

  function getItemsByTag(tag: string): string[] {
    const ids: string[] = []
    for (const [id, tags] of contentTags.value.entries()) {
      if (tags.includes(tag)) ids.push(id)
    }
    return ids
  }

  return {
    contentTags,
    allTags,
    tagCloud,
    getTagsForItem,
    addTag,
    removeTag,
    getItemsByTag,
  }
}

// M13.5 — Recently viewed history
const HISTORY_STORAGE_KEY = 'aiui-view-history'
const MAX_HISTORY = 50

export interface HistoryEntry {
  id: string
  type: FavoriteType
  title: string
  subtitle?: string
  viewedAt: number
}

const viewHistory = ref<HistoryEntry[]>([])

function loadHistory() {
  try {
    const stored = localStorage.getItem(HISTORY_STORAGE_KEY)
    if (stored) viewHistory.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function saveHistory() {
  localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(viewHistory.value))
}

loadHistory()

export function useViewHistory() {
  function addToHistory(entry: Omit<HistoryEntry, 'viewedAt'>) {
    // Remove duplicate
    viewHistory.value = viewHistory.value.filter(h => h.id !== entry.id)
    // Add to front
    viewHistory.value.unshift({ ...entry, viewedAt: Date.now() })
    // Cap at MAX_HISTORY
    if (viewHistory.value.length > MAX_HISTORY) {
      viewHistory.value = viewHistory.value.slice(0, MAX_HISTORY)
    }
    saveHistory()
  }

  function clearHistory() {
    viewHistory.value = []
    saveHistory()
  }

  return {
    viewHistory,
    addToHistory,
    clearHistory,
  }
}

// M13.7 — Trending in conversations
const REFERENCE_STORAGE_KEY = 'aiui-content-references'

interface ReferenceEntry {
  id: string
  title: string
  type: FavoriteType
  count: number
  lastReferenced: number
}

const trendingItems = ref<ReferenceEntry[]>([])

function loadReferences() {
  try {
    const stored = localStorage.getItem(REFERENCE_STORAGE_KEY)
    if (stored) trendingItems.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function saveReferences() {
  localStorage.setItem(REFERENCE_STORAGE_KEY, JSON.stringify(trendingItems.value))
}

loadReferences()

export function useTrending() {
  function recordReference(item: { id: string; title: string; type: FavoriteType }) {
    const existing = trendingItems.value.find(t => t.id === item.id)
    if (existing) {
      existing.count++
      existing.lastReferenced = Date.now()
    } else {
      trendingItems.value.push({
        id: item.id,
        title: item.title,
        type: item.type,
        count: 1,
        lastReferenced: Date.now(),
      })
    }
    saveReferences()
  }

  const trending = computed(() => {
    const thirtyDaysAgo = Date.now() - 30 * 86400 * 1000
    return trendingItems.value
      .filter(t => t.lastReferenced > thirtyDaysAgo)
      .sort((a, b) => b.count - a.count)
      .slice(0, 20)
  })

  return {
    trending,
    recordReference,
  }
}
