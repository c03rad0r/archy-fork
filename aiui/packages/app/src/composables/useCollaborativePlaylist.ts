import { ref, computed } from 'vue'
import { useNostrIdentity } from './useNostrIdentity'
import { useNostr } from './useNostr'

export interface PlaylistItem {
  type: string
  title: string
  artist?: string
  id?: string
  addedBy: string
  addedAt: number
}

export interface CollaborativePlaylist {
  id: string
  dTag: string
  title: string
  description: string
  items: PlaylistItem[]
  contributors: string[]
  createdAt: number
  updatedAt: number
}

const STORAGE_KEY = 'aiui-collaborative-playlists'

const playlists = ref<CollaborativePlaylist[]>([])

function loadPlaylists() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) playlists.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function savePlaylists() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(playlists.value))
}

loadPlaylists()

export function useCollaborativePlaylist() {
  const { signEvent, pubkey, isLoggedIn } = useNostrIdentity()
  const { publishEvent } = useNostr()

  const isPublishing = ref(false)

  function createPlaylist(title: string, description = ''): CollaborativePlaylist {
    const playlist: CollaborativePlaylist = {
      id: crypto.randomUUID(),
      dTag: `aiui-playlist-${crypto.randomUUID().slice(0, 8)}`,
      title,
      description,
      items: [],
      contributors: pubkey.value ? [pubkey.value] : [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    }
    playlists.value.push(playlist)
    savePlaylists()
    return playlist
  }

  function addItem(playlistId: string, item: Omit<PlaylistItem, 'addedBy' | 'addedAt'>) {
    const playlist = playlists.value.find((p) => p.id === playlistId)
    if (!playlist) return
    playlist.items.push({
      ...item,
      addedBy: pubkey.value ?? 'local',
      addedAt: Date.now(),
    })
    playlist.updatedAt = Date.now()
    savePlaylists()
  }

  function removeItem(playlistId: string, index: number) {
    const playlist = playlists.value.find((p) => p.id === playlistId)
    if (!playlist) return
    playlist.items.splice(index, 1)
    playlist.updatedAt = Date.now()
    savePlaylists()
  }

  function inviteContributor(playlistId: string, npub: string) {
    const playlist = playlists.value.find((p) => p.id === playlistId)
    if (!playlist) return
    if (!playlist.contributors.includes(npub)) {
      playlist.contributors.push(npub)
      savePlaylists()
    }
  }

  function deletePlaylist(playlistId: string) {
    playlists.value = playlists.value.filter((p) => p.id !== playlistId)
    savePlaylists()
  }

  async function publishToNostr(playlistId: string): Promise<boolean> {
    if (!isLoggedIn.value || !pubkey.value) return false
    const playlist = playlists.value.find((p) => p.id === playlistId)
    if (!playlist) return false

    isPublishing.value = true
    try {
      // NIP-51 kind:30004 — Categorized People/Content List
      const tags: string[][] = [
        ['d', playlist.dTag],
        ['title', playlist.title],
        ['description', playlist.description],
      ]

      for (const contributor of playlist.contributors) {
        tags.push(['p', contributor])
      }

      for (const item of playlist.items) {
        tags.push(['r', JSON.stringify(item)])
      }

      const unsigned = {
        kind: 30004,
        pubkey: pubkey.value,
        created_at: Math.floor(Date.now() / 1000),
        content: '',
        tags,
      }

      const signed = await signEvent(unsigned)
      if (!signed) return false

      await publishEvent(signed)
      return true
    } catch {
      return false
    } finally {
      isPublishing.value = false
    }
  }

  function mergeFromNostrEvent(eventContent: string, eventTags: string[][]) {
    const dTag = eventTags.find((t) => t[0] === 'd')?.[1]
    const titleTag = eventTags.find((t) => t[0] === 'title')?.[1]
    if (!dTag) return

    let existing = playlists.value.find((p) => p.dTag === dTag)
    if (!existing) {
      existing = {
        id: crypto.randomUUID(),
        dTag,
        title: titleTag ?? 'Shared Playlist',
        description: eventTags.find((t) => t[0] === 'description')?.[1] ?? '',
        items: [],
        contributors: [],
        createdAt: Date.now(),
        updatedAt: Date.now(),
      }
      playlists.value.push(existing)
    }

    // Merge contributors
    for (const tag of eventTags) {
      if (tag[0] === 'p' && !existing.contributors.includes(tag[1])) {
        existing.contributors.push(tag[1])
      }
    }

    // Merge items
    for (const tag of eventTags) {
      if (tag[0] === 'r') {
        try {
          const item = JSON.parse(tag[1]) as PlaylistItem
          const exists = existing.items.some(
            (i) => i.title === item.title && i.type === item.type
          )
          if (!exists) existing.items.push(item)
        } catch { /* skip invalid */ }
      }
    }

    existing.updatedAt = Date.now()
    savePlaylists()
  }

  return {
    playlists: computed(() => playlists.value),
    isPublishing,
    createPlaylist,
    addItem,
    removeItem,
    inviteContributor,
    deletePlaylist,
    publishToNostr,
    mergeFromNostrEvent,
  }
}
