import { ref, shallowRef, computed, watch } from 'vue'
import type { Song } from '@aiui/core/types/content'
import Plyr from 'plyr'
import 'plyr/dist/plyr.css'
import { apiFetch } from '@/utils/api-fetch'

interface MusicSearchResult {
  source: 'wavlake' | 'node'
  type: 'stream'
  url: string
  title?: string
  artist?: string
  coverUrl?: string
  duration?: number
  trackId?: string
  albumTitle?: string
  wavlakeUrl?: string
}

// ─── Global singleton state ───────────────────────────────────

const currentSong = ref<Song | null>(null)
const playableSource = ref<MusicSearchResult | null>(null)
const isPlaying = ref(false)
const isLoading = ref(false)
const error = ref<string | null>(null)
const currentTime = ref(0)
const duration = ref(0)

// Queue management
const queue = shallowRef<Song[]>([])
const currentIndex = ref(-1)

let plyrInstance: Plyr | null = null
let containerEl: HTMLDivElement | null = null
let audioEl: HTMLAudioElement | null = null

// Client-side search result cache — avoids re-searching songs
// Null results use a short TTL so transient failures don't stick
const NULL_CACHE_TTL = 2 * 60 * 1000 // 2 minutes
const resultCache = new Map<string, MusicSearchResult | null>()
const nullCacheTimestamps = new Map<string, number>()

// Active search abort controller — cancel stale searches on rapid switching
let activeSearchController: AbortController | null = null

export function usePlayer() {
  const hasTrack = computed(() => !!currentSong.value)
  const progress = computed(() => {
    if (duration.value <= 0) return 0
    return (currentTime.value / duration.value) * 100
  })

  // ─── Search with abort + cache ────────────────────────────

  async function searchWavlakeDirect(
    query: string,
    title?: string,
    artist?: string,
    signal?: AbortSignal,
  ): Promise<MusicSearchResult | null> {
    const searches: string[] = []
    if (title) searches.push(title)
    if (title && artist) searches.push(`${title} ${artist}`)
    if (artist) searches.push(artist)
    if (!title && !artist) searches.push(query)

    for (const term of searches) {
      try {
        const res = await fetch(
          `https://wavlake.com/api/v1/content/search?term=${encodeURIComponent(term)}`,
          { signal, headers: { Accept: 'application/json' } },
        )
        if (!res.ok) continue
        const items = (await res.json()) as {
          id: string; title?: string; name?: string; type: string
          mediaUrl?: string; artist?: string; albumArtUrl?: string
          artistArtUrl?: string; duration?: number; albumTitle?: string
        }[]
        if (!Array.isArray(items)) continue
        const tracks = items.filter(i => i.type === 'track' && !!i.mediaUrl)
        if (tracks.length === 0) continue
        const best = tracks[0]
        return {
          source: 'wavlake',
          type: 'stream',
          url: best.mediaUrl!,
          title: best.title ?? best.name,
          artist: best.artist,
          coverUrl: best.albumArtUrl ?? best.artistArtUrl,
          duration: best.duration,
          trackId: best.id,
          albumTitle: best.albumTitle,
          wavlakeUrl: `https://wavlake.com/track/${best.id}`,
        }
      } catch (e) {
        if ((e as Error).name === 'AbortError') return null
      }
    }
    return null
  }

  async function searchMusic(query: string, title?: string, artist?: string): Promise<MusicSearchResult | null> {
    const cacheKey = `${title ?? query}|${artist ?? ''}`
    const cached = resultCache.get(cacheKey)
    if (cached !== undefined) {
      // Positive results stay cached forever; null results expire after TTL
      if (cached !== null) return cached
      const ts = nullCacheTimestamps.get(cacheKey)
      if (ts && Date.now() - ts < NULL_CACHE_TTL) return null
      // Expired null — retry
      resultCache.delete(cacheKey)
      nullCacheTimestamps.delete(cacheKey)
    }

    // Cancel any in-flight search
    activeSearchController?.abort()
    const controller = new AbortController()
    activeSearchController = controller

    // Try local API proxy first (works in dev), then Wavlake directly (works in prod)
    let result: MusicSearchResult | null = null
    try {
      const params = new URLSearchParams({ q: query })
      if (title) params.set('title', title)
      if (artist) params.set('artist', artist)
      const base = import.meta.env.BASE_URL || '/'
      const res = await apiFetch(`${base}api/music/search?${params}`, {
        signal: controller.signal,
      })
      if (res.ok) {
        const data = (await res.json()) as MusicSearchResult & { error?: string }
        if (!data.error && data.url) {
          result = data as MusicSearchResult
        }
      }
    } catch (e) {
      if ((e as Error).name === 'AbortError') return null
    }

    // Fallback: call Wavlake API directly
    if (!result) {
      result = await searchWavlakeDirect(query, title, artist, controller.signal)
    }

    if (!result) {
      error.value = 'Not found on Wavlake'
      resultCache.set(cacheKey, null)
      nullCacheTimestamps.set(cacheKey, Date.now())
      return null
    }

    resultCache.set(cacheKey, result)
    return result
  }

  // ─── Container management ─────────────────────────────────

  function setContainer(el: HTMLDivElement | null) {
    containerEl = el
    if (el && playableSource.value) {
      initPlayer(playableSource.value)
    }
  }

  // ─── Audio player init — reuses audio element when possible ─

  function initPlayer(result: MusicSearchResult) {
    if (!containerEl) return

    // Reuse existing audio element if we have one — just change src
    if (audioEl && plyrInstance) {
      audioEl.src = result.url
      audioEl.load()
      Promise.resolve(plyrInstance.play()).catch(() => { /* autoplay blocked */ })
      return
    }

    // First time — create audio element and Plyr
    destroyPlayer()
    audioEl = document.createElement('audio')
    audioEl.src = result.url
    audioEl.preload = 'auto'
    containerEl.textContent = ''
    containerEl.appendChild(audioEl)
    plyrInstance = new Plyr(audioEl, {
      controls: [],
      autoplay: true,
      muted: false,
    })

    plyrInstance.on('ready', () => {
      Promise.resolve(plyrInstance!.play()).catch(() => { /* autoplay blocked */ })
    })
    plyrInstance.on('timeupdate', () => {
      currentTime.value = plyrInstance!.currentTime ?? 0
    })
    plyrInstance.on('loadedmetadata', () => {
      duration.value = plyrInstance!.duration ?? 0
    })
    plyrInstance.on('ended', () => {
      isPlaying.value = false
      if (currentIndex.value >= 0 && currentIndex.value < queue.value.length - 1) {
        playNext()
      }
    })
    plyrInstance.on('playing', () => {
      isPlaying.value = true
      isLoading.value = false
    })
    plyrInstance.on('pause', () => {
      isPlaying.value = false
    })
    plyrInstance.on('error', (event: Plyr.PlyrEvent) => {
      const mediaError = audioEl?.error
      console.error('[player] Audio error:', mediaError?.code, mediaError?.message)
      isLoading.value = false
      error.value = 'Audio failed to load'
    })
  }

  function destroyPlayer() {
    if (plyrInstance) {
      plyrInstance.destroy()
      plyrInstance = null
    }
    audioEl = null
    if (containerEl) {
      containerEl.textContent = ''
    }
  }

  // ─── Play — instant feedback, then load ───────────────────

  async function play(song: Song) {
    error.value = null

    // Resume if same song
    if (currentSong.value?.id === song.id && playableSource.value) {
      if (plyrInstance) {
        Promise.resolve(plyrInstance.play()).catch(() => {})
        isPlaying.value = true
      }
      return
    }

    // Set song immediately for instant UI feedback
    currentSong.value = song
    isLoading.value = true
    currentTime.value = 0
    duration.value = 0

    // A real node track carries its own sources (same-origin `/content/<id>`,
    // Range-streamed by the node itself) — play those FIRST. Wavlake is the
    // metadata-only fallback; on a node it is CSP-blocked outright, so every
    // library track used to end at "Not found on Wavlake" without ever
    // trying the bytes sitting on the operator's own disk.
    const nodeSource = song.sources?.find(
      (s) => !!s.url && (s.url.startsWith('/') || s.url.startsWith(window.location.origin)),
    )
    if (nodeSource) {
      isLoading.value = false
      error.value = null
      playableSource.value = {
        source: 'node',
        type: 'stream',
        url: nodeSource.url,
        title: song.title,
        artist: song.artist,
        coverUrl: song.coverUrl,
        duration: song.duration,
      }
      if (containerEl) {
        initPlayer(playableSource.value)
      }
      return
    }

    const query = `${song.title} ${song.artist}`.trim()
    const result = await searchMusic(query, song.title, song.artist)

    // Guard: user may have switched to a different song while we were searching
    if (currentSong.value?.id !== song.id) return

    isLoading.value = false
    if (!result) {
      if (!error.value) error.value = 'Not found on Wavlake'
      return
    }

    // Enrich song with Wavlake metadata if available
    if (result.coverUrl && currentSong.value && !currentSong.value.coverUrl) {
      currentSong.value = { ...currentSong.value, coverUrl: result.coverUrl }
    }
    if (result.duration && currentSong.value && !currentSong.value.duration) {
      currentSong.value = { ...currentSong.value, duration: result.duration }
    }
    error.value = null
    playableSource.value = result
    if (containerEl) {
      initPlayer(result)
    }
  }

  function pause() {
    plyrInstance?.pause()
    isPlaying.value = false
  }

  function toggle() {
    if (isLoading.value) return
    if (plyrInstance) {
      if (isPlaying.value) pause()
      else Promise.resolve(plyrInstance.play()).catch(() => {})
    }
  }

  function seek(percent: number) {
    if (!plyrInstance || !duration.value) return
    const time = (percent / 100) * duration.value
    plyrInstance.currentTime = time
    currentTime.value = time
  }

  // ─── Queue management ─────────────────────────────────────

  function addToQueue(song: Song) {
    const existing = queue.value.find(s => s.id === song.id)
    if (!existing) {
      queue.value = [...queue.value, song]
    }
  }

  function playNext() {
    if (queue.value.length === 0) return
    const nextIdx = currentIndex.value + 1
    if (nextIdx < queue.value.length) {
      currentIndex.value = nextIdx
      play(queue.value[nextIdx])
    }
  }

  function playPrevious() {
    if (queue.value.length === 0) return
    const prevIdx = currentIndex.value - 1
    if (prevIdx >= 0) {
      currentIndex.value = prevIdx
      play(queue.value[prevIdx])
    }
  }

  function removeFromQueue(index: number) {
    const newQueue = [...queue.value]
    newQueue.splice(index, 1)
    queue.value = newQueue
    if (index < currentIndex.value) {
      currentIndex.value--
    } else if (index === currentIndex.value) {
      currentIndex.value = Math.min(currentIndex.value, newQueue.length - 1)
    }
  }

  async function playWithQueue(song: Song) {
    const idx = queue.value.findIndex(s => s.id === song.id)
    if (idx === -1) {
      addToQueue(song)
      currentIndex.value = queue.value.length - 1
    } else {
      currentIndex.value = idx
    }
    await play(song)
  }

  // ─── Prefetch next song in queue ──────────────────────────

  watch(currentIndex, (idx) => {
    const nextIdx = idx + 1
    if (nextIdx < queue.value.length) {
      const next = queue.value[nextIdx]
      const query = `${next.title} ${next.artist}`.trim()
      // Fire-and-forget — populates the cache
      searchMusic(query, next.title, next.artist)
    }
  })

  // ─── Cleanup ──────────────────────────────────────────────

  function clear() {
    pause()
    destroyPlayer()
    currentSong.value = null
    playableSource.value = null
    currentTime.value = 0
    duration.value = 0
    error.value = null
  }

  function clearQueue() {
    clear()
    queue.value = []
    currentIndex.value = -1
  }

  const hasNext = computed(() => currentIndex.value < queue.value.length - 1)
  const hasPrevious = computed(() => currentIndex.value > 0)

  return {
    currentSong,
    playableSource,
    isPlaying,
    isLoading,
    error,
    currentTime,
    duration,
    hasTrack,
    progress,
    queue,
    currentIndex,
    hasNext,
    hasPrevious,
    play: playWithQueue,
    pause,
    toggle,
    seek,
    clear,
    clearQueue,
    addToQueue,
    playNext,
    playPrevious,
    removeFromQueue,
    setContainer,
  }
}
