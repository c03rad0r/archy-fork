import { describe, it, expect, vi, beforeEach } from 'vitest'
import { sanitizePath } from '../filebrowser-client'

const mockFetch = vi.fn()
vi.stubGlobal('fetch', mockFetch)

// FileBrowserClient reads window.location.origin in constructor, so stub it
Object.defineProperty(window, 'location', {
  value: { origin: 'http://localhost', protocol: 'http:', hostname: 'localhost', pathname: '/app/filebrowser' },
  writable: true,
})

// Import after stubs
const { fileBrowserClient } = await import('../filebrowser-client')

function jsonResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(typeof body === 'string' ? body : JSON.stringify(body)),
    blob: () => Promise.resolve(new Blob([JSON.stringify(body)])),
    // A real File Browser JSON response carries this; listDirectory now guards
    // on it (B4) to detect the SPA-fallback HTML / 502 cases.
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

/** Set up authenticated state — bypasses jsdom cookie path restrictions */
function setAuthenticated() {
  ;(fileBrowserClient as any)._authenticated = true
  document.cookie = 'auth=test-token'
}

describe('FileBrowserClient', () => {
  beforeEach(() => {
    mockFetch.mockReset()
    ;(fileBrowserClient as any)._authenticated = false
    document.cookie = 'auth=; expires=Thu, 01 Jan 1970 00:00:00 GMT'
  })

  describe('login', () => {
    it('authenticates via backend RPC and stores token', async () => {
      mockFetch.mockResolvedValueOnce(jsonResponse({ result: { token: 'jwt-token-123' } }))

      const result = await fileBrowserClient.login()

      expect(result).toBe(true)
      expect(fileBrowserClient.isAuthenticated).toBe(true)
      expect(mockFetch).toHaveBeenCalledWith(
        '/rpc/v1',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ method: 'app.filebrowser-token' }),
        }),
      )
    })

    it('returns false on failed login', async () => {
      mockFetch.mockResolvedValueOnce(jsonResponse(null, 403))

      const result = await fileBrowserClient.login()

      expect(result).toBe(false)
    })

    it('returns false on network error', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'))

      const result = await fileBrowserClient.login()

      expect(result).toBe(false)
    })
  })

  describe('listDirectory', () => {
    it('lists items in a directory', async () => {
      setAuthenticated()

      const mockItems = {
        items: [
          { name: 'photos', path: '/photos', size: 0, modified: '2026-01-01', isDir: true, type: '', extension: '' },
          { name: 'readme.txt', path: '/readme.txt', size: 1024, modified: '2026-01-01', isDir: false, type: '', extension: 'txt' },
        ],
        numDirs: 1,
        numFiles: 1,
        sorting: { by: 'name', asc: true },
      }
      mockFetch.mockResolvedValueOnce(jsonResponse(mockItems))

      const items = await fileBrowserClient.listDirectory('/')

      expect(items).toHaveLength(2)
      expect(items[0]!.name).toBe('photos')
      expect(items[1]!.extension).toBe('txt')
    })

    it('adds leading slash if missing', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse({ items: [], numDirs: 0, numFiles: 0, sorting: { by: 'name', asc: true } }))

      await fileBrowserClient.listDirectory('photos')

      const [url] = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]!
      expect(url).toContain('/api/resources/photos')
    })

    it('throws on non-OK response', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 404))

      await expect(fileBrowserClient.listDirectory('/missing')).rejects.toThrow('File Browser is not available (HTTP 404)')
    })

    it('throws a friendly error when File Browser is absent and nginx serves the SPA (B4)', async () => {
      setAuthenticated()

      // 200 but text/html (SPA index.html fallback) — res.json() would throw the
      // opaque "Unexpected token '<'"; the guard must surface a friendly message.
      const htmlResponse = {
        ...jsonResponse('<!doctype html><html></html>'),
        headers: new Headers({ 'content-type': 'text/html' }),
      } as Response
      mockFetch.mockResolvedValueOnce(htmlResponse)

      await expect(fileBrowserClient.listDirectory('/')).rejects.toThrow('File Browser is not available')
    })
  })

  describe('downloadUrl', () => {
    it('constructs download URL for file path', async () => {
      const url = fileBrowserClient.downloadUrl('/photos/sunset.jpg')

      expect(url).toContain('/api/raw/photos/sunset.jpg')
    })

    it('adds leading slash if missing', async () => {
      const url = fileBrowserClient.downloadUrl('file.txt')

      expect(url).toContain('/api/raw/file.txt')
    })
  })

  describe('upload', () => {
    it('uploads a file to the correct path', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 200))
      const file = new File(['hello'], 'test.txt', { type: 'text/plain' })

      await fileBrowserClient.upload('/documents', file)

      const [url, init] = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]!
      expect(url).toContain('/api/resources/documents/test.txt')
      expect(url).toContain('override=true')
      expect(init.method).toBe('POST')
      expect(init.body).toBe(file)
    })

    it('throws on upload failure', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse('Disk full', 507))
      const file = new File(['data'], 'big.bin')

      await expect(fileBrowserClient.upload('/', file)).rejects.toThrow('Upload failed (507)')
    })
  })

  describe('createFolder', () => {
    it('creates a folder at the correct path', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 200))

      await fileBrowserClient.createFolder('/documents', 'photos')

      const [url, init] = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]!
      expect(url).toContain('/api/resources/documents/photos/')
      expect(init.method).toBe('POST')
    })

    it('throws on failure', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 500))

      await expect(fileBrowserClient.createFolder('/', 'test')).rejects.toThrow('Create folder failed: 500')
    })
  })

  describe('deleteItem', () => {
    it('sends DELETE request for the item', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 200))

      await fileBrowserClient.deleteItem('/photos/old.jpg')

      const [url, init] = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]!
      expect(url).toContain('/api/resources/photos/old.jpg')
      expect(init.method).toBe('DELETE')
    })

    it('throws on failure', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 403))

      await expect(fileBrowserClient.deleteItem('/protected')).rejects.toThrow('Delete failed: 403')
    })
  })

  describe('getUsage', () => {
    it('returns usage summary for root directory', async () => {
      setAuthenticated()

      const mockData = {
        items: [
          { name: 'photos', path: '/photos', size: 0, modified: '2026-01-01', isDir: true, type: '', extension: '' },
          { name: 'file1.txt', path: '/file1.txt', size: 500, modified: '2026-01-01', isDir: false, type: '', extension: 'txt' },
          { name: 'file2.jpg', path: '/file2.jpg', size: 1500, modified: '2026-01-01', isDir: false, type: '', extension: 'jpg' },
        ],
        numDirs: 1,
        numFiles: 2,
        sorting: { by: 'name', asc: true },
      }
      mockFetch.mockResolvedValueOnce(jsonResponse(mockData))

      const usage = await fileBrowserClient.getUsage()

      expect(usage.totalSize).toBe(2000)
      expect(usage.folderCount).toBe(1)
      expect(usage.fileCount).toBe(2)
    })

    it('returns zeros on failed request', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 500))

      const usage = await fileBrowserClient.getUsage()

      expect(usage).toEqual({ totalSize: 0, folderCount: 0, fileCount: 0 })
    })
  })

  describe('isTextFile', () => {
    it('identifies text file extensions', () => {
      expect(fileBrowserClient.isTextFile('readme.md')).toBe(true)
      expect(fileBrowserClient.isTextFile('config.json')).toBe(true)
      expect(fileBrowserClient.isTextFile('script.py')).toBe(true)
      expect(fileBrowserClient.isTextFile('main.rs')).toBe(true)
      expect(fileBrowserClient.isTextFile('style.css')).toBe(true)
    })

    it('returns false for binary files', () => {
      expect(fileBrowserClient.isTextFile('photo.jpg')).toBe(false)
      expect(fileBrowserClient.isTextFile('video.mp4')).toBe(false)
      expect(fileBrowserClient.isTextFile('archive.zip')).toBe(false)
    })
  })

  describe('rename', () => {
    it('sends PATCH request with new destination', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 200))

      await fileBrowserClient.rename('/photos/old.jpg', 'new.jpg')

      const [url, init] = mockFetch.mock.calls[mockFetch.mock.calls.length - 1]!
      expect(url).toContain('/api/resources/photos/old.jpg')
      expect(init.method).toBe('PATCH')
      expect(JSON.parse(init.body)).toEqual({ destination: '/photos/new.jpg' })
    })

    it('throws on rename failure', async () => {
      setAuthenticated()

      mockFetch.mockResolvedValueOnce(jsonResponse(null, 409))

      await expect(fileBrowserClient.rename('/a.txt', 'b.txt')).rejects.toThrow('Rename failed: 409')
    })
  })
})

describe('sanitizePath', () => {
  it('returns / for empty path', () => {
    expect(sanitizePath('')).toBe('/')
  })

  it('preserves simple paths', () => {
    expect(sanitizePath('/photos')).toBe('/photos')
    expect(sanitizePath('/docs/readme.md')).toBe('/docs/readme.md')
  })

  it('adds leading slash', () => {
    expect(sanitizePath('photos/image.jpg')).toBe('/photos/image.jpg')
  })

  it('resolves . segments', () => {
    expect(sanitizePath('/photos/./image.jpg')).toBe('/photos/image.jpg')
  })

  it('resolves .. segments', () => {
    expect(sanitizePath('/photos/../etc/passwd')).toBe('/etc/passwd')
  })

  it('prevents traversal past root', () => {
    expect(sanitizePath('/../../../etc/passwd')).toBe('/etc/passwd')
    expect(sanitizePath('/../../..')).toBe('/')
  })

  it('handles multiple consecutive .. at root', () => {
    expect(sanitizePath('/../../../etc/shadow')).toBe('/etc/shadow')
  })

  it('handles mixed . and .. segments', () => {
    expect(sanitizePath('/a/./b/../c')).toBe('/a/c')
  })

  it('removes trailing slashes in segments', () => {
    expect(sanitizePath('/photos//image.jpg')).toBe('/photos/image.jpg')
  })
})

describe('upload transport progress and cancellation', () => {
  class UploadXHR {
    static instances: UploadXHR[] = []
    upload = { onprogress: null as ((e: { loaded: number }) => void) | null }
    onload: (() => void) | null = null
    onabort: (() => void) | null = null
    onerror: (() => void) | null = null
    status = 200
    withCredentials = false
    contentType = 'application/json'
    open = vi.fn(); setRequestHeader = vi.fn(); send = vi.fn()
    abort = vi.fn(() => this.onabort?.())
    getResponseHeader() { return this.contentType }
    constructor() { UploadXHR.instances.push(this) }
  }
  beforeEach(() => { setAuthenticated(); UploadXHR.instances = []; vi.stubGlobal('XMLHttpRequest', UploadXHR) })
  it('sends the file bytes, escapes folder names, and waits for server acceptance after 100%', async () => {
    const controller = new AbortController(); const onProgress = vi.fn()
    const file = new File(['data'], 'a #.txt')
    let complete = false
    const job = fileBrowserClient.upload('/folder #1', file, { signal: controller.signal, onProgress }).then(() => { complete = true })
    await Promise.resolve(); await Promise.resolve()
    const xhr = UploadXHR.instances[0]!
    expect(xhr.open).toHaveBeenCalledWith('POST', expect.stringMatching(/\/app\/filebrowser\/api\/resources\/folder%20%231\/a%20%23.txt\?override=true$/))
    expect(xhr.send).toHaveBeenCalledWith(file)
    xhr.upload.onprogress?.({ loaded: 4 }); expect(onProgress).toHaveBeenCalledWith(4)
    expect(complete).toBe(false)
    xhr.onload?.(); await job; expect(complete).toBe(true)
  })
  it('cancels the actual request and refuses HTML masquerading as upload success', async () => {
    const controller = new AbortController()
    const job = fileBrowserClient.upload('/', new File(['x'], 'f'), { signal: controller.signal, onProgress: vi.fn() })
    await Promise.resolve(); await Promise.resolve()
    controller.abort(); await expect(job).rejects.toMatchObject({ name: 'AbortError' })
    expect(UploadXHR.instances[0]!.abort).toHaveBeenCalled()
    const next = fileBrowserClient.upload('/', new File(['x'], 'f'), { signal: new AbortController().signal, onProgress: vi.fn() })
    await Promise.resolve(); await Promise.resolve()
    const xhr = UploadXHR.instances[1]!; xhr.contentType = 'text/html'; xhr.onload?.()
    await expect(next).rejects.toThrow('login page')
  })
})
