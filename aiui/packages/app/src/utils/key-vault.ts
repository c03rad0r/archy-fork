/**
 * Encrypted API key vault using IndexedDB + AES-256-GCM.
 * Keys are encrypted with the session key derived from passphrase.
 * Refuses to store keys when encryption is unavailable.
 */
import {
  isCryptoEnabled,
  getSessionKey,
  encryptToString,
  decryptFromString,
} from './crypto'

const DB_NAME = 'aiui-vault'
const DB_VERSION = 1
const STORE_NAME = 'api-keys'

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION)
    req.onupgradeneeded = () => {
      const db = req.result
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'provider' })
      }
    }
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

interface VaultRecord {
  provider: string
  value: string // encrypted or plaintext API key
  encrypted: boolean
}

export async function storeApiKey(provider: string, key: string): Promise<void> {
  if (typeof indexedDB === 'undefined') return
  const db = await openDB()
  const sessionKey = getSessionKey()

  if (!isCryptoEnabled() || !sessionKey) {
    throw new Error('Encryption required: cannot store API keys without an active session key. Please set a passphrase first.')
  }

  const value = await encryptToString(key, sessionKey)
  const record: VaultRecord = { provider, value, encrypted: true }

  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite')
    tx.objectStore(STORE_NAME).put(record)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

export async function getApiKey(provider: string): Promise<string | null> {
  if (typeof indexedDB === 'undefined') return null
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly')
    const req = tx.objectStore(STORE_NAME).get(provider)
    req.onsuccess = async () => {
      const record = req.result as VaultRecord | undefined
      if (!record) {
        resolve(null)
        return
      }
      if (record.encrypted) {
        const sessionKey = getSessionKey()
        if (!sessionKey) {
          resolve(null)
          return
        }
        try {
          const decrypted = await decryptFromString(record.value, sessionKey)
          resolve(decrypted)
        } catch {
          resolve(null)
        }
      } else {
        resolve(record.value)
      }
    }
    req.onerror = () => reject(req.error)
  })
}

export async function deleteApiKey(provider: string): Promise<void> {
  if (typeof indexedDB === 'undefined') return
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite')
    tx.objectStore(STORE_NAME).delete(provider)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

export async function listProviders(): Promise<string[]> {
  if (typeof indexedDB === 'undefined') return []
  const db = await openDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly')
    const req = tx.objectStore(STORE_NAME).getAllKeys()
    req.onsuccess = () => resolve(req.result as string[])
    req.onerror = () => reject(req.error)
  })
}

export function maskApiKey(key: string): string {
  if (key.length <= 4) return '****'
  return '****' + key.slice(-4)
}
