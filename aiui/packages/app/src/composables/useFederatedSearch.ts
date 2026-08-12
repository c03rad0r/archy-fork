import { ref, watch } from 'vue'
import { mockFilms } from '@/mocks/films'
import { mockSongs } from '@/mocks/songs'
import { mockPodcasts } from '@/mocks/podcasts'
import type { Film, Song, Podcast } from '@aiui/core/types/content'

// Demo-site content pack (operator decision 2026-08-07): demo/dev only.
const demoFilms: Film[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockFilms : []
const demoSongs: Song[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockSongs : []
const demoPodcasts: Podcast[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockPodcasts : []

export interface SearchResult {
  type: 'film' | 'song' | 'podcast'
  title: string
  subtitle: string
  id: string
  data: unknown
}

const query = ref('')
const results = ref<SearchResult[]>([])
const isSearching = ref(false)

let debounceTimer: ReturnType<typeof setTimeout> | null = null

function searchLibrary(q: string): SearchResult[] {
  const lower = q.toLowerCase()
  const matched: SearchResult[] = []

  for (const film of demoFilms) {
    if (
      film.title.toLowerCase().includes(lower) ||
      film.director.toLowerCase().includes(lower) ||
      film.genres.some(g => g.toLowerCase().includes(lower))
    ) {
      matched.push({
        type: 'film',
        title: film.title,
        subtitle: `${film.year} · ${film.director}`,
        id: film.id,
        data: film,
      })
    }
  }

  for (const song of demoSongs) {
    if (
      song.title.toLowerCase().includes(lower) ||
      song.artist.toLowerCase().includes(lower) ||
      (song.album ?? '').toLowerCase().includes(lower)
    ) {
      matched.push({
        type: 'song',
        title: song.title,
        subtitle: song.artist,
        id: song.id,
        data: song,
      })
    }
  }

  for (const podcast of demoPodcasts) {
    if (
      podcast.title.toLowerCase().includes(lower) ||
      (podcast.host ?? '').toLowerCase().includes(lower)
    ) {
      matched.push({
        type: 'podcast',
        title: podcast.title,
        subtitle: podcast.host ?? 'Unknown',
        id: podcast.id,
        data: podcast,
      })
    }
  }

  return matched.slice(0, 20)
}

export function useFederatedSearch() {
  function search(q: string) {
    query.value = q
  }

  function clear() {
    query.value = ''
    results.value = []
    isSearching.value = false
  }

  watch(query, (q) => {
    if (debounceTimer) clearTimeout(debounceTimer)
    if (!q.trim()) {
      results.value = []
      isSearching.value = false
      return
    }

    isSearching.value = true
    debounceTimer = setTimeout(() => {
      results.value = searchLibrary(q.trim())
      isSearching.value = false
    }, 150)
  })

  return {
    query,
    results,
    isSearching,
    search,
    clear,
  }
}
