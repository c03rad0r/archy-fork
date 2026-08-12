import { ref, computed } from 'vue'

export interface ContentPackItem {
  type: string
  title: string
  data: Record<string, unknown>
}

export interface ContentPack {
  id: string
  name: string
  description: string
  version: string
  author: string
  items: ContentPackItem[]
  installedAt?: number
  source?: string
}

const STORAGE_KEY = 'aiui-content-packs'

const installedPacks = ref<ContentPack[]>([])

function loadPacks() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) installedPacks.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function savePacks() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(installedPacks.value))
}

loadPacks()

// Built-in registry of available packs
const registryPacks: Omit<ContentPack, 'installedAt'>[] = [
  {
    id: 'pack-best-films-2024',
    name: '2024 Best Films',
    description: 'Curated selection of the best films released in 2024.',
    version: '1.0.0',
    author: 'AIUI Community',
    source: 'builtin',
    items: [
      { type: 'film', title: 'Dune: Part Two', data: { year: 2024, director: 'Denis Villeneuve', rating: 8.3 } },
      { type: 'film', title: 'The Substance', data: { year: 2024, director: 'Coralie Fargeat', rating: 7.5 } },
      { type: 'film', title: 'Conclave', data: { year: 2024, director: 'Edward Berger', rating: 7.8 } },
      { type: 'film', title: 'Anora', data: { year: 2024, director: 'Sean Baker', rating: 7.9 } },
      { type: 'film', title: 'The Brutalist', data: { year: 2024, director: 'Brady Corbet', rating: 7.6 } },
    ],
  },
  {
    id: 'pack-bitcoin-music',
    name: 'Bitcoin Music Playlist',
    description: 'Songs celebrating Bitcoin, sound money, and sovereignty.',
    version: '1.0.0',
    author: 'AIUI Community',
    source: 'builtin',
    items: [
      { type: 'song', title: 'Value 4 Value', data: { artist: 'Ainsley Costello', year: 2023 } },
      { type: 'song', title: 'Bitcoin Thunder', data: { artist: 'Mandrik', year: 2022 } },
      { type: 'song', title: 'Bitcoin is Dead', data: { artist: 'HODL Band', year: 2024 } },
      { type: 'song', title: 'Sound Money', data: { artist: 'Cory Klippsten', year: 2023 } },
      { type: 'song', title: 'Stack Sats', data: { artist: 'Pleb Music', year: 2024 } },
    ],
  },
  {
    id: 'pack-essential-nostr',
    name: 'Essential Nostr Reads',
    description: 'Must-read resources for understanding and building on Nostr.',
    version: '1.0.0',
    author: 'AIUI Community',
    source: 'builtin',
    items: [
      { type: 'book', title: 'The Nostr Protocol', data: { author: 'fiatjaf', year: 2023, category: 'Protocol Spec' } },
      { type: 'book', title: 'NIP-01: Basic Protocol', data: { author: 'fiatjaf', year: 2022, category: 'NIP' } },
      { type: 'book', title: 'Decentralized Social Media', data: { author: 'Various', year: 2023, category: 'Guide' } },
      { type: 'book', title: 'Building Censorship-Resistant Apps', data: { author: 'Nostr Community', year: 2024, category: 'Tutorial' } },
    ],
  },
]

export function useContentPacks() {
  const availablePacks = computed(() => {
    const installedIds = new Set(installedPacks.value.map((p) => p.id))
    return registryPacks.filter((p) => !installedIds.has(p.id))
  })

  function installPack(packId: string): boolean {
    const pack = registryPacks.find((p) => p.id === packId)
    if (!pack) return false

    const installed: ContentPack = {
      ...pack,
      installedAt: Date.now(),
    }
    installedPacks.value.push(installed)
    savePacks()
    return true
  }

  function uninstallPack(packId: string) {
    installedPacks.value = installedPacks.value.filter((p) => p.id !== packId)
    savePacks()
  }

  async function importFromUrl(url: string): Promise<ContentPack | null> {
    try {
      const parsed = new URL(url)
      if (parsed.protocol !== 'https:') return null

      const response = await fetch(url)
      if (!response.ok) return null
      const data = (await response.json()) as ContentPack
      if (
        !data.id || typeof data.id !== 'string' ||
        !data.name || typeof data.name !== 'string' ||
        !Array.isArray(data.items) ||
        !data.items.every((item: unknown) => {
          const i = item as Record<string, unknown>
          return i && typeof i.type === 'string' && typeof i.title === 'string'
        })
      ) return null

      const existing = installedPacks.value.find((p) => p.id === data.id)
      if (existing) {
        Object.assign(existing, data, { installedAt: Date.now(), source: url })
      } else {
        installedPacks.value.push({ ...data, installedAt: Date.now(), source: url })
      }
      savePacks()
      return data
    } catch {
      return null
    }
  }

  function importFromJson(json: string): ContentPack | null {
    try {
      const data = JSON.parse(json) as ContentPack
      if (!data.id || !data.name || !data.items) return null

      const existing = installedPacks.value.find((p) => p.id === data.id)
      if (existing) {
        Object.assign(existing, data, { installedAt: Date.now() })
      } else {
        installedPacks.value.push({ ...data, installedAt: Date.now() })
      }
      savePacks()
      return data
    } catch {
      return null
    }
  }

  function getPackItems(packId: string): ContentPackItem[] {
    const pack = installedPacks.value.find((p) => p.id === packId)
    return pack?.items ?? []
  }

  function getAllInstalledItems(type?: string): ContentPackItem[] {
    const all = installedPacks.value.flatMap((p) => p.items)
    return type ? all.filter((i) => i.type === type) : all
  }

  return {
    installedPacks: computed(() => installedPacks.value),
    availablePacks,
    registryPacks,
    installPack,
    uninstallPack,
    importFromUrl,
    importFromJson,
    getPackItems,
    getAllInstalledItems,
  }
}
