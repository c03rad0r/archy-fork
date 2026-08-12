import { ref, shallowRef, onUnmounted } from 'vue'

export interface NostrEvent {
  id: string
  pubkey: string
  kind: number
  content: string
  created_at: number
  tags: string[][]
  sig: string
}

export interface NostrNote {
  id: string
  pubkey: string
  authorName?: string
  authorPicture?: string
  nip05?: string
  kind: number
  content: string
  created_at: number
  tags: string[][]
}

export interface RelayConfig {
  url: string
  read: boolean
  write: boolean
}

export interface RelayInfo {
  url: string
  connected: boolean
  read: boolean
  write: boolean
  latencyMs: number | null
}

interface RelayState {
  url: string
  ws: WebSocket | null
  connected: boolean
  read: boolean
  write: boolean
  latencyMs: number | null
  connectTime: number | null
}

const DEFAULT_RELAYS: RelayConfig[] = [
  { url: 'wss://relay.damus.io', read: true, write: true },
  { url: 'wss://nos.lol', read: true, write: true },
  { url: 'wss://relay.snort.social', read: true, write: true },
]

const STORAGE_KEY = 'aiui-nostr-relays'

function loadRelayConfig(): RelayConfig[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) return JSON.parse(stored)
  } catch { /* ignore */ }
  return DEFAULT_RELAYS
}

function saveRelayConfig(configs: RelayConfig[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(configs))
}

const events = shallowRef<NostrNote[]>([])
const isConnected = ref(false)
const relayStates = ref<RelayInfo[]>([])

// Profile metadata cache: pubkey → { name, picture, nip05 }
interface ProfileMeta { name?: string; picture?: string; nip05?: string }
const profileCache = new Map<string, ProfileMeta>()
const pendingProfiles = new Set<string>()

let relays: RelayState[] = []
let subscriptionId: string | null = null
let profileSubId: string | null = null
let initialized = false

function generateSubId(): string {
  return 'aiui-' + Math.random().toString(36).slice(2, 10)
}

function truncatePubkey(pubkey: string): string {
  if (pubkey.length <= 12) return pubkey
  return pubkey.slice(0, 8) + '...' + pubkey.slice(-4)
}

function parseEvent(data: unknown): NostrEvent | null {
  if (!Array.isArray(data)) return null
  if (data[0] !== 'EVENT' || !data[2]) return null
  const evt = data[2]
  if (!evt.id || !evt.pubkey || typeof evt.kind !== 'number' || typeof evt.content !== 'string') {
    return null
  }
  return evt as NostrEvent
}

function handleMetadataEvent(evt: NostrEvent) {
  if (evt.kind !== 0) return
  try {
    const meta = JSON.parse(evt.content) as Record<string, unknown>
    const profile: ProfileMeta = {
      name: (meta.display_name ?? meta.name ?? '') as string || undefined,
      picture: (meta.picture ?? '') as string || undefined,
      nip05: (meta.nip05 ?? '') as string || undefined,
    }
    profileCache.set(evt.pubkey, profile)
    pendingProfiles.delete(evt.pubkey)

    // Update existing notes with this profile data
    const updated = events.value.map(n => {
      if (n.pubkey !== evt.pubkey) return n
      return {
        ...n,
        authorName: profile.name || n.authorName,
        authorPicture: profile.picture,
        nip05: profile.nip05 || n.nip05,
      }
    })
    events.value = updated
  } catch { /* malformed kind 0 */ }
}

function enrichWithProfile(note: NostrNote): NostrNote {
  const cached = profileCache.get(note.pubkey)
  if (cached) {
    return {
      ...note,
      authorName: cached.name || note.authorName,
      authorPicture: cached.picture,
      nip05: cached.nip05 || note.nip05,
    }
  }
  return note
}

function requestProfiles(pubkeys: string[]) {
  const needed = pubkeys.filter(pk => !profileCache.has(pk) && !pendingProfiles.has(pk))
  if (needed.length === 0) return

  for (const pk of needed) pendingProfiles.add(pk)

  // Send kind 0 REQ to connected relays
  const subId = 'prof-' + Math.random().toString(36).slice(2, 8)
  for (const relay of relays) {
    if (relay.connected && relay.ws && relay.read) {
      relay.ws.send(JSON.stringify(['REQ', subId, { kinds: [0], authors: needed, limit: needed.length }]))
    }
  }
}

function addEvent(evt: NostrEvent) {
  // Handle metadata events (kind 0) for profile pictures
  if (evt.kind === 0) {
    handleMetadataEvent(evt)
    return
  }

  const existing = events.value.find(e => e.id === evt.id)
  if (existing) return

  const note = enrichWithProfile({
    id: evt.id,
    pubkey: evt.pubkey,
    authorName: truncatePubkey(evt.pubkey),
    kind: evt.kind,
    content: evt.content,
    created_at: evt.created_at,
    tags: evt.tags ?? [],
  })

  const newEvents = [...events.value, note]
    .sort((a, b) => b.created_at - a.created_at)
    .slice(0, 200)

  events.value = newEvents

  // Queue profile fetch for this author
  requestProfiles([evt.pubkey])
}

function connectRelay(relayState: RelayState) {
  if (relayState.ws) return

  try {
    relayState.connectTime = Date.now()
    const ws = new WebSocket(relayState.url)
    relayState.ws = ws

    ws.onopen = () => {
      relayState.connected = true
      relayState.latencyMs = relayState.connectTime ? Date.now() - relayState.connectTime : null
      updateRelayStates()
      isConnected.value = relays.some(r => r.connected)

      if (relayState.read) {
        if (!subscriptionId) subscriptionId = generateSubId()
        ws.send(JSON.stringify(['REQ', subscriptionId, { kinds: [1], limit: 50 }]))
      }
    }

    ws.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data)
        const evt = parseEvent(data)
        if (evt) addEvent(evt)
      } catch { /* skip */ }
    }

    ws.onclose = () => {
      relayState.connected = false
      relayState.ws = null
      relayState.latencyMs = null
      updateRelayStates()
      isConnected.value = relays.some(r => r.connected)
    }

    ws.onerror = () => { /* triggers onclose */ }
  } catch {
    relayState.connected = false
    updateRelayStates()
  }
}

function updateRelayStates() {
  relayStates.value = relays.map(r => ({
    url: r.url,
    connected: r.connected,
    read: r.read,
    write: r.write,
    latencyMs: r.latencyMs,
  }))
}

function connect() {
  if (initialized) return
  initialized = true

  const configs = loadRelayConfig()
  relays = configs.map(c => ({
    url: c.url,
    ws: null,
    connected: false,
    read: c.read,
    write: c.write,
    latencyMs: null,
    connectTime: null,
  }))
  updateRelayStates()
  relays.forEach(r => connectRelay(r))
}

function disconnect() {
  if (subscriptionId) {
    relays.forEach(r => {
      if (r.ws && r.connected) {
        try { r.ws.send(JSON.stringify(['CLOSE', subscriptionId])) } catch { /* ignore */ }
      }
    })
    subscriptionId = null
  }

  relays.forEach(r => {
    if (r.ws) {
      r.ws.close()
      r.ws = null
      r.connected = false
    }
  })

  updateRelayStates()
  isConnected.value = false
  initialized = false
}

function addRelay(url: string, read = true, write = true) {
  if (relays.some(r => r.url === url)) return
  const state: RelayState = { url, ws: null, connected: false, read, write, latencyMs: null, connectTime: null }
  relays.push(state)
  saveRelayConfig(relays.map(r => ({ url: r.url, read: r.read, write: r.write })))
  updateRelayStates()
  if (initialized) connectRelay(state)
}

function removeRelay(url: string) {
  const idx = relays.findIndex(r => r.url === url)
  if (idx === -1) return
  const relay = relays[idx]
  if (relay.ws) {
    relay.ws.close()
    relay.ws = null
  }
  relays.splice(idx, 1)
  saveRelayConfig(relays.map(r => ({ url: r.url, read: r.read, write: r.write })))
  updateRelayStates()
  isConnected.value = relays.some(r => r.connected)
}

function toggleRelayRead(url: string) {
  const relay = relays.find(r => r.url === url)
  if (!relay) return
  relay.read = !relay.read
  saveRelayConfig(relays.map(r => ({ url: r.url, read: r.read, write: r.write })))
  updateRelayStates()
}

function toggleRelayWrite(url: string) {
  const relay = relays.find(r => r.url === url)
  if (!relay) return
  relay.write = !relay.write
  saveRelayConfig(relays.map(r => ({ url: r.url, read: r.read, write: r.write })))
  updateRelayStates()
}

function testRelay(url: string): Promise<number | null> {
  return new Promise((resolve) => {
    const start = Date.now()
    const timer = setTimeout(() => resolve(null), 5000)
    try {
      const ws = new WebSocket(url)
      ws.onopen = () => {
        const latency = Date.now() - start
        clearTimeout(timer)
        ws.close()
        resolve(latency)
      }
      ws.onerror = () => {
        clearTimeout(timer)
        resolve(null)
      }
    } catch {
      clearTimeout(timer)
      resolve(null)
    }
  })
}

function importNIP65Relays(event: NostrEvent) {
  if (event.kind !== 10002) return
  for (const tag of event.tags) {
    if (tag[0] === 'r' && tag[1]) {
      const url = tag[1]
      const marker = tag[2]
      const read = !marker || marker === 'read'
      const write = !marker || marker === 'write'
      addRelay(url, read, write)
    }
  }
}

const NIP50_RELAYS = [
  'wss://relay.nostr.band',
  'wss://nostr.wine',
]

const searchResults = shallowRef<NostrNote[]>([])
const isSearching = ref(false)

function searchNostr(query: string, kinds?: number[]): void {
  if (!query.trim()) {
    searchResults.value = []
    return
  }

  isSearching.value = true
  searchResults.value = []

  const subId = generateSubId()
  const filter: Record<string, unknown> = { search: query, limit: 50 }
  if (kinds && kinds.length > 0) filter.kinds = kinds

  const results: NostrNote[] = []
  let closedCount = 0

  for (const relayUrl of NIP50_RELAYS) {
    try {
      const ws = new WebSocket(relayUrl)
      const timer = setTimeout(() => {
        ws.close()
      }, 8000)

      ws.onopen = () => {
        ws.send(JSON.stringify(['REQ', subId, filter]))
      }

      ws.onmessage = (msg) => {
        try {
          const data = JSON.parse(msg.data)
          if (Array.isArray(data) && data[0] === 'EVENT' && data[1] === subId && data[2]) {
            const evt = data[2] as NostrEvent
            if (!results.find(r => r.id === evt.id)) {
              results.push({
                id: evt.id,
                pubkey: evt.pubkey,
                authorName: truncatePubkey(evt.pubkey),
                kind: evt.kind,
                content: evt.content,
                created_at: evt.created_at,
                tags: evt.tags ?? [],
              })
              searchResults.value = [...results].sort((a, b) => b.created_at - a.created_at)
            }
          }
          if (Array.isArray(data) && data[0] === 'EOSE' && data[1] === subId) {
            clearTimeout(timer)
            ws.close()
          }
        } catch { /* skip */ }
      }

      ws.onclose = () => {
        closedCount++
        if (closedCount >= NIP50_RELAYS.length) {
          isSearching.value = false
        }
      }

      ws.onerror = () => {
        clearTimeout(timer)
      }
    } catch {
      closedCount++
      if (closedCount >= NIP50_RELAYS.length) {
        isSearching.value = false
      }
    }
  }
}

export interface PublishResult {
  url: string
  success: boolean
  message: string
}

function publishEvent(signedEvent: NostrEvent): Promise<PublishResult[]> {
  const writeRelays = relays.filter(r => r.connected && r.ws && r.write)
  if (writeRelays.length === 0) return Promise.resolve([])

  const results = writeRelays.map(relay => {
    return new Promise<PublishResult>((resolve) => {
      const timer = setTimeout(() => {
        resolve({ url: relay.url, success: false, message: 'Timeout' })
      }, 5000)

      const handler = (msg: MessageEvent) => {
        try {
          const data = JSON.parse(msg.data)
          if (Array.isArray(data) && data[0] === 'OK' && data[1] === signedEvent.id) {
            clearTimeout(timer)
            relay.ws?.removeEventListener('message', handler)
            resolve({
              url: relay.url,
              success: !!data[2],
              message: data[3] ?? (data[2] ? 'Published' : 'Rejected'),
            })
          }
        } catch { /* skip */ }
      }

      relay.ws!.addEventListener('message', handler)
      try {
        relay.ws!.send(JSON.stringify(['EVENT', signedEvent]))
      } catch {
        clearTimeout(timer)
        relay.ws?.removeEventListener('message', handler)
        resolve({ url: relay.url, success: false, message: 'Send failed' })
      }
    })
  })

  addEvent(signedEvent)
  return Promise.all(results)
}

function fetchNote(hexId: string, timeoutMs = 5000): Promise<NostrNote | null> {
  const cached = events.value.find(e => e.id === hexId)
  if (cached) return Promise.resolve(cached)

  return new Promise((resolve) => {
    const subId = generateSubId()
    let resolved = false
    const timer = setTimeout(() => {
      if (!resolved) { resolved = true; resolve(null) }
    }, timeoutMs)

    const relay = relays.find(r => r.connected && r.ws && r.read)
    if (!relay?.ws) {
      clearTimeout(timer)
      resolve(null)
      return
    }

    const handler = (msg: MessageEvent) => {
      try {
        const data = JSON.parse(msg.data)
        if (Array.isArray(data) && data[0] === 'EVENT' && data[1] === subId && data[2]) {
          const evt = data[2] as NostrEvent
          const note: NostrNote = {
            id: evt.id,
            pubkey: evt.pubkey,
            authorName: truncatePubkey(evt.pubkey),
            kind: evt.kind,
            content: evt.content,
            created_at: evt.created_at,
            tags: evt.tags ?? [],
          }
          if (!resolved) { resolved = true; clearTimeout(timer); resolve(note) }
          relay.ws?.removeEventListener('message', handler)
        }
        if (Array.isArray(data) && data[0] === 'EOSE' && data[1] === subId) {
          if (!resolved) { resolved = true; clearTimeout(timer); resolve(null) }
          relay.ws?.removeEventListener('message', handler)
        }
      } catch { /* skip */ }
    }

    relay.ws.addEventListener('message', handler)
    relay.ws.send(JSON.stringify(['REQ', subId, { ids: [hexId], limit: 1 }]))
  })
}

export function useNostr() {
  onUnmounted(() => {
    disconnect()
  })

  return {
    events,
    isConnected,
    relayStates,
    connect,
    disconnect,
    fetchNote,
    publishEvent,
    searchResults,
    isSearching,
    searchNostr,
    addRelay,
    removeRelay,
    toggleRelayRead,
    toggleRelayWrite,
    testRelay,
    importNIP65Relays,
  }
}
