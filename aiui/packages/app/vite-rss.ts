import type { Plugin } from 'vite'
import type { Connect } from 'vite'
import Parser from 'rss-parser'
import { lookup } from 'dns/promises'
import { validateDevAuth, setCorsHeaders, checkRateLimit } from './server/dev-auth'

export interface RssArticle {
  title: string
  url: string
  content?: string
  imgSrc?: string
}

const RSS_PATHS = ['/feed', '/rss', '/atom.xml', '/feed.xml', '/?feed=rss', '/rss.xml', '/.rss', '/feeds/posts/default']

function extractImgFromItem(item: Record<string, unknown>): string | undefined {
  const enc = item.enclosure as { url?: string; type?: string } | undefined
  if (enc?.url && /^https?:\/\//i.test(enc.url)) {
    const t = (enc.type ?? '').toLowerCase()
    if (t.startsWith('image/') || t === '') return enc.url
  }
  const thumb = item['media:thumbnail'] as { $?: { url?: string }; url?: string } | undefined
  if (thumb?.$?.url) return thumb.$.url
  if (typeof thumb?.url === 'string') return thumb.url
  const arr = thumb as unknown[]
  if (Array.isArray(arr) && arr[0]?.$?.url) return (arr[0] as { $: { url: string } }).$.url
  const media = item['media:content'] as { $?: { url?: string }; url?: string } | undefined
  if (media?.$?.url) return media.$.url
  if (typeof media?.url === 'string') return media.url
  const group = item['media:group'] as { 'media:content'?: Array<{ $?: { url?: string } }> } | undefined
  if (group?.['media:content']?.[0]?.$?.url) return group['media:content'][0].$.url
  const itunes = item['itunes:image'] as { href?: string } | undefined
  if (itunes?.href) return itunes.href
  const content = item.content ?? item.summary
  const html = typeof content === 'string' ? content : ''
  const imgMatch = html.match(/<img[^>]+src=["']([^"']+)["']/i)
  if (imgMatch?.[1]) return imgMatch[1]
  return undefined
}

async function tryParseFeed(parser: Parser, feedUrl: string): Promise<RssArticle[] | null> {
  try {
    const feed = await parser.parseURL(feedUrl)
    if (!feed?.items?.length) return null
    return feed.items.slice(0, 10).map((item) => {
      const rawContent = item.contentSnippet ?? item.content ?? item.summary
      const contentStr = typeof rawContent === 'string' ? rawContent : ''
      const imgSrc = extractImgFromItem(item as Record<string, unknown>)
      return {
        title: (item.title ?? '').trim() || 'Untitled',
        url: (item.link ?? item.guid ?? '').trim() || feedUrl,
        content: contentStr.trim().slice(0, 15000),
        imgSrc: imgSrc && /^https?:\/\//i.test(imgSrc) ? imgSrc : undefined,
      }
    })
  } catch {
    return null
  }
}

function isPrivateIp(ip: string): boolean {
  if (ip === '127.0.0.1' || ip === '::1' || ip === '0.0.0.0') return true
  if (/^10\./.test(ip)) return true
  if (/^172\.(1[6-9]|2\d|3[01])\./.test(ip)) return true
  if (/^192\.168\./.test(ip)) return true
  if (/^169\.254\./.test(ip)) return true
  if (/^fc|^fd/i.test(ip)) return true // IPv6 unique local
  if (/^fe80/i.test(ip)) return true // IPv6 link-local
  return false
}

function isPrivateUrl(urlStr: string): boolean {
  try {
    const u = new URL(urlStr)
    // Block non-http(s) schemes
    if (u.protocol !== 'https:' && u.protocol !== 'http:') return true
    const hostname = u.hostname.toLowerCase()
    if (hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1' || hostname === '[::1]') return true
    if (isPrivateIp(hostname)) return true
    return false
  } catch {
    return true
  }
}

/** Post-DNS SSRF protection: resolve hostname and verify IP is not private */
async function validateResolvedIp(hostname: string): Promise<boolean> {
  try {
    const result = await lookup(hostname)
    return !isPrivateIp(result.address)
  } catch {
    return false // DNS failure = block
  }
}

function discoverFeedUrl(siteUrl: string): string[] {
  try {
    if (isPrivateUrl(siteUrl)) return []
    const u = new URL(siteUrl)
    // Only allow http(s) schemes
    if (u.protocol !== 'https:' && u.protocol !== 'http:') return []
    const base = `${u.protocol}//${u.host}`
    return RSS_PATHS.map((path) => base + path)
  } catch {
    return []
  }
}

async function fetchRssFromUrls(urls: string[]): Promise<RssArticle[]> {
  const parser = new Parser({
    timeout: 5000,
    headers: { 'User-Agent': 'AIUI/1.0 (RSS reader)' },
  })

  const seen = new Set<string>()
  const articles: RssArticle[] = []

  for (const url of urls.slice(0, 5)) {
    // Post-DNS SSRF: verify resolved IP is not private
    try {
      const hostname = new URL(url).hostname
      if (!(await validateResolvedIp(hostname))) continue
    } catch { continue }

    const candidates = discoverFeedUrl(url)
    for (const feedUrl of candidates) {
      const items = await tryParseFeed(parser, feedUrl)
      if (!items?.length) continue
      for (const a of items) {
        const k = a.url.toLowerCase()
        if (seen.has(k)) continue
        seen.add(k)
        articles.push(a)
      }
      break // found a feed for this site, move to next URL
    }
  }

  return articles.slice(0, 15)
}

function createRssMiddleware() {
  return async (req: Connect.IncomingMessage, res: any, next: () => void) => {
    if (req.method !== 'GET') return next()
    const requestUrl = new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
    if (!requestUrl.pathname.startsWith('/api/rss-articles')) return next()
    if (!validateDevAuth(req, res)) return
    if (!checkRateLimit(req, res)) return

    const urls = requestUrl.searchParams.getAll('url').map((u) => u.trim()).filter(Boolean)

    if (urls.length === 0) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Missing urls (array or comma-separated)' }))
      return
    }

    const safe = urls.filter((u) => /^https?:\/\//i.test(u.trim()) && !isPrivateUrl(u.trim())).slice(0, 8)
    if (safe.length === 0) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'No valid https URLs' }))
      return
    }

    try {
      const articles = await fetchRssFromUrls(safe)
      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=300')
      res.end(JSON.stringify({ articles }))
    } catch (err) {
      console.error('[rss]', err)
      res.writeHead(502, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: String(err) }))
    }
  }
}

export function rssPlugin(): Plugin {
  return {
    name: 'aiui-rss',
    configureServer(server) {
      server.middlewares.use(createRssMiddleware())
    },
    configurePreviewServer(server) {
      server.middlewares.use(createRssMiddleware())
    },
  }
}
