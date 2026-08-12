/**
 * Regression pin for T-13-39 — `streamUrl` used to append `?auth=<jwt>` to
 * the raw-file URL, leaking the filebrowser JWT into browser history,
 * `Referer` headers and access logs. 13-CONTEXT.md names this "the known
 * leak to fix rather than propagate"; this file pins the fix so it cannot
 * silently regress.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

// FileBrowserClient reads window.location.origin in its constructor.
Object.defineProperty(window, 'location', {
  value: { origin: 'http://localhost', protocol: 'http:', hostname: 'localhost', pathname: '/app/filebrowser' },
  writable: true,
})

const { fileBrowserClient } = await import('../filebrowser-client')

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(typeof body === 'string' ? body : JSON.stringify(body)),
    blob: () => Promise.resolve(new Blob([JSON.stringify(body)])),
    headers: new Headers({ 'content-type': 'application/json' }),
    redirected: false,
    type: 'basic' as ResponseType,
    url: '',
    clone: () => jsonResponse(body, status),
    body: null,
    bodyUsed: false,
    arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    formData: () => Promise.resolve(new FormData()),
    bytes: () => Promise.resolve(new Uint8Array()),
  }
}

describe('FileBrowserClient.streamUrl', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    ;(fileBrowserClient as unknown as { _authenticated: boolean })._authenticated = false
    document.cookie = 'auth=; expires=Thu, 01 Jan 1970 00:00:00 GMT'
  })

  it('resolves to a same-origin raw-file URL with no query component', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: { token: 'super-secret-jwt-token' } }))

    const url = await fileBrowserClient.streamUrl('/Music/song.m4a')

    expect(url).toBe('http://localhost/app/filebrowser/api/raw/Music/song.m4a')
    expect(url).not.toContain('?')
  })

  it('never embeds the filebrowser JWT anywhere in the returned string', async () => {
    const token = 'super-secret-jwt-token-value-12345'
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: { token } }))

    const url = await fileBrowserClient.streamUrl('/Videos/movie.mp4')

    expect(url).not.toContain(token)
    expect(url).not.toMatch(/[?&]auth=/)
  })

  it('awaits authentication (sets the cookie the media request relies on) before returning', async () => {
    mockFetch.mockResolvedValueOnce(jsonResponse({ result: { token: 'jwt-abc' } }))

    await fileBrowserClient.streamUrl('/Videos/movie.mp4')

    // The cookie login() sets is what the same-origin media request depends
    // on now that the URL itself carries no credential — assert it's really
    // there by the time the caller has the URL in hand.
    expect(document.cookie).toContain('auth=jwt-abc')
  })

  it('does not re-authenticate when a valid session already exists', async () => {
    ;(fileBrowserClient as unknown as { _authenticated: boolean })._authenticated = true
    document.cookie = 'auth=already-authed'

    const url = await fileBrowserClient.streamUrl('/a.mp3')

    expect(mockFetch).not.toHaveBeenCalled()
    expect(url).toBe('http://localhost/app/filebrowser/api/raw/a.mp3')
  })

  it('still resolves traversal via sanitizePath — a path cannot escape root', async () => {
    ;(fileBrowserClient as unknown as { _authenticated: boolean })._authenticated = true
    document.cookie = 'auth=already-authed'

    const url = await fileBrowserClient.streamUrl('/Music/../../etc/passwd')

    expect(url).toBe('http://localhost/app/filebrowser/api/raw/etc/passwd')
    expect(url).not.toContain('..')
  })
})
