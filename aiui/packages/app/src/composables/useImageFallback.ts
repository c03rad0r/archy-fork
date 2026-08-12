import { apiFetch } from '@/utils/api-fetch'

type TmdbResult = { posterUrl: string | null; backdropUrl: string | null }
const memoryCache = new Map<string, TmdbResult>()
const SESSION_KEY = 'aiui-poster-cache'
const failedUrls = new Set<string>()

const musicCoverCache = new Map<string, string>()
const SESSION_MUSIC_KEY = 'aiui-music-cover-cache'
const podcastCoverCache = new Map<string, string>()
const SESSION_PODCAST_KEY = 'aiui-podcast-cover-cache'

function musicCacheKey(artist: string, title: string): string {
  return `${artist.toLowerCase().trim()}|${title.toLowerCase().trim()}`
}

function podcastCacheKey(title: string, host?: string): string {
  return `${title.toLowerCase().trim()}|${(host ?? '').toLowerCase().trim()}`
}

function loadMusicCache(): void {
  try {
    const raw = sessionStorage.getItem(SESSION_MUSIC_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, string>
      Object.entries(parsed).forEach(([k, v]) => musicCoverCache.set(k, v))
    }
  } catch { /* ignore */ }
}

function saveMusicCache(): void {
  try {
    const entries = [...musicCoverCache.entries()].slice(-300)
    sessionStorage.setItem(SESSION_MUSIC_KEY, JSON.stringify(Object.fromEntries(entries)))
  } catch { /* ignore */ }
}

loadMusicCache()

function cacheKey(t: string, y?: number): string {
  return `${t.toLowerCase().trim()}|${y ?? ''}`
}

function loadSessionCache(): void {
  try {
    const raw = sessionStorage.getItem(SESSION_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, TmdbResult>
      Object.entries(parsed).forEach(([k, v]) => memoryCache.set(k, v))
    }
  } catch { /* ignore */ }
}

function saveSessionCache(): void {
  try {
    const entries = [...memoryCache.entries()].slice(-200)
    sessionStorage.setItem(SESSION_KEY, JSON.stringify(Object.fromEntries(entries)))
  } catch { /* ignore */ }
}

loadSessionCache()

function loadPodcastCache(): void {
  try {
    const raw = sessionStorage.getItem(SESSION_PODCAST_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, string>
      Object.entries(parsed).forEach(([k, v]) => podcastCoverCache.set(k, v))
    }
  } catch { /* ignore */ }
}

function savePodcastCache(): void {
  try {
    const entries = [...podcastCoverCache.entries()].slice(-200)
    sessionStorage.setItem(SESSION_PODCAST_KEY, JSON.stringify(Object.fromEntries(entries)))
  } catch { /* ignore */ }
}

loadPodcastCache()

/** Helper to build a consistent text-only SVG fallback (matches Film/TV Series style) */
function buildSquareFallback(
  label: string, title: string, subtitle: string | undefined,
  hue: number, sat: number,
): string {
  const cx = 100
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">
    <rect width="200" height="200" fill="hsl(${hue}, ${sat}%, 28%)"/>
    <rect x="3" y="3" width="194" height="194" rx="12" fill="none" stroke="hsl(${hue}, ${sat + 5}%, 38%)" stroke-width="1"/>
    <text x="${cx}" y="82" text-anchor="middle" fill="hsl(${hue}, ${sat}%, 48%)" font-family="system-ui,sans-serif" font-size="10" font-weight="400" letter-spacing="4">${label}</text>
    <line x1="60" y1="92" x2="140" y2="92" stroke="hsl(${hue}, ${sat + 5}%, 40%)" stroke-width="1"/>
    <text x="${cx}" y="118" text-anchor="middle" fill="hsl(${hue}, ${sat + 15}%, 78%)" font-family="system-ui,sans-serif" font-size="13" font-weight="700">${escapeXml(title.length > 18 ? title.slice(0, 16) + '…' : title)}</text>
    ${subtitle ? `<text x="${cx}" y="138" text-anchor="middle" fill="hsl(${hue}, ${sat}%, 60%)" font-family="system-ui,sans-serif" font-size="10" font-weight="300">${escapeXml(subtitle.length > 22 ? subtitle.slice(0, 20) + '…' : subtitle)}</text>` : ''}
  </svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

export function generatePodcastCoverFallback(title: string, host?: string): string {
  const hue = [...(title + (host ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('PODCAST', title, host, hue, 30)
}

export function generateSongCoverFallback(title: string, artist?: string): string {
  const hue = [...(title + (artist ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('MUSIC', title, artist, hue, 30)
}

export function generateNewsFallback(title: string, source?: string): string {
  const hue = [...(title + (source ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('NEWS', title, source, hue, 22)
}

export function generateImageFallback(title: string): string {
  const hue = [...title].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('IMAGE', title, undefined, hue, 20)
}

/** Website fallback — domain-based */
export function generateWebsiteFallback(title: string, domain?: string): string {
  const hue = [...(title + (domain ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('WEBSITE', title, domain, hue, 25)
}

/** Film poster fallback — cinematic sans-serif */
export function generatePosterFallback(title: string, year?: number): string {
  const hue = [...title].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 342 513">
    <rect width="342" height="513" fill="hsl(${hue}, 25%, 28%)"/>
    <rect x="3" y="3" width="336" height="507" rx="6" fill="none" stroke="hsl(${hue}, 30%, 38%)" stroke-width="1"/>
    <text x="171" y="210" text-anchor="middle" fill="hsl(${hue}, 25%, 48%)" font-family="'Helvetica Neue',Helvetica,Arial,sans-serif" font-size="13" font-weight="400" letter-spacing="5">FILM</text>
    <line x1="121" y1="225" x2="221" y2="225" stroke="hsl(${hue}, 30%, 40%)" stroke-width="1"/>
    <text x="171" y="265" text-anchor="middle" fill="hsl(${hue}, 45%, 78%)" font-family="'Helvetica Neue',Helvetica,Arial,sans-serif" font-size="18" font-weight="700">
      ${escapeXml(title.length > 20 ? title.slice(0, 18) + '…' : title)}
    </text>
    ${year ? `<text x="171" y="295" text-anchor="middle" fill="hsl(${hue}, 30%, 60%)" font-family="'Helvetica Neue',Helvetica,Arial,sans-serif" font-size="14" font-weight="300">${year}</text>` : ''}
  </svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/** TV Series poster fallback — modern grotesque */
export function generateTVSeriesFallback(title: string, year?: number): string {
  const hue = [...title].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 342 513">
    <rect width="342" height="513" fill="hsl(${hue}, 22%, 28%)"/>
    <rect x="3" y="3" width="336" height="507" rx="6" fill="none" stroke="hsl(${hue}, 28%, 38%)" stroke-width="1"/>
    <text x="171" y="210" text-anchor="middle" fill="hsl(${hue}, 22%, 48%)" font-family="'SF Pro Display',system-ui,sans-serif" font-size="13" font-weight="400" letter-spacing="5">SERIES</text>
    <line x1="111" y1="225" x2="231" y2="225" stroke="hsl(${hue}, 28%, 40%)" stroke-width="1"/>
    <text x="171" y="265" text-anchor="middle" fill="hsl(${hue}, 40%, 78%)" font-family="'SF Pro Display',system-ui,sans-serif" font-size="18" font-weight="700">
      ${escapeXml(title.length > 20 ? title.slice(0, 18) + '…' : title)}
    </text>
    ${year ? `<text x="171" y="295" text-anchor="middle" fill="hsl(${hue}, 28%, 60%)" font-family="'SF Pro Display',system-ui,sans-serif" font-size="14" font-weight="300">${year}</text>` : ''}
  </svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

function escapeXml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

async function fetchTmdbGeneric(
  endpoint: string,
  title: string,
  year?: number,
): Promise<TmdbResult> {
  const key = `${endpoint}:${cacheKey(title, year)}`
  const cached = memoryCache.get(key)
  if (cached) return cached

  const empty: TmdbResult = { posterUrl: null, backdropUrl: null }
  try {
    const params = new URLSearchParams({ q: title.trim() })
    if (year && year > 0) params.set('y', String(year))
    const res = await apiFetch(`/api/tmdb/${endpoint}?${params}`)
    if (!res.ok) return empty
    const data = (await res.json()) as { posterUrl?: string | null; backdropUrl?: string | null }
    const result: TmdbResult = {
      posterUrl: data.posterUrl ?? null,
      backdropUrl: data.backdropUrl ?? null,
    }
    if (result.posterUrl || result.backdropUrl) {
      memoryCache.set(key, result)
      saveSessionCache()
    }
    return result
  } catch {
    return empty
  }
}

export async function fetchTmdbPoster(
  title: string,
  year?: number,
): Promise<TmdbResult> {
  return fetchTmdbGeneric('search', title, year)
}

export async function handleImgError(
  e: Event,
  title: string,
  year?: number,
): Promise<void> {
  const img = e.target as HTMLImageElement
  if (img.dataset.fallback === 'done') return

  const originalSrc = img.src
  failedUrls.add(originalSrc)

  const key = cacheKey(title, year)
  const cached = memoryCache.get(key)
  if (cached?.posterUrl && cached.posterUrl !== originalSrc) {
    img.dataset.fallback = 'tmdb'
    img.src = cached.posterUrl
    return
  }

  if (img.dataset.fallback !== 'tmdb') {
    const { posterUrl } = await fetchTmdbPoster(title, year)
    if (posterUrl && posterUrl !== originalSrc) {
      img.dataset.fallback = 'tmdb'
      img.src = posterUrl
      return
    }
  }

  if (img.dataset.fallback !== 'wiki') {
    const wiki = await fetchWikipediaImage(title, 'film')
    if (wiki) {
      img.dataset.fallback = 'wiki'
      img.src = wiki
      return
    }
  }

  img.dataset.fallback = 'done'
  img.src = generatePosterFallback(title, year)
}

export function isUrlFailed(url: string | undefined): boolean {
  return !!url && failedUrls.has(url)
}

export async function fetchTmdbTVPoster(
  title: string,
  year?: number,
): Promise<TmdbResult> {
  return fetchTmdbGeneric('search-tv', title, year)
}

/** Fetch podcast artwork from iTunes Search API (free, no key). Returns hi-res URL (600x600). */
export async function fetchPodcastCover(
  title: string,
  host?: string,
): Promise<string | null> {
  const key = podcastCacheKey(title, host)
  const cached = podcastCoverCache.get(key)
  if (cached) return cached

  try {
    const term = host ? `${title} ${host}` : title
    const res = await fetch(
      `https://itunes.apple.com/search?term=${encodeURIComponent(term.trim())}&media=podcast&limit=3`,
    )
    if (!res.ok) return null
    const data = (await res.json()) as { results?: { artworkUrl100?: string }[] }
    const first = data.results?.[0]
    const url = first?.artworkUrl100
    if (!url) return null
    const hiRes = url.replace(/100x100/g, '600x600')
    podcastCoverCache.set(key, hiRes)
    savePodcastCache()
    return hiRes
  } catch {
    return null
  }
}

/** Fetch album artwork from iTunes Search API (free, no key). Returns hi-res URL (600x600). */
const bookCoverCache = new Map<string, string>()
const SESSION_BOOK_KEY = 'aiui-book-cover-cache'

function bookCacheKey(title: string, author: string): string {
  return `${title.toLowerCase().trim()}|${author.toLowerCase().trim()}`
}

function loadBookCache(): void {
  try {
    const raw = sessionStorage.getItem(SESSION_BOOK_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Record<string, string>
      Object.entries(parsed).forEach(([k, v]) => bookCoverCache.set(k, v))
    }
  } catch { /* ignore */ }
}

function saveBookCache(): void {
  try {
    const entries = [...bookCoverCache.entries()].slice(-200)
    sessionStorage.setItem(SESSION_BOOK_KEY, JSON.stringify(Object.fromEntries(entries)))
  } catch { /* ignore */ }
}

loadBookCache()

export function generateBookCoverFallback(title: string, author?: string): string {
  const hue = [...(title + (author ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 342 513">
    <rect width="342" height="513" fill="hsl(${hue}, 20%, 28%)"/>
    <rect x="3" y="3" width="336" height="507" rx="6" fill="none" stroke="hsl(${hue}, 25%, 38%)" stroke-width="1"/>
    <text x="171" y="210" text-anchor="middle" fill="hsl(${hue}, 18%, 48%)" font-family="Georgia,'Times New Roman',serif" font-size="13" font-weight="400" letter-spacing="5">BOOK</text>
    <line x1="121" y1="225" x2="221" y2="225" stroke="hsl(${hue}, 22%, 40%)" stroke-width="1"/>
    <text x="171" y="265" text-anchor="middle" fill="hsl(${hue}, 35%, 78%)" font-family="Georgia,'Times New Roman',serif" font-size="18" font-weight="700">
      ${escapeXml(title.length > 20 ? title.slice(0, 18) + '…' : title)}
    </text>
    ${author ? `<text x="171" y="295" text-anchor="middle" fill="hsl(${hue}, 22%, 60%)" font-family="Georgia,'Times New Roman',serif" font-size="14" font-weight="300">${escapeXml(author.length > 24 ? author.slice(0, 22) + '…' : author)}</text>` : ''}
  </svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/** Fetch book cover from Open Library Covers API (free, no key). */
export async function fetchBookCover(
  title: string,
  author: string,
): Promise<string | null> {
  const key = bookCacheKey(title, author)
  const cached = bookCoverCache.get(key)
  if (cached) return cached

  try {
    const q = `${title} ${author}`.trim()
    const res = await fetch(
      `https://openlibrary.org/search.json?q=${encodeURIComponent(q)}&limit=1&fields=cover_i`,
    )
    if (!res.ok) return null
    const data = (await res.json()) as { docs?: { cover_i?: number }[] }
    const coverId = data.docs?.[0]?.cover_i
    if (!coverId) return null
    const url = `https://covers.openlibrary.org/b/id/${coverId}-L.jpg`
    bookCoverCache.set(key, url)
    saveBookCache()
    return url
  } catch {
    return null
  }
}

export function generatePlaceFallback(name: string, cuisine?: string): string {
  const hue = [...(name + (cuisine ?? ''))].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('PLACE', name, cuisine, hue, 20)
}

export function generateRecipeFallback(title: string, time?: string): string {
  const hue = [...title].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return buildSquareFallback('RECIPE', title, time, hue, 30)
}

/** Place image: Wikipedia (try restaurant/place name) */
export async function fetchPlaceImage(
  name: string,
  city?: string,
): Promise<string | null> {
  // Try Wikipedia with city disambiguation first
  if (city) {
    const wiki = await fetchWikipediaImage(name, city)
    if (wiki) return wiki
  }
  // Try just the name (for well-known places/chains)
  const wiki = await fetchWikipediaImage(name, 'restaurant')
  if (wiki) return wiki
  return null
}

export async function fetchMusicCover(
  title: string,
  artist: string,
  album?: string,
): Promise<string | null> {
  const key = musicCacheKey(artist, title)
  const cached = musicCoverCache.get(key)
  if (cached) return cached

  // Try local API proxy first (works in dev with Vite middleware)
  try {
    const base = import.meta.env.BASE_URL || '/'
    const params = new URLSearchParams({ q: title, title, artist })
    const wlRes = await apiFetch(`${base}api/music/search?${params}`)
    if (wlRes.ok) {
      const wlData = (await wlRes.json()) as { coverUrl?: string }
      if (wlData.coverUrl) {
        musicCoverCache.set(key, wlData.coverUrl)
        saveMusicCache()
        return wlData.coverUrl
      }
    }
  } catch {
    // Fall through to direct Wavlake
  }

  // Try Wavlake API directly (works in production without Vite middleware)
  try {
    const term = `${title} ${artist}`.trim()
    const res = await fetch(
      `https://wavlake.com/api/v1/content/search?term=${encodeURIComponent(term)}`,
      { headers: { Accept: 'application/json' } },
    )
    if (res.ok) {
      const items = (await res.json()) as { type: string; albumArtUrl?: string; artistArtUrl?: string }[]
      if (Array.isArray(items)) {
        const track = items.find(i => i.type === 'track')
        const coverUrl = track?.albumArtUrl ?? track?.artistArtUrl
        if (coverUrl) {
          musicCoverCache.set(key, coverUrl)
          saveMusicCache()
          return coverUrl
        }
      }
    }
  } catch {
    // Fall through to iTunes
  }

  // Fallback: iTunes Search API
  try {
    const term = `${artist} ${title}`.trim().replace(/\s+/g, '+')
    const res = await fetch(
      `https://itunes.apple.com/search?term=${encodeURIComponent(term)}&media=music&limit=3`,
    )
    if (!res.ok) return null
    const data = (await res.json()) as { results?: { artworkUrl100?: string }[] }
    const first = data.results?.[0]
    const url = first?.artworkUrl100
    if (!url) return null
    const hiRes = url.replace(/100x100/g, '600x600')
    musicCoverCache.set(key, hiRes)
    saveMusicCache()
    return hiRes
  } catch {
    return null
  }
}

// ---------------------------------------------------------------------------
// Wikipedia image source (free, no key)
// ---------------------------------------------------------------------------
const wikiImageCache = new Map<string, string | null>()

/** Fetch an image from Wikipedia REST API. Free, no API key needed. */
export async function fetchWikipediaImage(
  title: string,
  disambiguator?: string,
): Promise<string | null> {
  const key = `${title.toLowerCase().trim()}|${(disambiguator ?? '').toLowerCase()}`
  if (wikiImageCache.has(key)) return wikiImageCache.get(key) ?? null

  const tryTitle = async (t: string): Promise<string | null> => {
    try {
      const encoded = encodeURIComponent(t.trim().replace(/\s+/g, '_'))
      const res = await fetch(`https://en.wikipedia.org/api/rest_v1/page/summary/${encoded}`)
      if (!res.ok) return null
      const data = (await res.json()) as {
        thumbnail?: { source?: string }
        originalimage?: { source?: string }
      }
      return data.originalimage?.source ?? data.thumbnail?.source ?? null
    } catch {
      return null
    }
  }

  let url: string | null = null

  // Try disambiguated title first (more specific), then exact title
  if (disambiguator) {
    url = await tryTitle(`${title} (${disambiguator})`)
  }
  if (!url) {
    url = await tryTitle(title)
  }

  wikiImageCache.set(key, url)
  return url
}

// ---------------------------------------------------------------------------
// Google Books image source (free, no key)
// ---------------------------------------------------------------------------

/** Fetch book cover from Google Books API. Free, no API key needed. */
export async function fetchGoogleBooksImage(
  title: string,
  author?: string,
): Promise<string | null> {
  const key = bookCacheKey(title, author ?? '')
  const gKey = `gbooks:${key}`
  if (wikiImageCache.has(gKey)) return wikiImageCache.get(gKey) ?? null

  try {
    const q = author ? `intitle:${title}+inauthor:${author}` : `intitle:${title}`
    const res = await fetch(
      `https://www.googleapis.com/books/v1/volumes?q=${encodeURIComponent(q)}&maxResults=1`,
    )
    if (!res.ok) return null
    const data = (await res.json()) as {
      items?: { volumeInfo?: { imageLinks?: { thumbnail?: string; smallThumbnail?: string } } }[]
    }
    const links = data.items?.[0]?.volumeInfo?.imageLinks
    let url = links?.thumbnail ?? links?.smallThumbnail ?? null
    // Google Books returns http URLs and small sizes — upgrade
    if (url) {
      url = url.replace(/^http:/, 'https:').replace(/&edge=curl/g, '')
      // Request larger zoom
      if (!url.includes('zoom=')) url += '&zoom=2'
    }
    wikiImageCache.set(gKey, url)
    return url
  } catch {
    return null
  }
}

// ---------------------------------------------------------------------------
// Chained fetchers — try multiple sources in order
// ---------------------------------------------------------------------------

/** Film image: TMDB → Wikipedia */
export async function fetchFilmImage(
  title: string,
  year?: number,
): Promise<{ posterUrl: string | null; backdropUrl: string | null }> {
  // Try TMDB first
  const tmdb = await fetchTmdbPoster(title, year)
  if (tmdb.posterUrl || tmdb.backdropUrl) return tmdb

  // Fall back to Wikipedia
  const wiki = await fetchWikipediaImage(title, 'film')
  if (wiki) return { posterUrl: wiki, backdropUrl: null }

  return { posterUrl: null, backdropUrl: null }
}

/** TV series image: TMDB → Wikipedia */
export async function fetchTVImage(
  title: string,
  year?: number,
): Promise<{ posterUrl: string | null; backdropUrl: string | null }> {
  const tmdb = await fetchTmdbTVPoster(title, year)
  if (tmdb.posterUrl || tmdb.backdropUrl) return tmdb

  const wiki = await fetchWikipediaImage(title, 'TV series')
  if (wiki) return { posterUrl: wiki, backdropUrl: null }

  return { posterUrl: null, backdropUrl: null }
}

/** Book image: Open Library → Google Books → Wikipedia */
export async function fetchBookImage(
  title: string,
  author?: string,
): Promise<string | null> {
  // Try Open Library first
  const ol = await fetchBookCover(title, author ?? '')
  if (ol) return ol

  // Try Google Books
  const gb = await fetchGoogleBooksImage(title, author)
  if (gb) return gb

  // Try Wikipedia
  const wiki = await fetchWikipediaImage(title, 'novel')
  return wiki
}
