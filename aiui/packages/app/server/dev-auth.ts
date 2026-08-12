/**
 * Shared dev server authentication and rate limiting middleware.
 * Validates Bearer token on all /api/* requests.
 * Token is auto-generated in scripts/dev.sh and injected via VITE_DEV_API_TOKEN.
 */
import type { IncomingMessage, ServerResponse } from 'http'

/** Validate Authorization header. Returns true if authorized, false if rejected (response already sent). */
export function validateDevAuth(req: IncomingMessage, res: ServerResponse): boolean {
  const token = process.env.VITE_DEV_API_TOKEN ?? ''
  if (!token) return true // No token configured, skip auth
  const auth = req.headers.authorization
  if (auth === `Bearer ${token}`) return true
  res.writeHead(401, { 'Content-Type': 'application/json' })
  res.end(JSON.stringify({ error: 'Unauthorized' }))
  return false
}

const ALLOWED_ORIGIN = 'http://localhost:5173'

/** Set CORS headers with explicit localhost origin instead of wildcard. */
export function setCorsHeaders(res: ServerResponse): void {
  res.setHeader('Access-Control-Allow-Origin', ALLOWED_ORIGIN)
}

/** Write CORS preflight response. */
export function handleCorsOptions(res: ServerResponse): void {
  res.writeHead(204, {
    'Access-Control-Allow-Origin': ALLOWED_ORIGIN,
    'Access-Control-Allow-Methods': 'GET, POST, PUT, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Authorization',
  })
  res.end()
}

// ─── Rate Limiting ──────────────────────────────────────────

const WINDOW_MS = 60_000 // 1 minute window
const READ_LIMIT = 60 // 60 requests per minute for reads
const WRITE_LIMIT = 10 // 10 requests per minute for writes

interface RateBucket {
  count: number
  resetAt: number
}

const rateBuckets = new Map<string, RateBucket>()

// Clean up stale buckets every 5 minutes
setInterval(() => {
  const now = Date.now()
  for (const [key, bucket] of rateBuckets) {
    if (now > bucket.resetAt) rateBuckets.delete(key)
  }
}, 5 * 60_000)

function getClientIp(req: IncomingMessage): string {
  return req.socket.remoteAddress ?? 'unknown'
}

/**
 * Check rate limit for a request. Returns true if allowed, false if rejected (response already sent).
 * @param isWrite - Set to true for write operations (POST/PUT/DELETE) which have a lower limit.
 */
export function checkRateLimit(req: IncomingMessage, res: ServerResponse, isWrite = false): boolean {
  const ip = getClientIp(req)
  const limit = isWrite ? WRITE_LIMIT : READ_LIMIT
  const key = `${ip}:${isWrite ? 'w' : 'r'}`
  const now = Date.now()

  let bucket = rateBuckets.get(key)
  if (!bucket || now > bucket.resetAt) {
    bucket = { count: 0, resetAt: now + WINDOW_MS }
    rateBuckets.set(key, bucket)
  }

  bucket.count++
  if (bucket.count > limit) {
    res.writeHead(429, { 'Content-Type': 'application/json', 'Retry-After': '60' })
    res.end(JSON.stringify({ error: 'Too many requests' }))
    return false
  }
  return true
}
