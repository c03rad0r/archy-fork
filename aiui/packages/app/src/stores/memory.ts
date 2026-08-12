import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface MemoryItem {
  id: string
  text: string
}

const STORAGE_KEY = 'aiui-memory'
const MAX_ITEMS = 20

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

function loadFromStorage(): MemoryItem[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    return JSON.parse(raw)
  } catch {
    return []
  }
}

function saveToStorage(items: MemoryItem[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(items))
}

export const useMemoryStore = defineStore('memory', () => {
  const items = ref<MemoryItem[]>(loadFromStorage())

  const isFull = computed(() => items.value.length >= MAX_ITEMS)

  function persist() {
    saveToStorage(items.value)
  }

  function addItem(text: string): MemoryItem | null {
    if (items.value.length >= MAX_ITEMS) return null
    const trimmed = text.trim()
    if (!trimmed) return null
    const item: MemoryItem = { id: generateId(), text: trimmed }
    items.value.push(item)
    persist()
    return item
  }

  function updateItem(id: string, text: string) {
    const item = items.value.find(x => x.id === id)
    if (!item) return
    item.text = text.trim()
    persist()
  }

  function deleteItem(id: string) {
    const idx = items.value.findIndex(x => x.id === id)
    if (idx !== -1) {
      items.value.splice(idx, 1)
      persist()
    }
  }

  /** Build the memory section for the system prompt */
  function buildMemoryContext(): string {
    if (items.value.length === 0) return ''
    const facts = items.value.map(i => `- ${i.text}`).join('\n')
    return `\n\n**User memory (always remember these facts):**\n${facts}`
  }

  return {
    items,
    isFull,
    addItem,
    updateItem,
    deleteItem,
    buildMemoryContext,
  }
})
