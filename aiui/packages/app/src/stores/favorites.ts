import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export type FavoriteType = 'film' | 'song' | 'podcast' | 'book' | 'tv' | 'place' | 'article'

export interface FavoriteItem {
  id: string
  type: FavoriteType
  title: string
  subtitle?: string
  data: unknown
  savedAt: number
}

const IDB_STORE = 'favorites'
const DB_NAME = 'aiui-favorites'
const DB_VERSION = 1

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION)
    req.onupgradeneeded = () => {
      const db = req.result
      if (!db.objectStoreNames.contains(IDB_STORE)) {
        const store = db.createObjectStore(IDB_STORE, { keyPath: 'id' })
        store.createIndex('type', 'type', { unique: false })
        store.createIndex('savedAt', 'savedAt', { unique: false })
      }
    }
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function idbGetAll(): Promise<FavoriteItem[]> {
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(IDB_STORE, 'readonly')
    const store = tx.objectStore(IDB_STORE)
    const req = store.getAll()
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function idbPut(item: FavoriteItem): Promise<void> {
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(IDB_STORE, 'readwrite')
    const store = tx.objectStore(IDB_STORE)
    const req = store.put(item)
    req.onsuccess = () => resolve()
    req.onerror = () => reject(req.error)
  })
}

async function idbDelete(id: string): Promise<void> {
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(IDB_STORE, 'readwrite')
    const store = tx.objectStore(IDB_STORE)
    const req = store.delete(id)
    req.onsuccess = () => resolve()
    req.onerror = () => reject(req.error)
  })
}

export const useFavoritesStore = defineStore('favorites', () => {
  const items = ref<FavoriteItem[]>([])
  const loaded = ref(false)

  async function loadFavorites() {
    try {
      items.value = await idbGetAll()
    } catch {
      // IDB unavailable — in-memory only
    }
    loaded.value = true
  }

  loadFavorites()

  const sortedItems = computed(() =>
    [...items.value].sort((a, b) => b.savedAt - a.savedAt)
  )

  function isFavorited(id: string): boolean {
    return items.value.some(i => i.id === id)
  }

  async function addFavorite(item: Omit<FavoriteItem, 'savedAt'>) {
    if (isFavorited(item.id)) return
    const fav: FavoriteItem = { ...item, savedAt: Date.now() }
    items.value = [...items.value, fav]
    idbPut(fav).catch(() => {})
  }

  async function removeFavorite(id: string) {
    items.value = items.value.filter(i => i.id !== id)
    idbDelete(id).catch(() => {})
  }

  async function toggleFavorite(item: Omit<FavoriteItem, 'savedAt'>) {
    if (isFavorited(item.id)) {
      await removeFavorite(item.id)
    } else {
      await addFavorite(item)
    }
  }

  function getFavoritesByType(type: FavoriteType): FavoriteItem[] {
    return sortedItems.value.filter(i => i.type === type)
  }

  return {
    items,
    sortedItems,
    loaded,
    isFavorited,
    addFavorite,
    removeFavorite,
    toggleFavorite,
    getFavoritesByType,
  }
})
