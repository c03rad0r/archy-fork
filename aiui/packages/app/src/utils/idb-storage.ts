import type { Conversation } from '@aiui/core/types/message'
import {
  isCryptoEnabled,
  getSessionKey,
  encryptToString,
  decryptFromString,
} from './crypto'
import { toRaw } from 'vue'

const DB_NAME = 'aiui-store'
const DB_VERSION = 1
const STORE_NAME = 'conversations'

let dbPromise: Promise<IDBDatabase> | null = null

export function openDB(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise
  dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION)
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, { keyPath: 'id' })
        store.createIndex('updatedAt', 'updatedAt', { unique: false })
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => {
      dbPromise = null
      reject(request.error)
    }
  })
  return dbPromise
}

interface EncryptedRecord {
  id: string
  updatedAt: number
  encrypted: string
}

async function encryptConversation(conv: Conversation): Promise<EncryptedRecord | Conversation> {
  const key = getSessionKey()
  if (!isCryptoEnabled() || !key) return conv
  const encrypted = await encryptToString(JSON.stringify(conv), key)
  return { id: conv.id, updatedAt: conv.updatedAt, encrypted }
}

async function decryptConversation(record: EncryptedRecord | Conversation): Promise<Conversation | null> {
  if (!('encrypted' in record)) return record as Conversation
  const key = getSessionKey()
  if (!key) return null
  try {
    const json = await decryptFromString(record.encrypted, key)
    return JSON.parse(json) as Conversation
  } catch {
    return null // Wrong passphrase or corrupted data
  }
}

/**
 * Deep-unwrap Vue reactive proxies before storing in IDB.
 * structuredClone on a Proxy can silently produce empty objects or throw.
 */
function toPlain(conv: Conversation): Conversation {
  return JSON.parse(JSON.stringify(toRaw(conv)))
}

export async function saveConversation(conv: Conversation): Promise<void> {
  const db = await openDB()
  const plain = toPlain(conv)
  const record = await encryptConversation(plain)
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite')
    tx.objectStore(STORE_NAME).put(record)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

export async function loadAllConversations(): Promise<Map<string, Conversation>> {
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly')
    const request = tx.objectStore(STORE_NAME).index('updatedAt').getAll()
    request.onsuccess = async () => {
      const map = new Map<string, Conversation>()
      for (const record of request.result) {
        const conv = await decryptConversation(record)
        if (conv) map.set(conv.id, conv)
      }
      resolve(map)
    }
    request.onerror = () => reject(request.error)
  })
}

export async function deleteConversation(id: string): Promise<void> {
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite')
    tx.objectStore(STORE_NAME).delete(id)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

export function isIDBAvailable(): boolean {
  return typeof indexedDB !== 'undefined'
}
