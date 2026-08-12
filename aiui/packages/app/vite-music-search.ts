import type { Plugin } from 'vite'
import type { Connect } from 'vite'
import { validateDevAuth, setCorsHeaders, checkRateLimit } from './server/dev-auth'

export interface MusicSearchResult {
  source: 'wavlake'
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

// ─── Wavlake API types ───────────────────────────────────────

interface WavlakeSearchItem {
  id: string
  title?: string
  name?: string
  type: 'track' | 'artist' | 'album'
  albumArtUrl?: string
  artistArtUrl?: string
  artistId?: string
  albumId?: string
  albumTitle?: string
  mediaUrl?: string
  artist?: string
  duration?: number
}

interface WavlakeTrack {
  id: string
  title: string
  albumTitle?: string
  artist: string
  artistId?: string
  albumId?: string
  artistArtUrl?: string
  albumArtUrl?: string
  mediaUrl: string
  duration?: number
  msatTotal?: string
  url?: string
}

// ─── Server-side LRU cache ────────────────────────────────────

const CACHE_MAX = 200
const CACHE_TTL = 60 * 60 * 1000 // 1 hour for positive results
const NULL_CACHE_TTL = 5 * 60 * 1000 // 5 minutes for not-found results
const searchCache = new Map<string, { result: MusicSearchResult | null; ts: number }>()

function getCached(key: string): MusicSearchResult | null | undefined {
  const entry = searchCache.get(key)
  if (!entry) return undefined
  const ttl = entry.result ? CACHE_TTL : NULL_CACHE_TTL
  if (Date.now() - entry.ts > ttl) {
    searchCache.delete(key)
    return undefined
  }
  return entry.result
}

function setCache(key: string, result: MusicSearchResult | null) {
  if (searchCache.size >= CACHE_MAX) {
    const oldest = searchCache.keys().next().value
    if (oldest) searchCache.delete(oldest)
  }
  searchCache.set(key, { result, ts: Date.now() })
}

// ─── Scoring ─────────────────────────────────────────────────

function scoreTrack(
  track: { title?: string; name?: string; artist?: string },
  title: string,
  artist: string,
): number {
  const t = (track.title ?? track.name ?? '').toLowerCase()
  const a = (track.artist ?? '').toLowerCase()
  const titleTerms = title.toLowerCase().split(/\s+/).filter(w => w.length > 1)
  const artistTerms = artist.toLowerCase().split(/\s+/).filter(w => w.length > 1)
  let score = 0
  for (const term of titleTerms) {
    if (t.includes(term)) score += 2
  }
  for (const term of artistTerms) {
    if (a.includes(term)) score += 2
  }
  return score
}

// ─── Wavlake search ──────────────────────────────────────────

const WAVLAKE_API = 'https://wavlake.com/api/v1'

async function wavlakeSearch(term: string): Promise<WavlakeSearchItem[]> {
  const res = await fetch(
    `${WAVLAKE_API}/content/search?term=${encodeURIComponent(term)}`,
    { signal: AbortSignal.timeout(8000) },
  )
  if (!res.ok) return []
  const items = (await res.json()) as WavlakeSearchItem[]
  return Array.isArray(items) ? items : []
}

/** Strip parenthetical suffixes, "feat.", and other noise from titles */
function cleanTitle(t: string): string {
  return t
    .replace(/\s*[\(\[].*?[\)\]]/g, '')  // (feat. X), [Remix], etc.
    .replace(/\s*[-–—]\s*(feat|ft|featuring)\.?\s*.*/i, '')
    .trim()
}

async function searchWavlake(
  q: string,
  title?: string,
  artist?: string,
): Promise<MusicSearchResult | null> {
  try {
    // Wavlake search works best with short queries — combining title+artist
    // often returns nothing. Strategy: search title first, fall back to
    // cleaned title, combined query, then artist-only.
    const searches: string[] = []
    if (title) searches.push(title)
    const cleaned = title ? cleanTitle(title) : ''
    if (cleaned && cleaned !== title) searches.push(cleaned)
    if (title && artist) searches.push(`${title} ${artist}`)
    if (artist) searches.push(artist)
    if (!title && !artist) searches.push(q)

    let allTracks: WavlakeSearchItem[] = []

    for (const term of searches) {
      const items = await wavlakeSearch(term)
      const tracks = items.filter(
        (item): item is WavlakeSearchItem & { mediaUrl: string } =>
          item.type === 'track' && !!item.mediaUrl,
      )
      if (tracks.length > 0) {
        allTracks = tracks
        break
      }
    }

    if (allTracks.length === 0) return null

    // Score and pick best match using both title and artist
    const best =
      title && artist && allTracks.length > 1
        ? allTracks.reduce((a, b) =>
            scoreTrack(b, title, artist) > scoreTrack(a, title, artist) ? b : a,
          )
        : allTracks[0]

    return {
      source: 'wavlake',
      type: 'stream',
      url: best.mediaUrl,
      title: best.title ?? best.name,
      artist: best.artist,
      coverUrl: best.albumArtUrl ?? best.artistArtUrl,
      duration: best.duration,
      trackId: best.id,
      albumTitle: best.albumTitle,
      wavlakeUrl: `https://wavlake.com/track/${best.id}`,
    }
  } catch {
    return null
  }
}

// ─── Wavlake rankings ────────────────────────────────────────

async function getWavlakeRankings(
  days: number,
  genre?: string,
  limit?: number,
): Promise<MusicSearchResult[]> {
  try {
    const params = new URLSearchParams({
      sort: 'sats',
      days: String(days),
      limit: String(limit ?? 20),
    })
    if (genre) params.set('genre', genre)

    const res = await fetch(`${WAVLAKE_API}/content/rankings?${params}`, {
      signal: AbortSignal.timeout(8000),
    })
    if (!res.ok) return []

    const tracks = (await res.json()) as WavlakeTrack[]
    if (!Array.isArray(tracks)) return []

    return tracks
      .filter(t => !!t.mediaUrl)
      .map(t => ({
        source: 'wavlake' as const,
        type: 'stream' as const,
        url: t.mediaUrl,
        title: t.title,
        artist: t.artist,
        coverUrl: t.albumArtUrl ?? t.artistArtUrl,
        duration: t.duration,
        trackId: t.id,
        albumTitle: t.albumTitle,
        wavlakeUrl: t.url ?? `https://wavlake.com/track/${t.id}`,
      }))
  } catch {
    return []
  }
}

// ─── Vite middleware: search ─────────────────────────────────

function createSearchMiddleware() {
  return async (req: Connect.IncomingMessage, res: any, next: () => void) => {
    if (req.method !== 'GET') return next()
    if (!validateDevAuth(req, res)) return
    if (!checkRateLimit(req, res)) return
    const url = new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
    const q = url.searchParams.get('q')?.trim()
    const title = url.searchParams.get('title')?.trim()
    const artist = url.searchParams.get('artist')?.trim()
    if (!q) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Missing q (query)' }))
      return
    }
    try {
      // Normalize cache key: use title|artist when available (q is ignored by searchWavlake in that case)
      const key = (title || artist) ? `search|${title ?? ''}|${artist ?? ''}` : `search|${q}||`
      const cached = getCached(key)
      if (cached !== undefined) {
        res.setHeader('Content-Type', 'application/json')
        setCorsHeaders(res)
        res.setHeader('Cache-Control', 'public, max-age=3600')
        res.setHeader('X-Cache', 'HIT')
        res.end(JSON.stringify(cached ?? { error: 'No results on Wavlake' }))
        return
      }

      console.log(`[music-search] Wavlake query: q="${q}" title="${title ?? ''}" artist="${artist ?? ''}"`)
      const result = await searchWavlake(q, title ?? undefined, artist ?? undefined)
      console.log(`[music-search] Result: ${result ? `"${result.title}" by ${result.artist}` : 'null'}`)
      setCache(key, result)

      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=3600')
      res.end(JSON.stringify(result ?? { error: 'No results on Wavlake' }))
    } catch (err) {
      console.error('[music-search]', err)
      res.writeHead(502, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: String(err) }))
    }
  }
}

// ─── Vite middleware: rankings ────────────────────────────────

function createRankingsMiddleware() {
  const rankingsCache = new Map<string, { data: MusicSearchResult[]; ts: number }>()
  const RANKINGS_TTL = 10 * 60 * 1000 // 10 minutes

  return async (req: Connect.IncomingMessage, res: any, next: () => void) => {
    if (req.method !== 'GET') return next()
    if (!validateDevAuth(req, res)) return
    if (!checkRateLimit(req, res)) return
    const url = new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
    const days = parseInt(url.searchParams.get('days') ?? '7', 10)
    const genre = url.searchParams.get('genre')?.trim() || undefined
    const limit = parseInt(url.searchParams.get('limit') ?? '20', 10)

    const cacheKey = `rankings|${days}|${genre ?? ''}|${limit}`
    const cached = rankingsCache.get(cacheKey)
    if (cached && Date.now() - cached.ts < RANKINGS_TTL) {
      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=600')
      res.setHeader('X-Cache', 'HIT')
      res.end(JSON.stringify(cached.data))
      return
    }

    try {
      const results = await getWavlakeRankings(days, genre, limit)
      rankingsCache.set(cacheKey, { data: results, ts: Date.now() })

      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=600')
      res.end(JSON.stringify(results))
    } catch (err) {
      console.error('[music-rankings]', err)
      res.writeHead(502, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: String(err) }))
    }
  }
}

// ─── Plugin ──────────────────────────────────────────────────

export function musicSearchPlugin(): Plugin {
  return {
    name: 'aiui-music-search',
    configureServer(server) {
      server.middlewares.use('/api/music/search', createSearchMiddleware())
      server.middlewares.use('/api/music/rankings', createRankingsMiddleware())
    },
    configurePreviewServer(server) {
      server.middlewares.use('/api/music/search', createSearchMiddleware())
      server.middlewares.use('/api/music/rankings', createRankingsMiddleware())
    },
  }
}
