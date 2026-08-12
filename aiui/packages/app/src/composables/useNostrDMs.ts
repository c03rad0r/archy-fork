import { ref, computed } from 'vue'
import { useNostrIdentity } from './useNostrIdentity'

export interface DirectMessage {
  id: string
  fromPubkey: string
  toPubkey: string
  content: string
  created_at: number
  decrypted: boolean
}

export interface DMThread {
  contactPubkey: string
  contactName: string
  messages: DirectMessage[]
  lastMessage: DirectMessage | null
  unread: number
}

const DB_NAME = 'aiui-nostr-dms'
const DB_VERSION = 1
const STORE_NAME = 'messages'

const threads = ref<DMThread[]>([])
const activeContact = ref<string | null>(null)
const isLoading = ref(false)

let dbPromise: Promise<IDBDatabase> | null = null

function openDMDB(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise
  dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION)
    req.onupgradeneeded = () => {
      const db = req.result
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        const store = db.createObjectStore(STORE_NAME, { keyPath: 'id' })
        store.createIndex('contact', 'contactPubkey', { unique: false })
        store.createIndex('created_at', 'created_at', { unique: false })
      }
    }
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => {
      dbPromise = null
      reject(req.error)
    }
  })
  return dbPromise
}

interface StoredDM extends DirectMessage {
  contactPubkey: string
}

function truncatePubkey(pk: string): string {
  if (pk.length <= 12) return pk
  return pk.slice(0, 8) + '...' + pk.slice(-4)
}

async function saveDM(msg: DirectMessage, myPubkey: string): Promise<void> {
  const db = await openDMDB()
  const contactPubkey = msg.fromPubkey === myPubkey ? msg.toPubkey : msg.fromPubkey
  const record: StoredDM = { ...msg, contactPubkey }

  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite')
    tx.objectStore(STORE_NAME).put(record)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

async function loadAllDMs(): Promise<StoredDM[]> {
  const db = await openDMDB()
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly')
    const req = tx.objectStore(STORE_NAME).getAll()
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

function buildThreads(messages: StoredDM[]): DMThread[] {
  const threadMap = new Map<string, DirectMessage[]>()

  for (const msg of messages) {
    const existing = threadMap.get(msg.contactPubkey) ?? []
    existing.push(msg)
    threadMap.set(msg.contactPubkey, existing)
  }

  const result: DMThread[] = []
  for (const [contactPubkey, msgs] of threadMap) {
    msgs.sort((a, b) => a.created_at - b.created_at)
    result.push({
      contactPubkey,
      contactName: truncatePubkey(contactPubkey),
      messages: msgs,
      lastMessage: msgs[msgs.length - 1] ?? null,
      unread: 0,
    })
  }

  // Sort by latest message
  result.sort((a, b) => (b.lastMessage?.created_at ?? 0) - (a.lastMessage?.created_at ?? 0))
  return result
}

export function useNostrDMs() {
  const { pubkey, isLoggedIn } = useNostrIdentity()

  const activeThread = computed(() => {
    if (!activeContact.value) return null
    return threads.value.find(t => t.contactPubkey === activeContact.value) ?? null
  })

  async function loadDMs() {
    if (!isLoggedIn.value) return
    isLoading.value = true
    try {
      const all = await loadAllDMs()
      threads.value = buildThreads(all)
    } catch {
      // IDB error
    } finally {
      isLoading.value = false
    }
  }

  async function sendDM(toPubkey: string, plaintext: string): Promise<boolean> {
    if (!window.nostr?.nip04 || !pubkey.value) return false

    try {
      const encrypted = await window.nostr.nip04.encrypt(toPubkey, plaintext)

      const unsigned = {
        kind: 4,
        created_at: Math.floor(Date.now() / 1000),
        tags: [['p', toPubkey]],
        content: encrypted,
      }

      const signed = await window.nostr.signEvent(unsigned)
      if (!signed) return false

      // Store locally
      const dm: DirectMessage = {
        id: signed.id,
        fromPubkey: pubkey.value,
        toPubkey,
        content: plaintext,
        created_at: signed.created_at,
        decrypted: true,
      }
      await saveDM(dm, pubkey.value)

      // Broadcast to relays (reuse useNostr relay infra)
      const { publishEvent } = await import('./useNostr').then(m => m.useNostr())
      await publishEvent(signed)

      // Refresh threads
      const all = await loadAllDMs()
      threads.value = buildThreads(all)

      return true
    } catch {
      return false
    }
  }

  async function receiveDM(eventId: string, fromPubkey: string, encryptedContent: string, createdAt: number): Promise<void> {
    if (!window.nostr?.nip04 || !pubkey.value) return

    try {
      const plaintext = await window.nostr.nip04.decrypt(fromPubkey, encryptedContent)

      const dm: DirectMessage = {
        id: eventId,
        fromPubkey,
        toPubkey: pubkey.value,
        content: plaintext,
        created_at: createdAt,
        decrypted: true,
      }
      await saveDM(dm, pubkey.value)

      const all = await loadAllDMs()
      threads.value = buildThreads(all)
    } catch {
      // Decryption failed
    }
  }

  function selectContact(contactPubkey: string) {
    activeContact.value = contactPubkey
  }

  function clearActiveContact() {
    activeContact.value = null
  }

  return {
    threads,
    activeThread,
    activeContact,
    isLoading,
    loadDMs,
    sendDM,
    receiveDM,
    selectContact,
    clearActiveContact,
  }
}
