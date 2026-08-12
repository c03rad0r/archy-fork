import type { Plugin } from 'vite'
import type { Connect } from 'vite'
import { loadEnv } from 'vite'
import { validateDevAuth, setCorsHeaders, checkRateLimit } from './server/dev-auth'

const TMDB_POSTER = 'https://image.tmdb.org/t/p/w342'
const TMDB_BACKDROP = 'https://image.tmdb.org/t/p/w780'

function createTmdbSearchMiddleware(tmdbKey: string | undefined, type: 'movie' | 'tv') {
  return async (req: Connect.IncomingMessage, res: any, next: () => void) => {
    if (req.method !== 'GET') return next()
    if (!validateDevAuth(req, res)) return
    if (!checkRateLimit(req, res)) return
    const url = new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
    const q = url.searchParams.get('q')?.trim()
    const y = url.searchParams.get('y')
    if (!q) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Missing q (query)' }))
      return
    }
    if (!tmdbKey) {
      res.writeHead(503, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'TMDB_API_KEY not configured' }))
      return
    }
    try {
      const yearParam = type === 'movie' ? 'primary_release_year' : 'first_air_date_year'
      const params = new URLSearchParams({
        api_key: tmdbKey,
        query: q,
        ...(y ? { [yearParam]: y } : {}),
      })
      const tmdbRes = await fetch(
        `https://api.themoviedb.org/3/search/${type}?${params}`
      )
      const data = (await tmdbRes.json()) as {
        results?: { poster_path?: string; backdrop_path?: string }[]
      }
      const first = data.results?.[0]
      const posterUrl = first?.poster_path ? `${TMDB_POSTER}${first.poster_path}` : null
      const backdropUrl = first?.backdrop_path ? `${TMDB_BACKDROP}${first.backdrop_path}` : null
      res.setHeader('Content-Type', 'application/json')
      setCorsHeaders(res)
      res.setHeader('Cache-Control', 'public, max-age=86400')
      res.end(JSON.stringify({ posterUrl, backdropUrl }))
    } catch (err) {
      console.error('[tmdb]', err)
      res.writeHead(502, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: String(err) }))
    }
  }
}

export function tmdbPlugin(): Plugin {
  let tmdbKey: string | undefined

  return {
    name: 'aiui-tmdb-proxy',
    configResolved(config) {
      const env = loadEnv(config.mode, process.cwd(), '')
      tmdbKey = env.TMDB_API_KEY ?? env.VITE_TMDB_API_KEY
    },
    configureServer(server) {
      server.middlewares.use('/api/tmdb/search', createTmdbSearchMiddleware(tmdbKey, 'movie'))
      server.middlewares.use('/api/tmdb/search-tv', createTmdbSearchMiddleware(tmdbKey, 'tv'))
    },
    configurePreviewServer(server) {
      server.middlewares.use('/api/tmdb/search', createTmdbSearchMiddleware(tmdbKey, 'movie'))
      server.middlewares.use('/api/tmdb/search-tv', createTmdbSearchMiddleware(tmdbKey, 'tv'))
    },
  }
}
