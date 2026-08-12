import type { Film, Song, Podcast, Book, TVSeries, ImageItem, Place } from '@aiui/core/types/content'
import type { WebSearchResult } from '@aiui/core/types/message'
import type { MagazineSection } from './contentFiltering'
import { mockFilms } from '@/mocks/films'
import { mockSongs } from '@/mocks/songs'
import { mockPodcasts } from '@/mocks/podcasts'

// Demo-site content pack (operator decision 2026-08-07): [[film:ID]]-style
// library references resolve against the mock libraries in demo/dev builds
// only. In production the tables constant-fold to empty — no mock module
// reaches the shipped bundle — and ext tags (self-contained) keep working.
const libraryFilms: Film[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockFilms : []
const librarySongs: Song[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockSongs : []
const libraryPodcasts: Podcast[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockPodcasts : []
import { isNewsLikeResponse, isMusicQuery, isTVQuery, isBookQuery, isBookLikeResponse, isImageQuery, isPlaceQuery, isPlaceLikeResponse } from './contentFiltering'

// ─── Tag regexes ──────────────────────────────────────────────────

export const FILM_TAG_RE = /\[\[film:(f?\d+)\]\]/gi
export const FILM_EXT_RE = /\[\[film_ext:([^|]+)\|(\d{4})\|([^\]]+)\]\]/gi
export const SONG_TAG_RE = /\[\[song:(s?\d+)\]\]/gi
export const SONG_EXT_RE = /\[\[song_ext:([^|]+)\|([^|]+)(?:\|(\d{4}))?\]\]/gi
export const PODCAST_TAG_RE = /\[\[podcast:(p?\d+)\]\]/gi
export const PODCAST_EXT_RE = /\[\[podcast_ext:([^|]+)\|([^|]+)(?:\|(\d{4}))?\]\]/gi
export const BOOK_TAG_RE = /\[\[book:(b?\d+)\]\]/gi
export const BOOK_EXT_RE = /\[\[book_ext:([^|]+)\|([^|]+)(?:\|(\d{4}))?\]\]/gi
export const TV_EXT_RE = /\[\[tv_ext:([^|]+)\|([^|\]]+)(?:\|([^|\]]+))?\]\]/gi
export const PLACE_EXT_RE = /\[\[place_ext:([^|]+)\|([^|]*)(?:\|([^|]*))?(?:\|([^|]*))?(?:\|([^|]*))?(?:\|([^|]*))?\]\]/gi
export const APP_EXT_RE = /\[\[app_ext:([^|]+)\|([^|]*)(?:\|([^|]*))?(?:\|([^|]*))?\]\]/gi
export const MARKDOWN_LINK_RE = /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g
const SAFE_URL_SCHEME = /^https?:\/\//i

// ─── Utility helpers ──────────────────────────────────────────────

function extractUrlFromText(text: string): string | undefined {
  const mdLink = /\[([^\]]*)\]\((https?:\/\/[^)]+)\)/.exec(text)
  const raw = mdLink ? mdLink[2] : (/(https?:\/\/[^\s)\]"'<>]+)/.exec(text)?.[1])
  if (!raw?.trim()) return undefined
  const trimmed = raw.trim()
  if (trimmed.length > 2048) return undefined
  try {
    const u = new URL(trimmed)
    if (!/^https?:$/i.test(u.protocol)) return undefined
    return u.href
  } catch {
    return undefined
  }
}

function extractAuthorFromText(text: string): string | undefined {
  const patterns = [
    /(?:analyst|according to)\s+\*{0,2}([A-Z][^*\n]+?)\*{0,2}(?:\s+(?:is|calls?|says?|cited)|\.|,)/,
    /\bby\s+\*{0,2}([A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)+)\*{0,2}/,
    /(?:source|—)\s*:?\s*\*{0,2}([A-Z][^*\n]+?)\*{0,2}(?:\s|$|\.|,)/,
    /\*\*([A-Z][^*]+)\*\*(?:\s+(?:is|calls?|says?|cited|predicts?))/,
  ]
  for (const re of patterns) {
    const m = re.exec(text)
    if (m) {
      const name = m[1].trim().slice(0, 60)
      if (name.length > 3 && name.length < 50) return name
    }
  }
  return undefined
}

function extractFirstImageFromText(text: string): string | undefined {
  const mdImg = /!\[[^\]]*\]\((https?:\/\/[^)]+)\)/.exec(text)
  const raw = mdImg ? mdImg[1] : (/(https?:\/\/[^\s)\]"'<>]+\.(?:jpg|jpeg|png|gif|webp)(?:\?[^\s)\]]*)?)/i.exec(text)?.[1])
  if (!raw?.trim()) return undefined
  try {
    const u = new URL(raw.trim())
    if (!/^https?:$/i.test(u.protocol)) return undefined
    return u.href
  } catch {
    return undefined
  }
}

function extractDescriptionForTag(text: string, matchIndex: number, matchLength: number): string {
  // Several patterns anchor with `(?:^|\n)` so they only fire at a line start.
  // That makes `m.index` point at the NEWLINE, one character before the line
  // the match is actually on — and the line window below then walks back from
  // `matchIndex - 1` and lands on the PREVIOUS line. The description became
  // "previous line + this item's own text".
  //
  // Operator-visible result: "Banking on Bitcoin" captioned with *The Rise and
  // Rise of Bitcoin*'s description, and the section header "Documentaries:"
  // bleeding into the first card of each group. The model's prose was correct
  // throughout; only this pairing was wrong.
  //
  // Normalised here rather than at each of the nine call sites, so a pattern
  // that gains a `(?:^|\n)` anchor later cannot silently reintroduce it.
  while (matchLength > 0 && (text[matchIndex] === '\n' || text[matchIndex] === '\r')) {
    matchIndex += 1
    matchLength -= 1
  }

  const prevNewline = text.lastIndexOf('\n', matchIndex - 1)
  const lineStart = prevNewline === -1 ? 0 : prevNewline + 1
  const nextNewline = text.indexOf('\n', matchIndex + matchLength)
  const lineEnd = nextNewline === -1 ? text.length : nextNewline

  let line = text.slice(lineStart, lineEnd)
  line = line.replace(text.slice(matchIndex, matchIndex + matchLength), '')
  line = line.replace(/\[\[(?:film|song|podcast)(?:_ext)?:[^\]]*\]\]/g, '')
  line = line.replace(/^\s*[-*•]\s*/, '').replace(/^\s*\d+\.\s*/, '')
  line = line.replace(/\*\*[^*]+\*\*\s*[-–—:]\s*/, '').replace(/\*\*[^*]+\*\*\s*/, '')
  line = line.replace(/\*\*/g, '').replace(/\*/g, '')
  line = line.replace(/\(\d{4}\)\s*/g, '')
  line = line.replace(/^[\s\-–—:,]+/, '').replace(/[\s\-–—:,]+$/, '')

  const result = line.trim().slice(0, 300)
  return result.length >= 10 ? result : ''
}

export function normUrl(u: string): string {
  return u.toLowerCase().trim().replace(/\/$/, '')
}

// ─── Magazine extraction ──────────────────────────────────────────

const MAGAZINE_CONTENT_MAX = 2000

/** Clean markdown artifacts from magazine text */
function cleanMagazineText(text: string): string {
  return text
    .replace(/\[([^\]]*)\]\([^)]+\)/g, '$1')
    .replace(/https?:\/\/\S+/g, '')
    .replace(/\uFE0F/g, '')
    .replace(/(?:^|(?<=\s))[\p{Emoji_Presentation}\p{Extended_Pictographic}]+\s*/gu, '') // standalone emojis only
    .replace(/\*\*/g, '')
    .replace(/\*([^*\n]+)\*/g, '$1')          // *italic* → italic
    .replace(/---+/g, '')
    .replace(/^#+\s*/gm, '')
    .replace(/\|/g, ', ')                      // pipes → comma-space
    .replace(/,\s*,+/g, ',')                   // collapse multiple commas
    .replace(/(^|\n)\s*,\s*/g, '$1')           // trim leading commas per line
    .replace(/\s*,\s*($|\n)/g, '$1')           // trim trailing commas per line
}

function addSection(
  sections: MagazineSection[],
  title: string,
  content: string,
  seen: Set<string>,
  group?: string,
): void {
  const t = cleanMagazineText(title).replace(/\s+/g, ' ').trim().slice(0, 150)
  let c = cleanMagazineText(content).replace(/\n{3,}/g, '\n\n').trim().slice(0, MAGAZINE_CONTENT_MAX)
  if (t.length < 2 || c.length < 15) return
  const tLower = t.toLowerCase()
  const cLower = c.toLowerCase()
  if (tLower.length >= 10 && cLower.startsWith(tLower.slice(0, Math.min(tLower.length, 40)))) {
    c = c.slice(t.length).replace(/^[\s.,:;—–-]+/, '').trim()
    if (c.length < 15) return
  }
  const key = `${t.slice(0, 50)}`
  if (seen.has(key)) return
  seen.add(key)
  const imgMatch = /!\[[^\]]*\]\((https?:\/\/[^)]+)\)/.exec(content)?.[1]
  const imageUrl = imgMatch ? (() => {
    try {
      const u = new URL(imgMatch.trim())
      return /^https?:$/i.test(u.protocol) ? u.href : undefined
    } catch { return undefined }
  })() : undefined
  sections.push({
    title: t,
    content: c,
    url: extractUrlFromText(content),
    author: extractAuthorFromText(content),
    imageUrl,
    group,
  })
}

function cleanMagazineContent(raw: string): string {
  return raw
    .replace(/\[\[(?:podcast|film|song|book|tvshow|podcast_ext|film_ext|song_ext|book_ext|tv_ext|place_ext|recipe_ext|event_ext):[^\]]*\]\]/g, '')
    .replace(/^\s*\n---\s*\n?/g, '')
    .replace(/\n---\s*$/g, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function extractMagazineSections(text: string): MagazineSection[] {
  const sections: MagazineSection[] = []
  const seen = new Set<string>()

  const cleanedText = text
    .replace(/\n+(?:Sources|References|Links):?\s*\n[\s\S]*$/i, '')
    .replace(/\n+\*{0,2}For deeper[^:]*:[\s\S]*$/im, '')

  const headingPositions: { title: string; start: number; contentStart: number }[] = []
  const headingLineRe = /^#{2,3}\s+(.+)$/gm
  let m: RegExpExecArray | null
  while ((m = headingLineRe.exec(cleanedText)) !== null) {
    const rawTitle = m[1].trim()
    const title = rawTitle
      .replace(/[\p{Emoji_Presentation}\p{Extended_Pictographic}]\uFE0F?\s*/gu, '')
      .replace(/^[#*_\s-]+/, '')
      .trim()
    if (title.length > 1) {
      headingPositions.push({ title, start: m.index, contentStart: m.index + m[0].length })
    }
  }

  for (let i = 0; i < headingPositions.length; i++) {
    const hp = headingPositions[i]
    const nextStart = i + 1 < headingPositions.length ? headingPositions[i + 1].start : cleanedText.length
    const rawContent = cleanMagazineContent(cleanedText.slice(hp.contentStart, nextStart))
    if (rawContent.length < 20) continue

    const bullets = rawContent
      .split(/\n\s*[-•]\s+/)
      .map(s => s.replace(/^[-•]\s*/, '').trim())
      .filter(s => s.length > 10)

    if (bullets.length >= 2) {
      for (const bullet of bullets) {
        const cleaned = cleanMagazineContent(bullet)
        if (cleaned.length < 15) continue
        const boldMatch = /^\*\*([^*]+)\*\*\s*[:\u2014\u2013–]\s*(.+)/s.exec(cleaned)
        if (boldMatch) {
          addSection(sections, boldMatch[1].trim(), boldMatch[2].trim(), seen, hp.title)
        } else {
          const sentenceMatch = /^([^.!?]{10,80}[.!?])/.exec(cleaned)
          const bulletTitle = sentenceMatch ? sentenceMatch[1] : cleaned.slice(0, 60)
          addSection(sections, bulletTitle, cleaned, seen, hp.title)
        }
      }
    } else {
      addSection(sections, hp.title, rawContent, seen)
    }
  }

  const campRe = /\*\*([^*]+(?:camp| side| view)[^*]*)\*\*[🟠🔴🟢]?\s*\n([\s\S]+?)(?=\n\*\*[^*]+(?:camp| side)[^*]*\*\*|\n#{2,3}\s|\n\nThis is|\n\nFor deeper|$)/gimu
  while ((m = campRe.exec(cleanedText)) !== null) {
    const title = m[1].trim()
    const content = cleanMagazineContent(m[2])
    if (content.length > 15) addSection(sections, title, content, seen)
  }

  const firstHeadingIdx = headingPositions.length > 0 ? headingPositions[0].start : -1
  const introEnd = firstHeadingIdx > 0 ? firstHeadingIdx : cleanedText.indexOf('\n---\n')
  if (introEnd > 0) {
    const intro = cleanedText.slice(0, introEnd).trim()
      .replace(/^[#*_\s-]+/gm, '').trim()
    if (intro.length > 30 && !seen.has('Summary')) {
      addSection(sections, 'Summary', intro, seen)
    }
  }

  // ── Fallback: numbered/bullet lists with bold titles ──────────────
  // Handles responses like: "1. **Strong price recovery** — BTC climbed..."
  if (sections.length === 0) {
    // Detect bold pseudo-headings on their own line: "**Key developments today:**"
    const boldHeadingRe = /(?:^|\n)\s*\*\*([^*]+?)\*\*\s*:?\s*(?:\n|$)/g
    const boldHeadings: { title: string; start: number; end: number }[] = []
    while ((m = boldHeadingRe.exec(cleanedText)) !== null) {
      const title = m[1].trim()
      // Skip if preceded by a list marker (it's a list item, not a heading)
      const before = cleanedText.slice(Math.max(0, m.index - 10), m.index)
      if (/\d+\.\s*$/.test(before) || /[-•]\s*$/.test(before)) continue
      if (title.length > 2 && title.length < 100) {
        boldHeadings.push({ title, start: m.index, end: m.index + m[0].length })
      }
    }

    // Try numbered items: "1. **Title** — content"
    const numberedRe = /(?:^|\n)\s*\d+\.\s*\*\*([^*]+)\*\*\s*[-–—:]+\s*/g
    const numberedItems: { title: string; start: number; contentStart: number }[] = []
    while ((m = numberedRe.exec(cleanedText)) !== null) {
      const title = m[1].trim()
      if (title.length > 1) {
        numberedItems.push({ title, start: m.index, contentStart: m.index + m[0].length })
      }
    }

    // Find group header for numbered items
    let group: string | undefined
    if (numberedItems.length > 0 && boldHeadings.length > 0) {
      const firstItemStart = numberedItems[0].start
      const precedingHeading = boldHeadings.filter(h => h.end <= firstItemStart).pop()
      if (precedingHeading) group = precedingHeading.title
    }

    for (let i = 0; i < numberedItems.length; i++) {
      const item = numberedItems[i]
      const nextStart = i + 1 < numberedItems.length ? numberedItems[i + 1].start : cleanedText.length
      const rawContent = cleanMagazineContent(cleanedText.slice(item.contentStart, nextStart))
      if (rawContent.length >= 15) {
        addSection(sections, item.title, rawContent, seen, group)
      }
    }

    // Also try bullet items: "- **Title** — content"
    if (sections.length === 0) {
      const bulletRe = /(?:^|\n)\s*[-•]\s*\*\*([^*]+)\*\*\s*[-–—:]+\s*/g
      const bulletItems: { title: string; start: number; contentStart: number }[] = []
      while ((m = bulletRe.exec(cleanedText)) !== null) {
        const title = m[1].trim()
        if (title.length > 1) {
          bulletItems.push({ title, start: m.index, contentStart: m.index + m[0].length })
        }
      }

      for (let i = 0; i < bulletItems.length; i++) {
        const item = bulletItems[i]
        const nextStart = i + 1 < bulletItems.length ? bulletItems[i + 1].start : cleanedText.length
        const rawContent = cleanMagazineContent(cleanedText.slice(item.contentStart, nextStart))
        if (rawContent.length >= 15) {
          addSection(sections, item.title, rawContent, seen)
        }
      }
    }

    // Also try standalone bold paragraphs: "**Title** — content\n\n**Next** — ..."
    if (sections.length === 0) {
      const boldParaRe = /(?:^|\n)\s*\*\*([^*]{3,60})\*\*\s*[-–—:]+\s*([\s\S]*?)(?=\n\s*\*\*[^*]{3,60}\*\*\s*[-–—:]|\n{2,}\*\*[^*]+\*\*\s*:|\s*$)/g
      while ((m = boldParaRe.exec(cleanedText)) !== null) {
        const title = m[1].trim()
        const content = cleanMagazineContent(m[2].trim())
        if (title.length > 2 && content.length >= 15) {
          addSection(sections, title, content, seen)
        }
      }
    }

    // Extract intro paragraph as Summary
    if (sections.length > 0) {
      const firstStructured = numberedItems.length > 0
        ? numberedItems[0].start
        : cleanedText.length
      // Bold pseudo-heading before items marks the end of the intro
      const precedingBoldHeading = boldHeadings.length > 0 && boldHeadings[0].start < firstStructured
        ? boldHeadings[0].start
        : firstStructured
      const fallbackIntroEnd = Math.min(firstStructured, precedingBoldHeading)
      if (fallbackIntroEnd > 30) {
        const intro = cleanedText.slice(0, fallbackIntroEnd)
          .replace(/\*\*/g, '')
          .replace(/^[#*_\s-]+/gm, '')
          .trim()
        if (intro.length > 30 && !seen.has('Summary')) {
          addSection(sections, 'Summary', intro, seen)
        }
      }
    }
  }

  const closingMatch = /(This is being called[^.]+\.[^"]*"[^"]+"[^.]*\.)/i.exec(cleanedText)
  if (closingMatch && !seen.has('Key')) {
    addSection(sections, 'Key takeaway', closingMatch[1].trim(), seen)
  }

  const order = ['Summary', 'Key takeaway']
  sections.sort((a, b) => {
    const ai = order.indexOf(a.title)
    const bi = order.indexOf(b.title)
    if (ai >= 0 && bi >= 0) return ai - bi
    if (ai >= 0) return -1
    if (bi >= 0) return 1
    return 0
  })

  return sections
}

export function extractMagazineHeroImage(text: string): string | undefined {
  return extractFirstImageFromText(text)
}

// ─── Links extraction ─────────────────────────────────────────────

export function extractBoldDomainLinks(text: string): WebSearchResult[] {
  const results: WebSearchResult[] = []
  const seen = new Set<string>()
  const re = /\*\*([^*]+)\*\*\s*\(([a-zA-Z0-9][-a-zA-Z0-9.]*\.[a-zA-Z]{2,})\)/g
  let match: RegExpExecArray | null
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim().slice(0, 500)
    const domain = match[2].trim()
    if (title.length < 2) continue
    const url = /^https?:\/\//i.test(domain) ? domain : `https://${domain}`
    const norm = normUrl(url)
    if (seen.has(norm)) continue
    seen.add(norm)
    results.push({ title, url, content: undefined })
  }
  return results
}

export function extractMarkdownLinks(text: string): WebSearchResult[] {
  const results: WebSearchResult[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(MARKDOWN_LINK_RE.source, 'g')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim().slice(0, 500)
    const rawUrl = match[2].trim()
    if (title.length < 2 || rawUrl.length < 10 || !SAFE_URL_SCHEME.test(rawUrl)) continue
    try {
      new URL(rawUrl)
    } catch {
      continue
    }
    const norm = rawUrl.toLowerCase().replace(/\/$/, '')
    if (seen.has(norm)) continue
    seen.add(norm)
    results.push({ title, url: rawUrl, content: undefined })
  }
  return results
}

export function mergeNewsResults(web: WebSearchResult[], fromText: WebSearchResult[]): WebSearchResult[] {
  const byUrl = new Map<string, WebSearchResult>()
  for (const r of web) {
    byUrl.set(normUrl(r.url), r)
  }
  for (const r of fromText) {
    const k = normUrl(r.url)
    if (!byUrl.has(k)) byUrl.set(k, r)
  }
  return [...byUrl.values()]
}

// ─── Film extraction ──────────────────────────────────────────────

function normalizeFilmId(raw: string): string {
  return raw.startsWith('f') ? raw : `f${raw}`
}

export function extractFilmIds(text: string): string[] {
  const ids: string[] = []
  let match: RegExpExecArray | null
  const re = new RegExp(FILM_TAG_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const id = normalizeFilmId(match[1])
    if (!ids.includes(id)) ids.push(id)
  }
  return ids
}

function resolveFilms(ids: string[]): Film[] {
  return ids
    .map((id) => libraryFilms.find((f) => f.id === id))
    .filter((f): f is Film => !!f)
}

function extractExternalFilms(text: string): Film[] {
  const films: Film[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(FILM_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const year = parseInt(match[2], 10)
    const director = match[3].trim()
    const key = `${title.toLowerCase()}|${year}`
    if (seen.has(key)) continue
    seen.add(key)
    films.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      year,
      posterUrl: '',
      synopsis: extractDescriptionForTag(text, match.index, match[0].length),
      genres: [],
      rating: 0,
      runtime: 0,
      director,
      cast: [],
      sources: [],
    })
  }
  return films
}

export function extractAllFilms(text: string): Film[] {
  const libraryFilms = resolveFilms(extractFilmIds(text))
  const externalFilms = extractExternalFilms(text)
  return [...libraryFilms, ...externalFilms]
}

// ─── Song extraction ──────────────────────────────────────────────

function normalizeSongId(raw: string): string {
  return raw.startsWith('s') ? raw : `s${raw}`
}

export function extractSongIds(text: string): string[] {
  const ids: string[] = []
  let match: RegExpExecArray | null
  const re = new RegExp(SONG_TAG_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const id = normalizeSongId(match[1])
    if (!ids.includes(id)) ids.push(id)
  }
  return ids
}

function resolveSongs(ids: string[]): Song[] {
  return ids
    .map((id) => librarySongs.find((s) => s.id === id))
    .filter((s): s is Song => !!s)
}

/** Reject obvious non-song phrases (news bullets, factual descriptions, etc.) */
function looksLikeSong(title: string, artist: string): boolean {
  const t = title.toLowerCase()
  const a = artist.toLowerCase()
  const bad = [
    'latest news', 'protocol updates', 'community debates', 'real-time information',
    'training cutoff', 'bip discussion', 'beyond my training', 'what people are saying',
    'want me to go', 'search for what', 'look up things', 'direct answer',
    'for deeper coverage', 'for instant', 'check these sources',
    'bip 110', 'bip discussions', 'web search',
    'developer mailing list', 'mailing list reactions', 'technical opinions',
    'community sentiment', 'twitter', 'reddit', 'github', 'stackexchange',
    'bitcoin bips', 'bitcoin mailing', 'canonical source', 'formal dev',
    'what i\'d suggest', 'for bip', 'sources to',
    'series', 'season', 'episode', 'animated', 'anime', 'netflix', 'amazon',
    'streaming', 'hbo', 'hulu', 'disney', 'showtime', 'cancelled', 'renewed',
    'viewership', 'rotten tomatoes', 'imdb',
    'published', 'author', 'edition', 'chapter', 'novel', 'nonfiction',
    'product', 'brand', 'company', 'startup', 'pricing',
    'meaning', 'definition', 'synonym', 'refers to', 'describes',
    'example', 'similar to', 'also known as', 'originates from',
    'a word', 'a term', 'conveys', 'evokes', 'suggests',
  ]
  for (const phrase of bad) {
    if (t.includes(phrase) || a.includes(phrase)) return false
  }
  if (t.length > 55 || a.length > 40) return false
  if (/\b(the act of|a type of|when something|which means|referring to|something that)\b/i.test(a)) return false
  if (/^(this|these|it|that|here|there|when|where|what|how|why|if|but|and|or|the |a |an )\b/i.test(t)) return false
  return true
}

function extractExternalSongs(text: string): Song[] {
  const songs: Song[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(SONG_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const artist = match[2].trim()
    if (!looksLikeSong(title, artist)) continue
    const year = match[3] ? parseInt(match[3], 10) : undefined
    const key = `${title.toLowerCase()}|${artist.toLowerCase()}`
    if (seen.has(key)) continue
    seen.add(key)
    songs.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      artist,
      year,
      coverUrl: undefined,
      sources: [],
    })
  }
  return songs
}

function extractSongsFromLibraryMatch(text: string): Song[] {
  const lower = text.toLowerCase()
  const found: { song: Song; pos: number }[] = []
  const seen = new Set<string>()
  for (const song of librarySongs) {
    const key = song.id
    if (seen.has(key)) continue
    const title = song.title.toLowerCase()
    const artist = song.artist.toLowerCase()
    if (!lower.includes(title)) continue
    const artistRe = new RegExp('\\b' + artist.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\b', 'i')
    if (!artistRe.test(lower)) continue
    const titlePos = lower.indexOf(title)
    const artistMatch = lower.match(artistRe)
    const artistPos = artistMatch?.index ?? -1
    if (artistPos < 0) continue
    const dist = Math.abs(titlePos - artistPos)
    if (dist > 120) continue
    seen.add(key)
    found.push({ song, pos: Math.min(titlePos, artistPos) })
  }
  return found.sort((a, b) => a.pos - b.pos).map((f) => f.song)
}

function extractSongsFromPatterns(text: string): Song[] {
  const songs: { title: string; artist: string; pos: number }[] = []
  const seen = new Set<string>()

  const patterns: { re: RegExp; titleIdx: number; artistIdx: number }[] = [
    { re: /"([^"]{2,80})"\s+by\s+([A-Za-z0-9][^,\n.]{1,50}?)(?:\s*[,\n.]|$)/gi, titleIdx: 1, artistIdx: 2 },
    { re: /\*\*([^*]{2,80})\*\*\s+by\s+([A-Za-z0-9][^,\n.]{1,50}?)(?:\s*[,\n.]|$)/g, titleIdx: 1, artistIdx: 2 },
    { re: /(?:^|\n)\s*(?:\d+\.\s*|[-•]\s*)?([^\n\-–—]{2,60}?)\s*[-–—]\s*([A-Za-z0-9][^,\n]{1,50}?)(?:\s*[,\n.]|$)/gm, titleIdx: 1, artistIdx: 2 },
    { re: /([A-Za-z0-9][^\-–—\n]{2,60}?)\s+[-–—]\s+([A-Za-z0-9][^,\n]{1,50}?)(?=\s*[,\n.]|$)/g, titleIdx: 1, artistIdx: 2 },
  ]

  for (const { re, titleIdx, artistIdx } of patterns) {
    let m: RegExpExecArray | null
    const rx = new RegExp(re.source, re.flags)
    while ((m = rx.exec(text)) !== null) {
      const title = m[titleIdx].trim()
      const artist = m[artistIdx].trim()
      if (title.length < 2 || artist.length < 2) continue
      if (!looksLikeSong(title, artist)) continue
      if (/^\d{4}$/.test(title) || /^\d{4}$/.test(artist)) continue
      if (/(\[\[)?(film|song|podcast|book|tv|place|recipe|event)(_ext)?:/i.test(title)) continue
      if (/(\[\[)?(film|song|podcast|book|tv|place|recipe|event)(_ext)?:/i.test(artist)) continue
      if (/\*\*\[\[/.test(title) || title.includes(']]**')) continue
      const key = `${title.toLowerCase()}|${artist.toLowerCase()}`
      if (seen.has(key)) continue
      seen.add(key)
      songs.push({ title, artist, pos: m.index })
    }
  }

  return songs
    .sort((a, b) => a.pos - b.pos)
    .map(({ title, artist }) => ({
      id: `ext-${`${title}|${artist}`.toLowerCase().replace(/\W/g, '-')}`,
      title,
      artist,
      coverUrl: undefined,
      sources: [],
    }))
}

export function extractAllSongs(text: string, userQuery = ''): Song[] {
  const librarySongs = resolveSongs(extractSongIds(text))
  const externalSongs = extractExternalSongs(text)
  // Explicit song tags and library matches always returned
  const explicitSongs = [...librarySongs, ...externalSongs]

  // Skip pattern-based fallback when conflicting content tags are present
  const hasConflictingTags = extractFilmIds(text).length > 0 || /\[\[film_ext:/.test(text) ||
    extractPodcastIds(text).length > 0 || /\[\[podcast_ext:/.test(text) ||
    /\[\[tv_ext:/.test(text) || /\[\[book_ext:/.test(text)
  if (explicitSongs.length > 0) {
    if (hasConflictingTags) return explicitSongs
    // Also grab pattern matches alongside explicit songs
    const libMatches = extractSongsFromLibraryMatch(text)
    const patternMatches = extractSongsFromPatterns(text)
    const existingKeys = new Set(explicitSongs.map((s) => `${s.title.toLowerCase()}|${s.artist.toLowerCase()}`))
    const extras = [...libMatches, ...patternMatches].filter(
      (p) => !existingKeys.has(`${p.title.toLowerCase()}|${p.artist.toLowerCase()}`)
    )
    return [...explicitSongs, ...extras]
  }
  if (hasConflictingTags) return []
  if (isNewsLikeResponse(text)) return []
  const q = userQuery.toLowerCase()
  if (q && !isMusicQuery(q)) return []
  const libMatches = extractSongsFromLibraryMatch(text)
  const patternMatches = extractSongsFromPatterns(text)
  const libKeys = new Set(libMatches.map((s) => `${s.title.toLowerCase()}|${s.artist.toLowerCase()}`))
  const fromPatterns = patternMatches.filter(
    (p) => !libKeys.has(`${p.title.toLowerCase()}|${p.artist.toLowerCase()}`)
  )
  return [...libMatches, ...fromPatterns]
}

// ─── Podcast extraction ───────────────────────────────────────────

function normalizePodcastId(raw: string): string {
  return raw.startsWith('p') ? raw : `p${raw}`
}

export function extractPodcastIds(text: string): string[] {
  const ids: string[] = []
  let match: RegExpExecArray | null
  const re = new RegExp(PODCAST_TAG_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const id = normalizePodcastId(match[1])
    if (!ids.includes(id)) ids.push(id)
  }
  return ids
}

function resolvePodcasts(ids: string[]): Podcast[] {
  return ids
    .map((id) => libraryPodcasts.find((p) => p.id === id))
    .filter((p): p is Podcast => !!p)
}

function looksLikePodcast(title: string, host: string): boolean {
  const t = title.toLowerCase()
  const h = host.toLowerCase()
  const bad = [
    'bitcoin mailing list', 'mailing list', 'developer mailing list', 'gnusha.org',
    'canonical source', 'formal dev', 'github', 'stackexchange', 'reddit', 'twitter',
    'latest news', 'protocol updates', 'web search', 'training cutoff',
    'documentation', 'bip discussion', 'bip 110', 'bitcoin bips',
  ]
  for (const phrase of bad) {
    if (t.includes(phrase) || h.includes(phrase)) return false
  }
  if (t.length > 80 || h.length > 50) return false
  return true
}

function extractExternalPodcasts(text: string): Podcast[] {
  const podcasts: Podcast[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(PODCAST_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const host = match[2].trim()
    if (!looksLikePodcast(title, host)) continue
    const year = match[3] ? parseInt(match[3], 10) : undefined
    const key = `${title.toLowerCase()}|${host.toLowerCase()}`
    if (seen.has(key)) continue
    seen.add(key)
    podcasts.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      host,
      year,
      coverUrl: undefined,
      sources: [],
    })
  }
  return podcasts
}

export function extractAllPodcasts(text: string): Podcast[] {
  const libraryPodcasts = resolvePodcasts(extractPodcastIds(text))
  const externalPodcasts = extractExternalPodcasts(text)
  return [...libraryPodcasts, ...externalPodcasts]
}

// ─── Book extraction ──────────────────────────────────────────────

function extractExternalBooks(text: string): Book[] {
  const books: Book[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(BOOK_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const author = match[2].trim()
    const year = match[3] ? parseInt(match[3], 10) : undefined
    const key = `${title.toLowerCase()}|${author.toLowerCase()}`
    if (seen.has(key)) continue
    seen.add(key)
    books.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      author,
      year,
      coverUrl: undefined,
      description: extractDescriptionForTag(text, match.index, match[0].length),
      genres: [],
      sources: [],
    })
  }
  return books
}

function extractBooksFromPatterns(text: string): Book[] {
  const books: { title: string; author: string; year?: number; desc: string; pos: number }[] = []
  const seen = new Set<string>()

  const patterns: { re: RegExp; titleIdx: number; authorIdx: number }[] = [
    // "Title" by Author or "Title" by Author (straight and smart quotes)
    { re: /[""\u201C\u201D]([^"""\u201C\u201D]{2,80})[""\u201C\u201D]\s+by\s+([A-Z][^,\n]{1,50}?)(?:\s+(?:is|was|has|—|–|-)|[,()\n.]|$)/gi, titleIdx: 1, authorIdx: 2 },
    // **Title** by/—/– Author
    { re: /\*\*([^*]{2,80})\*\*\s+(?:by|—|–)\s+\*?([A-Z][^*\n]{1,50}?)\*?(?:\s+(?:is|was|has|—|–|-)|[,()*\n.]|$)/g, titleIdx: 1, authorIdx: 2 },
    // List item: "1. Title by Author" or "- Title by Author"
    { re: /(?:^|\n)\s*(?:\d+\.\s*|[-•]\s*)\*{0,2}([^*\n\-–—]{2,80}?)\*{0,2}\s+(?:by|—|–)\s+\*?([A-Z][^*\n]{1,50}?)\*?(?:\s+(?:is|was|has|—|–|-)|[,()*\n.]|$)/gm, titleIdx: 1, authorIdx: 2 },
    // Numbered list: "1. **Title** by Author — Description"
    { re: /(?:^|\n)\s*\d+\.\s*\*\*([^*]{2,80})\*\*\s+by\s+([A-Z][^,\n(—–-]{1,50}?)(?:\s*[,()\n—–-]|$)/gm, titleIdx: 1, authorIdx: 2 },
    // Em-dash list: "- Title — Author (year)" or "- Title — Author"
    { re: /(?:^|\n)\s*[-•]\s+([^—–\n]{2,80}?)\s+[—–]\s+([A-Z][^,\n(]{1,50}?)(?:\s*\(\d{4}\))?(?:\s*[,\n]|$)/gm, titleIdx: 1, authorIdx: 2 },
    // Inline prose at line/sentence start: "Title Words by Author"
    { re: /(?:^|\n|[.!?]\s+)([A-Z][a-z]+(?:\s+[A-Z][a-z]*){1,8})\s+by\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]*){0,4})(?:\s+[a-z]|[,.()\n—–-]|$)/gm, titleIdx: 1, authorIdx: 2 },
    // After recommendation verbs: "enjoy Title by Author"
    { re: /(?:read|enjoy|recommend|check out|try|start with|pick up)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]*){1,8})\s+by\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]*){0,4})(?:\s+[a-z]|[,.()\n—–-]|$)/gi, titleIdx: 1, authorIdx: 2 },
  ]

  for (const { re, titleIdx, authorIdx } of patterns) {
    let m: RegExpExecArray | null
    const rx = new RegExp(re.source, re.flags)
    while ((m = rx.exec(text)) !== null) {
      const title = m[titleIdx].trim().replace(/^\*\*|\*\*$/g, '').replace(/^\[|\]$/g, '')
      const author = m[authorIdx].trim().replace(/^\*\*|\*\*$/g, '')
      if (title.length < 2 || author.length < 2) continue
      if (/\[\[(film|song|podcast|book)(_ext)?:/.test(title)) continue
      if (/^\d{4}$/.test(title) || /^\d{4}$/.test(author)) continue
      const afterMatch = text.substring(m.index + m[0].length, m.index + m[0].length + 200)
      if (/^\s*\]\s*\(https?:/.test(afterMatch) || /\]\(https?:\/\//.test(m[0])) continue
      const key = `${title.toLowerCase()}|${author.toLowerCase()}`
      if (seen.has(key)) continue
      seen.add(key)
      const desc = extractDescriptionForTag(text, m.index, m[0].length)
      books.push({ title, author, desc, pos: m.index })
    }
  }

  return books
    .sort((a, b) => a.pos - b.pos)
    .map(({ title, author, desc }) => ({
      id: `ext-${`${title}|${author}`.toLowerCase().replace(/\W/g, '-')}`,
      title,
      author,
      description: desc,
      coverUrl: undefined,
      genres: [],
      sources: [],
    }))
}

export function extractAllBooks(text: string, userQuery: string): Book[] {
  const externalBooks = extractExternalBooks(text)
  if (externalBooks.length > 0) return externalBooks
  if (!isBookQuery(userQuery) && !isBookLikeResponse(text)) return []
  if (isNewsLikeResponse(text)) return []
  const cleanText = text.replace(/\n---\n\s*(?:Sources|References|Links):?\s*\n[\s\S]*$/i, '')
  return extractBooksFromPatterns(cleanText)
}

// ─── TV Series extraction ─────────────────────────────────────────

function extractExternalTVSeries(text: string): TVSeries[] {
  const series: TVSeries[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(TV_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const field2 = match[2].trim()
    const field3 = match[3]?.trim()
    // Handle both Title|Year|Creator and Title|Creator|Year formats
    const isField2Year = /^\d{4}$/.test(field2)
    const year = isField2Year ? parseInt(field2, 10)
      : field3 && /^\d{4}$/.test(field3) ? parseInt(field3, 10) : undefined
    const creator = isField2Year ? (field3 || undefined) : (field2 || undefined)
    const key = title.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    series.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      creator: creator || undefined,
      year,
      synopsis: extractDescriptionForTag(text, match.index, match[0].length),
      genres: [],
      sources: [],
    })
  }
  return series
}

function isTVLikeResponse(text: string): boolean {
  const hasKeyword = /\b(season|episodes?|showrunner|streaming|renewed|cancelled|premiere|network|HBO|Netflix|AMC|FX|Apple TV|Disney\+|created by)\b/i.test(text)
  if (!hasKeyword) return false
  // Single "season" mention is sufficient
  if ((text.match(/\bseason\b/gi)?.length ?? 0) >= 1) return true
  // Multiple distinct TV signals without requiring "season"
  const tvSignals = [
    /\bepisodes?\b/i, /\bshowrunner\b/i, /\bstreaming\b/i, /\brenewed\b/i,
    /\bcancelled\b/i, /\bpremiere\b/i, /\bnetwork\b/i,
    /\b(HBO|Netflix|AMC|FX|Apple TV|Disney\+|Hulu|Amazon Prime)\b/i,
    /\bseries\b/i, /\bpilot\b/i, /\bminiseries\b/i, /\bcreated by\b/i,
  ]
  return tvSignals.filter(re => re.test(text)).length >= 2
}

function extractTVSeriesFromPatterns(text: string): TVSeries[] {
  const series: { title: string; desc: string; pos: number }[] = []
  const seen = new Set<string>()
  const patterns: RegExp[] = [
    /"([^"]{2,60})"\s*[-–—]\s*(?:a |an )?(?:series|show|tv)/gi,
    /\*\*([^*]{2,60})\*\*\s*[-–—:]\s*(?:a |an )?(?:\w+ )?(?:series|show|drama|comedy|thriller|animated)/gi,
    /(?:^|\n)\s*(?:\d+\.\s*|[-•]\s*)\*{0,2}([^*\n]{2,60}?)\*{0,2}\s*\((\d{4})(?:[-–]\d{0,4})?(?:,\s*\d+ seasons?)?\)/gm,
    // Bold title with seasons info: **Title** — 5 seasons
    /\*\*([^*]{2,60})\*\*\s*[-–—:]\s*\d+\s*seasons?/gi,
    // "Title" — creator (in TV context)
    /"([^"]{2,60})"\s*[-–—]\s*([A-Z][^,\n.]{1,50}?)(?:\s*[,()\n]|$)/gi,
  ]
  for (const re of patterns) {
    let m: RegExpExecArray | null
    const rx = new RegExp(re.source, re.flags)
    while ((m = rx.exec(text)) !== null) {
      const title = m[1].trim().replace(/^\*\*|\*\*$/g, '')
      if (title.length < 2) continue
      if (/\[\[(film|song|podcast|book|tv)(_ext)?:/.test(title)) continue
      const key = title.toLowerCase()
      if (seen.has(key)) continue
      seen.add(key)
      const desc = extractDescriptionForTag(text, m.index, m[0].length)
      series.push({ title, desc, pos: m.index })
    }
  }
  return series
    .sort((a, b) => a.pos - b.pos)
    .map(({ title, desc }) => ({
      id: `ext-${title.toLowerCase().replace(/\W/g, '-')}`,
      title,
      synopsis: desc,
      genres: [],
      sources: [],
    }))
}

function convertFilmExtToTVSeries(text: string): TVSeries[] {
  const series: TVSeries[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(FILM_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const title = match[1].trim()
    const year = parseInt(match[2], 10)
    const creator = match[3].trim()
    const key = title.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    series.push({
      id: `ext-${key.replace(/\W/g, '-')}`,
      title,
      year,
      synopsis: extractDescriptionForTag(text, match.index, match[0].length),
      creator,
      genres: [],
      sources: [],
    })
  }
  return series
}

export function extractAllTVSeries(text: string, userQuery: string): TVSeries[] {
  const external = extractExternalTVSeries(text)
  if (external.length > 0) return external
  if (!isTVQuery(userQuery) && !isTVLikeResponse(text)) return []
  if (isNewsLikeResponse(text)) return []
  if (isTVQuery(userQuery) && /\[\[film_ext:/.test(text)) {
    return convertFilmExtToTVSeries(text)
  }
  if (extractFilmIds(text).length > 0) return []
  return extractTVSeriesFromPatterns(text)
}

// ─── Image extraction ─────────────────────────────────────────────

function extractImages(text: string): ImageItem[] {
  const images: ImageItem[] = []
  const seen = new Set<string>()

  const mdImgRe = /!\[([^\]]*)\]\((https?:\/\/[^)]+)\)/gi
  let m: RegExpExecArray | null
  while ((m = mdImgRe.exec(text)) !== null) {
    const alt = m[1].trim()
    const url = m[2].trim()
    if (seen.has(url)) continue
    seen.add(url)
    const domain = new URL(url).hostname.replace(/^www\./, '')
    images.push({
      id: `img-${seen.size}`,
      url,
      alt: alt || undefined,
      title: alt || undefined,
      source: domain,
    })
  }

  const urlRe = /(https?:\/\/[^\s)"'>]+\.(?:jpg|jpeg|png|gif|webp|svg|avif|bmp|tiff)(?:\?[^\s)"'>]*)?)/gi
  while ((m = urlRe.exec(text)) !== null) {
    const url = m[1].trim()
    if (seen.has(url)) continue
    seen.add(url)
    const domain = new URL(url).hostname.replace(/^www\./, '')
    images.push({
      id: `img-${seen.size}`,
      url,
      source: domain,
    })
  }

  return images
}

export function extractAllImages(text: string, userQuery: string): ImageItem[] {
  const images = extractImages(text)
  if (images.length === 0) return []
  if (isImageQuery(userQuery) || images.length >= 2) return images
  // Show single image if it has meaningful alt text (intentional image context)
  if (images.length === 1 && images[0].alt && images[0].alt.length > 2) return images
  return []
}

// ─── Place extraction ─────────────────────────────────────────────

function extractExternalPlaces(text: string): Place[] {
  const places: Place[] = []
  const seen = new Set<string>()
  let match: RegExpExecArray | null
  const re = new RegExp(PLACE_EXT_RE.source, 'gi')
  while ((match = re.exec(text)) !== null) {
    const name = match[1].trim()
    const cuisine = match[2]?.trim() || undefined
    const city = match[3]?.trim() || undefined
    const rating = match[4] ? parseFloat(match[4]) : undefined
    const priceLevel = match[5] ? parseInt(match[5], 10) : undefined
    const address = match[6]?.trim() || undefined
    const key = name.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    places.push({
      id: `ext-place-${key.replace(/\W/g, '-')}`,
      name,
      cuisine,
      city,
      rating: rating && !isNaN(rating) ? rating : undefined,
      priceLevel: priceLevel && priceLevel >= 1 && priceLevel <= 4 ? priceLevel : undefined,
      address,
      description: extractDescriptionForTag(text, match.index, match[0].length),
      sources: [],
    })
  }
  return places
}

function extractPlacesFromPatterns(text: string): Place[] {
  const places: { name: string; cuisine?: string; city?: string; rating?: number; priceLevel?: number; desc: string; pos: number }[] = []
  const seen = new Set<string>()

  const placeTypeWords = 'restaurant|cafe|bar|bistro|pub|pizzeria|bakery|deli|steakhouse|grill|eatery|spot|joint|trattoria|taqueria|brasserie|cantina|chophouse|creamery|diner|tavern'
  const patterns: RegExp[] = [
    // **Name** — ... type word (within 80 chars of dash)
    new RegExp(`\\*\\*([^*]{2,60})\\*\\*\\s*[-–—:]\\s*(?:a |an )?(?:(\\w[\\w\\s]{1,30}?)\\s+)?(?:${placeTypeWords})`, 'gi'),
    // List item with type word
    new RegExp(`(?:^|\\n)\\s*(?:\\d+\\.\\s*|[-•]\\s*)\\*{0,2}([^*\\n]{2,60}?)\\*{0,2}\\s*[-–—(]\\s*(?:(\\w[\\w\\s&]{1,30}?)\\s+)?(?:${placeTypeWords}|cuisine|food|dining)`, 'gim'),
    // **Name** — description containing a place type word anywhere in the next 120 chars
    new RegExp(`\\*\\*([^*]{2,60})\\*\\*\\s*[-–—:]\\s*([^\\n]{3,120}?\\b(?:${placeTypeWords})\\b[^\\n]{0,40})`, 'gi'),
    // List: 1. **Name** — description with place type word
    new RegExp(`(?:^|\\n)\\s*(?:\\d+\\.\\s*|[-•]\\s*)\\*\\*([^*]{2,60})\\*\\*\\s*[-–—:]\\s*([^\\n]{3,120}?\\b(?:${placeTypeWords})\\b[^\\n]{0,40})`, 'gim'),
  ]

  for (const re of patterns) {
    let m: RegExpExecArray | null
    const rx = new RegExp(re.source, re.flags)
    while ((m = rx.exec(text)) !== null) {
      const name = m[1].trim().replace(/^\*\*|\*\*$/g, '').replace(/^\[|\]$/g, '')
      if (name.length < 2) continue
      if (/\[\[(film|song|podcast|book|tv|place)(_ext)?:/.test(name)) continue
      const key = name.toLowerCase()
      if (seen.has(key)) continue
      seen.add(key)
      const cuisine = m[2]?.trim()
      const desc = extractDescriptionForTag(text, m.index, m[0].length)
      const nearby = text.slice(m.index, m.index + 300)
      const ratingMatch = /(\d\.?\d?)\s*(?:\/\s*5|stars?|★)/i.exec(nearby)
      const rating = ratingMatch ? parseFloat(ratingMatch[1]) : undefined
      const priceMatch = /(\${1,4})\b/.exec(nearby)
      const priceLevel = priceMatch ? priceMatch[1].length : undefined
      places.push({ name, cuisine, desc, rating, priceLevel, pos: m.index })
    }
  }

  return places
    .sort((a, b) => a.pos - b.pos)
    .map(({ name, cuisine, desc, rating, priceLevel }) => ({
      id: `ext-place-${name.toLowerCase().replace(/\W/g, '-')}`,
      name,
      cuisine,
      rating,
      priceLevel,
      description: desc,
      sources: [],
    }))
}

function extractPlacesFromBoldPatterns(text: string): Place[] {
  const places: { name: string; desc: string; pos: number }[] = []
  const seen = new Set<string>()
  // Match any bold title followed by dash/colon and description (for place-query context)
  const re = /(?:^|\n)\s*(?:\d+\.\s*|[-•]\s*)\*\*([^*]{2,60})\*\*\s*[-–—:]\s*([^\n]{5,200})/gm
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    const name = m[1].trim()
    if (name.length < 2) continue
    if (/\[\[(film|song|podcast|book|tv|place)(_ext)?:/.test(name)) continue
    const key = name.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    places.push({ name, desc: m[2].trim(), pos: m.index })
  }
  return places
    .sort((a, b) => a.pos - b.pos)
    .map(({ name, desc }) => ({
      id: `ext-place-${name.toLowerCase().replace(/\W/g, '-')}`,
      name,
      description: desc,
      sources: [],
    }))
}

export function extractAllPlaces(text: string, userQuery: string): Place[] {
  const external = extractExternalPlaces(text)
  if (external.length > 0) return external
  if (!isPlaceQuery(userQuery) && !isPlaceLikeResponse(text)) return []
  if (isNewsLikeResponse(text)) return []
  const fromPatterns = extractPlacesFromPatterns(text)
  // For place queries, supplement with bold-title patterns to catch items without type words
  if (isPlaceQuery(userQuery)) {
    const fromBold = extractPlacesFromBoldPatterns(text)
    const seenNames = new Set(fromPatterns.map(p => p.name.toLowerCase()))
    const extras = fromBold.filter(p => !seenNames.has(p.name.toLowerCase()))
    const combined = [...fromPatterns, ...extras]
    return combined.length > 0 ? combined : []
  }
  return fromPatterns
}

// ─── Code block extraction ─────────────────────────────────────────

export interface CodeBlock {
  language: string
  code: string
  label?: string
}

export function extractCodeBlocks(text: string): CodeBlock[] {
  const blocks: CodeBlock[] = []
  const re = /```(\w*)\n([\s\S]*?)```/g
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    const language = m[1] || 'text'
    const code = m[2].trimEnd()
    if (code.length < 3) continue
    // Try to find a label from the line above the code block
    const before = text.slice(Math.max(0, m.index - 200), m.index)
    const labelMatch = /(?:^|\n)\s*(?:\*\*([^*]{2,80})\*\*|#+\s+(.{2,80}))\s*\n?\s*$/.exec(before)
    const label = labelMatch?.[1] || labelMatch?.[2] || undefined
    blocks.push({ language, code, label })
  }
  return blocks
}

// ─── Tag stripping ────────────────────────────────────────────────

export function stripFilmTags(text: string): string {
  return text
    .replace(FILM_TAG_RE, '')
    .replace(FILM_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripSongTags(text: string): string {
  return text
    .replace(SONG_TAG_RE, '')
    .replace(SONG_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripPodcastTags(text: string): string {
  return text
    .replace(PODCAST_TAG_RE, '')
    .replace(PODCAST_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripBookTags(text: string): string {
  return text
    .replace(BOOK_TAG_RE, '')
    .replace(BOOK_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripTVTags(text: string): string {
  return text
    .replace(TV_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripPlaceTags(text: string): string {
  return text
    .replace(PLACE_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripAppTags(text: string): string {
  return text
    .replace(APP_EXT_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function stripContentTags(text: string): string {
  return stripFilmTags(stripSongTags(stripPodcastTags(stripBookTags(stripTVTags(stripPlaceTags(stripRecipeTags(stripEventTags(stripAppTags(text)))))))))
}

export function stripMarkdownLinks(text: string): string {
  return text
    .replace(/^[\s]*[-*]\s*\[[^\]]+\]\(https?:\/\/[^)\s]+\)\s*$/gm, '')
    .replace(/\s*\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, (_, title) => ` ${title}`)
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

// ─── Recipe extraction (M10.4) ─────────────────────────────────

export interface RecipeData {
  title: string
  servings?: string
  time?: string
  calories?: string
  ingredients: string[]
  steps: string[]
}

const RECIPE_EXT_RE = /<recipe_ext\s+([^>]+)>([\s\S]*?)<\/recipe_ext>/gi

export function extractRecipes(text: string): RecipeData[] {
  const results: RecipeData[] = []
  let m: RegExpExecArray | null
  while ((m = RECIPE_EXT_RE.exec(text)) !== null) {
    const attrs = m[1]
    const body = m[2]

    const title = attrs.match(/title="([^"]*)"/)?.[1] || 'Recipe'
    const servings = attrs.match(/servings="([^"]*)"/)?.[1]
    const time = attrs.match(/time="([^"]*)"/)?.[1]
    const calories = attrs.match(/calories="([^"]*)"/)?.[1]

    // Parse body: lines starting with - are ingredients, numbered lines are steps
    const lines = body.split('\n').map(l => l.trim()).filter(Boolean)
    const ingredients: string[] = []
    const steps: string[] = []

    for (const line of lines) {
      if (/^[-*]\s+/.test(line)) {
        ingredients.push(line.replace(/^[-*]\s+/, ''))
      } else if (/^\d+[.)]\s+/.test(line)) {
        steps.push(line.replace(/^\d+[.)]\s+/, ''))
      }
    }

    results.push({ title, servings, time, calories, ingredients, steps })
  }
  return results
}

export function stripRecipeTags(text: string): string {
  return text.replace(RECIPE_EXT_RE, '').replace(/\n{3,}/g, '\n\n').trim()
}

// ─── Event extraction (M10.5) ──────────────────────────────────

export interface EventData {
  title: string
  date: string
  location?: string
  url?: string
  description?: string
}

const EVENT_EXT_RE = /<event_ext\s+([^>]*?)(?:\/>|>([\s\S]*?)<\/event_ext>)/gi

export function extractEvents(text: string): EventData[] {
  const results: EventData[] = []
  let m: RegExpExecArray | null
  while ((m = EVENT_EXT_RE.exec(text)) !== null) {
    const attrs = m[1]
    const body = m[2]?.trim()

    const title = attrs.match(/title="([^"]*)"/)?.[1] || 'Event'
    const date = attrs.match(/date="([^"]*)"/)?.[1] || ''
    const location = attrs.match(/location="([^"]*)"/)?.[1]
    const url = attrs.match(/url="([^"]*)"/)?.[1]

    results.push({ title, date, location, url, description: body })
  }
  return results
}

export function stripEventTags(text: string): string {
  return text.replace(EVENT_EXT_RE, '').replace(/\n{3,}/g, '\n\n').trim()
}

// ─── Bare domain extraction ──────────────────────────────────────

const KNOWN_TLDS = /\.(com|org|net|io|co|app|dev|xyz|social|news|info|me|tv|fm|live|chat|fyi|ai|so|world|land|pub|lol|cafe|money|exchange|market|tech|design|page|site|online|store|cloud|network|community|foundation)$/i
const FILE_EXT_BLOCK = /\.(js|ts|css|html|json|md|txt|pdf|png|jpg|jpeg|gif|svg|vue|yaml|yml|xml|csv|sql|sh|py|rb|go|rs|toml|lock|env|log|map|wasm|woff2?|ttf|eot|ico)$/i

export function extractBareDomainLinks(text: string): WebSearchResult[] {
  const results: WebSearchResult[] = []
  const seen = new Set<string>()

  // Mark positions already covered by markdown links, bold-domain patterns, and full URLs
  const coveredRanges: [number, number][] = []
  let m: RegExpExecArray | null

  const mdRe = /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g
  while ((m = mdRe.exec(text)) !== null) coveredRanges.push([m.index, m.index + m[0].length])

  const boldRe = /\*\*([^*]+)\*\*\s*\(([a-zA-Z0-9][-a-zA-Z0-9.]*\.[a-zA-Z]{2,})\)/g
  while ((m = boldRe.exec(text)) !== null) coveredRanges.push([m.index, m.index + m[0].length])

  const fullUrlRe = /https?:\/\/[^\s)\]"'<>,]+/g
  while ((m = fullUrlRe.exec(text)) !== null) coveredRanges.push([m.index, m.index + m[0].length])

  function isCovered(idx: number, len: number): boolean {
    return coveredRanges.some(([start, end]) => idx >= start && idx + len <= end)
  }

  const domainRe = /\b([a-zA-Z][a-zA-Z0-9-]*(?:\.[a-zA-Z][a-zA-Z0-9-]*)*\.[a-zA-Z]{2,})\b/g
  while ((m = domainRe.exec(text)) !== null) {
    if (isCovered(m.index, m[0].length)) continue
    const domain = m[1]
    if (!KNOWN_TLDS.test(domain)) continue
    if (FILE_EXT_BLOCK.test(domain)) continue
    if (/^\d/.test(domain)) continue

    const url = `https://${domain}`
    const norm = normUrl(url)
    if (seen.has(norm)) continue
    seen.add(norm)

    const title = domain.charAt(0).toUpperCase() + domain.slice(1)
    results.push({ title, url, content: undefined })
  }

  return results
}

// ─── App extraction ──────────────────────────────────────────────

import { APP_DATABASE, type AppEntry } from '@/data/apps'
import { isAppQuery, isAppLikeResponse, isNostrQuery } from './contentFiltering'

export { type AppEntry } from '@/data/apps'

/** Extract apps from [[app_ext:...]] tags first, then fall back to keyword matching */
export function extractApps(text: string, userQuery: string): AppEntry[] {
  const matched: AppEntry[] = []
  const seen = new Set<string>()
  let hasExplicitTags = false

  // 1. Extract from explicit [[app_ext:Name|Category|Platforms|URL]] tags
  const tagRe = new RegExp(APP_EXT_RE.source, 'gi')
  let m: RegExpExecArray | null
  while ((m = tagRe.exec(text)) !== null) {
    hasExplicitTags = true
    const name = m[1].trim()
    const category = (m[2]?.trim() || 'dev-tool') as AppEntry['category']
    const platforms = (m[3]?.trim() || 'web').split(',').map(p => p.trim()) as AppEntry['platforms']
    const url = m[4]?.trim() || ''
    const id = `ext-${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`
    if (seen.has(id)) continue
    seen.add(id)

    // Check if this matches a known app in the database
    const known = APP_DATABASE.find(a =>
      a.name.toLowerCase() === name.toLowerCase() ||
      a.keywords.some(k => k.toLowerCase() === name.toLowerCase())
    )
    if (known && !seen.has(known.id)) {
      seen.add(known.id)
      matched.push(known)
    } else if (!known) {
      const description = extractDescriptionForTag(text, m.index!, m[0].length) || `${name} — ${category} app`
      matched.push({
        id,
        name,
        description,
        longDescription: description,
        category,
        platforms,
        url,
        keywords: [name.toLowerCase()],
      })
    }
  }

  // 2. Keyword matching against APP_DATABASE (existing behavior)
  const lower = text.toLowerCase()
  for (const app of APP_DATABASE) {
    if (seen.has(app.id)) continue
    const allKeywords = [app.name.toLowerCase(), ...app.keywords.map(k => k.toLowerCase())]
    for (const kw of allKeywords) {
      if (kw.length < 3) continue
      const escaped = kw.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
      const re = kw.length <= 4
        ? new RegExp(`\\b${escaped}\\b`, 'i')
        : new RegExp(escaped, 'i')
      if (re.test(lower) && !seen.has(app.id)) {
        seen.add(app.id)
        matched.push(app)
        break
      }
    }
  }

  // 3. Determine if we should surface apps
  // If we found explicit tags, always show them
  if (hasExplicitTags) {
    return matched
  }

  // Check if query itself mentions a known app name
  const queryLower = userQuery.toLowerCase()
  const queryMatchesApp = APP_DATABASE.some(app =>
    [app.name.toLowerCase(), ...app.keywords.map(k => k.toLowerCase())]
      .some(kw => kw.length >= 3 && queryLower.includes(kw))
  )

  // Surface apps if: app/nostr/known-app query with 1+, or response looks app-like, or 2+ apps detected
  const isAppContext = isAppQuery(userQuery) || isNostrQuery(userQuery) || queryMatchesApp || isAppLikeResponse(text)
  if (isAppContext && matched.length >= 1) return matched
  if (matched.length >= 2) return matched
  return []
}
