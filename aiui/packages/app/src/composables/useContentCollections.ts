import { ref } from 'vue'
import type { FavoriteType } from '@/stores/favorites'

const STORAGE_KEY = 'aiui-content-collections'

export interface CollectionItem {
  id: string
  type: FavoriteType
  title: string
}

export interface ContentCollection {
  id: string
  name: string
  description: string
  items: CollectionItem[]
  createdAt: number
  updatedAt: number
}

const collections = ref<ContentCollection[]>([])

function load() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) collections.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function save() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(collections.value))
}

load()

export function useContentCollections() {
  function createCollection(name: string, description = ''): ContentCollection {
    const collection: ContentCollection = {
      id: crypto.randomUUID(),
      name,
      description,
      items: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    }
    collections.value.push(collection)
    save()
    return collection
  }

  function deleteCollection(id: string) {
    collections.value = collections.value.filter(c => c.id !== id)
    save()
  }

  function addToCollection(collectionId: string, item: CollectionItem) {
    const collection = collections.value.find(c => c.id === collectionId)
    if (!collection) return
    if (collection.items.find(i => i.id === item.id)) return
    collection.items.push(item)
    collection.updatedAt = Date.now()
    save()
  }

  function removeFromCollection(collectionId: string, itemId: string) {
    const collection = collections.value.find(c => c.id === collectionId)
    if (!collection) return
    collection.items = collection.items.filter(i => i.id !== itemId)
    collection.updatedAt = Date.now()
    save()
  }

  function renameCollection(id: string, name: string) {
    const collection = collections.value.find(c => c.id === id)
    if (!collection) return
    collection.name = name
    collection.updatedAt = Date.now()
    save()
  }

  return {
    collections,
    createCollection,
    deleteCollection,
    addToCollection,
    removeFromCollection,
    renameCollection,
  }
}
