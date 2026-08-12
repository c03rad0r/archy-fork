import type { Plugin } from 'vite'
import type { Connect } from 'vite'
import { loadEnv } from 'vite'
import { search as searchDuckDuckGo } from 'duck-duck-scrape'
import { validateDevAuth, setCorsHeaders, checkRateLimit } from './server/dev-auth'

export interface WebSearchResult {
  title: string
  url: string
  content?: string
  imgSrc?: string
  engine?: string
}

// SearXNG public instances — rotated on each request to spread load
const SEARXNG_INSTANCES = [
  'https://searx.tiekoetter.com',
  'https://search.bus-hit.me',
  'https://paulgo.io',
  'https://search.sapti.me',
  'https://search.ononoki.org',
  'https://priv.au',
  'https://opnxng.com',
  'https://etsi.me',
]

// Rotate starting instance per-request to avoid hammering a single one
let instanceRotation = 0

async function fetchFromSearXNG(
  searchUrl: string,
): Promise<{ results?: { title?: string; url?: string; content?: string; img_src?: string; thumbnail?: string; engine?: string }[] } | null> {
  try {
    const searchRes = await fetch(searchUrl, {
      headers: { Accept: 'application/json', 'User-Agent': 'AIUI/1.0' },
      signal: AbortSignal.timeout(3000),
    })
    if (!searchRes.ok) return null
    const text = await searchRes.text()
    // Guard against HTML responses from captcha/blocking pages
    if (text.startsWith('<') || text.startsWith('<!')) return null
    return JSON.parse(text)
  } catch {
    return null
  }
}

/** Brave Search API — free tier (2000 queries/month). Set BRAVE_SEARCH_API_KEY in .env.local */
async function fetchFromBrave(
  q: string,
  apiKey: string,
): Promise<WebSearchResult[]> {
  try {
    const res = await fetch(
      `https://api.search.brave.com/res/v1/web/search?${new URLSearchParams({ q, count: '6' })}`,
      {
        headers: {
          Accept: 'application/json',
          'Accept-Encoding': 'gzip',
          'X-Subscription-Token': apiKey,
        },
        signal: AbortSignal.timeout(8000),
      },
    )
    if (!res.ok) return []
    const data = (await res.json()) as {
      web?: { results?: { title?: string; url?: string; description?: string; thumbnail?: { src?: string } }[] }
    }
    return (data.web?.results ?? [])
      .filter((r) => r.title && r.url)
      .slice(0, 6)
      .map((r) => ({
        title: r.title ?? '',
        url: r.url ?? '',
        content: r.description ?? undefined,
        imgSrc: r.thumbnail?.src ?? undefined,
        engine: 'brave',
      }))
  } catch {
    return []
  }
}

function createWebSearchMiddleware(searxUrl: string | undefined, braveApiKey: string | undefined) {
  return async (req: Connect.IncomingMessage, res: any, next: () => void) => {
    if (req.method !== 'GET') return next()
    if (!validateDevAuth(req, res)) return
    if (!checkRateLimit(req, res)) return
    const url = new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
    const q = url.searchParams.get('q')?.trim()
    if (!q) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Missing q (query)' }))
      return
    }

    const sendResults = (results: WebSearchResult[]) => {
      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=300')
      res.end(JSON.stringify({ results }))
    }

    // 1. Brave Search API (most reliable if configured)
    if (braveApiKey) {
      console.log('[web-search]', q.slice(0, 40), '→ Brave API')
      const braveResults = await fetchFromBrave(q, braveApiKey)
      if (braveResults.length > 0) {
        console.log('[web-search]', braveResults.length, 'results from Brave')
        sendResults(braveResults)
        return
      }
      console.warn('[web-search] Brave API returned no results, falling through')
    }

    // 2. SearXNG instances (rotate to spread load)
    const instances = searxUrl ? [searxUrl] : SEARXNG_INSTANCES
    const startIdx = instanceRotation % instances.length
    instanceRotation++

    let data: { results?: { title?: string; url?: string; content?: string; img_src?: string; thumbnail?: string; engine?: string }[] } | null = null
    let lastError = ''

    // Try at most 3 instances to avoid long waits
    const maxTries = Math.min(3, instances.length)
    for (let i = 0; i < maxTries; i++) {
      const baseUrl = instances[(startIdx + i) % instances.length].replace(/\/$/, '')
      const searchUrl = `${baseUrl}/search?${new URLSearchParams({ q, format: 'json', pageno: '1' })}`
      console.log('[web-search]', q.slice(0, 40), '→', baseUrl)
      data = await fetchFromSearXNG(searchUrl)
      if (data?.results && data.results.length > 0) {
        console.log('[web-search]', data.results.length, 'results from', baseUrl)
        break
      }
      lastError = data ? 'no results' : 'request failed'
      console.warn('[web-search]', baseUrl, lastError, '— trying next')
      data = null
    }

    if (data?.results) {
      const results: WebSearchResult[] = (data.results ?? [])
        .filter((r) => r.title && r.url)
        .slice(0, 6)
        .map((r) => ({
          title: r.title ?? '',
          url: r.url ?? '',
          content: r.content ?? undefined,
          imgSrc: r.img_src || r.thumbnail || undefined,
          engine: r.engine ?? undefined,
        }))
      sendResults(results)
      return
    }

    // 3. DuckDuckGo fallback
    console.warn('[web-search] SearXNG failed, trying DuckDuckGo fallback')
    try {
      const ddg = await searchDuckDuckGo(q)
      if (ddg?.results?.length) {
        const results: WebSearchResult[] = ddg.results
          .filter((r: { title?: string; url?: string }) => r.title && r.url)
          .slice(0, 6)
          .map((r: { title: string; url: string; description?: string; icon?: string }) => ({
            title: r.title,
            url: r.url,
            content: r.description ?? undefined,
            imgSrc: r.icon ?? undefined,
          }))
        console.log('[web-search] DuckDuckGo fallback:', results.length, 'results')
        sendResults(results)
        return
      }
    } catch (ddgErr) {
      console.warn('[web-search] DuckDuckGo fallback failed:', ddgErr)
    }

    console.error('[web-search] All search backends failed:', lastError)
    res.writeHead(502, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Web search unavailable. Set BRAVE_SEARCH_API_KEY or SEARXNG_URL in .env.local.' }))
  }
}

export function webSearchPlugin(): Plugin {
  let searxUrl: string | undefined
  let braveApiKey: string | undefined

  return {
    name: 'aiui-web-search',
    configResolved(config) {
      const env = loadEnv(config.mode, process.cwd(), '')
      searxUrl = env.SEARXNG_URL ?? env.VITE_SEARXNG_URL
      braveApiKey = env.BRAVE_SEARCH_API_KEY ?? env.VITE_BRAVE_SEARCH_API_KEY
      if (braveApiKey) {
        console.log('[web-search] Brave Search API configured')
      } else if (searxUrl) {
        console.log('[web-search] SearXNG:', searxUrl)
      } else {
        console.log('[web-search] No search API key configured — using public SearXNG instances (unreliable)')
        console.log('[web-search] For reliable search, set BRAVE_SEARCH_API_KEY in .env.local (free: https://brave.com/search/api/)')
      }
    },
    configureServer(server) {
      server.middlewares.use('/api/web-search', createWebSearchMiddleware(searxUrl, braveApiKey))
    },
    configurePreviewServer(server) {
      server.middlewares.use('/api/web-search', createWebSearchMiddleware(searxUrl, braveApiKey))
    },
  }
}
