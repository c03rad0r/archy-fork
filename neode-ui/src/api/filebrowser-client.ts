export interface FileBrowserItem {
  name: string
  path: string
  size: number
  modified: string
  isDir: boolean
  type: string
  extension: string
}

interface FileBrowserListResponse {
  items: FileBrowserItem[]
  numDirs: number
  numFiles: number
  sorting: { by: string; asc: boolean }
}

/**
 * Normalize a path: resolve `.` and `..`, reject traversal outside root.
 * Always returns a path starting with `/` and never containing `..`.
 */
export function sanitizePath(path: string): string {
  const segments = path.split('/').filter(Boolean)
  const resolved: string[] = []

  for (const seg of segments) {
    if (seg === '.') continue
    if (seg === '..') {
      resolved.pop() // go up one level, but never past root
    } else {
      resolved.push(seg)
    }
  }

  return '/' + resolved.join('/')
}

class FileBrowserClient {
  private _authenticated = false
  private baseUrl: string

  constructor() {
    this.baseUrl = `${window.location.origin}/app/filebrowser`
  }

  get isAuthenticated(): boolean {
    return this._authenticated
  }

  private getAuthCookie(): string | null {
    const match = document.cookie.match(/(?:^|;\s*)auth=([^;]+)/)
    return match ? match[1]! : null
  }

  async login(): Promise<boolean> {
    try {
      // Get a filebrowser JWT via the authenticated backend (no credentials exposed to browser)
      // Use credentials: 'include' and CSRF token for proper auth
      const csrfMatch = document.cookie.match(/(?:^|;\s*)csrf_token=([^;]+)/)
      const csrfToken = csrfMatch ? csrfMatch[1]! : ''
      const headers: Record<string, string> = { 'Content-Type': 'application/json' }
      if (csrfToken) headers['X-CSRF-Token'] = csrfToken

      const rpcRes = await fetch('/rpc/v1', {
        method: 'POST',
        headers,
        body: JSON.stringify({ method: 'app.filebrowser-token' }),
        credentials: 'include',
      })
      if (!rpcRes.ok) return false
      const rpcData = await rpcRes.json()
      const token = rpcData?.result?.token
      if (!token) return false

      const expires = new Date(Date.now() + 24 * 60 * 60 * 1000).toUTCString()
      const secure = window.location.protocol === 'https:' ? '; Secure' : ''
      document.cookie = `auth=${token}; path=/; SameSite=Lax${secure}; expires=${expires}`
      this._authenticated = true
      return true
    } catch {
      return false
    }
  }

  private headers(): Record<string, string> {
    const h: Record<string, string> = {}
    const cookie = this.getAuthCookie()
    if (cookie) h['X-Auth'] = cookie
    return h
  }

  /** Don't hammer app.filebrowser-token after a failed login — one attempt
   *  per cooldown window, so a broken/missing filebrowser doesn't turn every
   *  poll of the Files card into a fresh login + 401 pair (console spam). */
  private _lastLoginFailure = 0
  private static readonly LOGIN_RETRY_COOLDOWN_MS = 60_000

  /** Ensure we're authenticated before making a request. Auto-logins if needed. */
  private async ensureAuth(): Promise<void> {
    if (this._authenticated && this.getAuthCookie()) return
    if (Date.now() - this._lastLoginFailure < FileBrowserClient.LOGIN_RETRY_COOLDOWN_MS) {
      throw new Error('FileBrowser authentication failed — please open Cloud to log in')
    }
    const ok = await this.login()
    if (!ok) {
      this._lastLoginFailure = Date.now()
      throw new Error('FileBrowser authentication failed — please open Cloud to log in')
    }
  }

  /** fetch() with auth headers + ONE transparent re-login on 401. The JWT
   *  from app.filebrowser-token is short-lived; before this, an expired
   *  cookie kept `_authenticated` true and every Files-card poll 401'd
   *  forever (the console-spam bug, 2026-07-22). */
  private async authedFetch(url: string, init?: RequestInit): Promise<Response> {
    await this.ensureAuth()
    let res = await fetch(url, { ...init, headers: { ...(init?.headers as Record<string, string> | undefined), ...this.headers() } })
    if (res.status === 401) {
      this._authenticated = false
      await this.ensureAuth()
      res = await fetch(url, { ...init, headers: { ...(init?.headers as Record<string, string> | undefined), ...this.headers() } })
    }
    return res
  }

  async listDirectory(path: string): Promise<FileBrowserItem[]> {
    const safePath = sanitizePath(path)
    const res = await this.authedFetch(`${this.baseUrl}/api/resources${safePath}`)
    if (!res.ok) throw new Error(`File Browser is not available (HTTP ${res.status})`)
    // When File Browser isn't installed, nginx falls through to the SPA and
    // returns index.html (200, text/html); when it's down it returns 502.
    // Either way res.json() would throw the opaque "Unexpected token '<'"
    // error, so detect a non-JSON body and surface a friendly message instead.
    const contentType = res.headers.get('content-type') || ''
    if (!contentType.includes('application/json')) {
      throw new Error('File Browser is not available — install or start the File Browser app to use your folders')
    }
    const data: FileBrowserListResponse = await res.json()
    return (data.items || []).map((item) => ({
      ...item,
      extension: item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : '',
    }))
  }

  /**
   * @deprecated Use fetchBlobUrl() instead to avoid exposing tokens in URLs.
   * Returns a plain URL (no token in query string).
   */
  downloadUrl(path: string): string {
    const safePath = sanitizePath(path)
    return `${this.baseUrl}/api/raw${safePath}`
  }

  /**
   * Fetch a file as a blob URL using header-based auth (no token in URL).
   * Use this for img/video/audio src attributes and download links.
   * For large files (video/audio), prefer streamUrl() instead.
   */
  async fetchBlobUrl(path: string): Promise<string> {
    const safePath = sanitizePath(path)
    const res = await this.authedFetch(`${this.baseUrl}/api/raw${safePath}`)
    if (!res.ok) throw new Error(`Failed to fetch file: ${res.status}`)
    const blob = await res.blob()
    return URL.createObjectURL(blob)
  }

  /**
   * Get a direct streaming URL for video/audio `<src>` where the browser
   * needs to make Range requests.
   *
   * Carries NO credential in the query string (T-13-39, fixed 2026-08-03 —
   * this was "the known leak to fix rather than propagate", per
   * 13-CONTEXT.md). `login()` already sets the filebrowser JWT as a
   * `path=/` cookie on this page's own origin, `baseUrl` is that same
   * origin, and the browser attaches the cookie to the same-origin media
   * subresource request automatically — the same mechanism filebrowser's
   * own web UI relies on. Putting the token in the URL too was redundant,
   * and it reached browser history, `Referer` headers and any access log on
   * the path. The cookie itself is unchanged by this fix: it is still a
   * 24-hour JWT, now confined to the cookie jar rather than also appearing
   * in the URL.
   */
  async streamUrl(path: string): Promise<string> {
    await this.ensureAuth()
    const safePath = sanitizePath(path)
    return `${this.baseUrl}/api/raw${safePath}`
  }

  /**
   * Trigger a file download using header-based auth (no token in URL).
   */
  async downloadFile(path: string): Promise<void> {
    const blobUrl = await this.fetchBlobUrl(path)
    const filename = path.split('/').pop() || 'download'
    const a = document.createElement('a')
    a.href = blobUrl
    a.download = filename
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(blobUrl)
  }

  async upload(dirPath: string, file: File): Promise<void> {
    const sanitized = sanitizePath(dirPath)
    const safePath = sanitized.endsWith('/') ? sanitized : `${sanitized}/`
    const encodedName = encodeURIComponent(file.name)
    const res = await this.authedFetch(
      `${this.baseUrl}/api/resources${safePath}${encodedName}?override=true`,
      { method: 'POST', body: file },
    )
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new Error(`Upload failed (${res.status}): ${text}`)
    }
  }

  async createFolder(parentPath: string, name: string): Promise<void> {
    const sanitized = sanitizePath(parentPath)
    const safePath = sanitized.endsWith('/') ? sanitized : `${sanitized}/`
    const sanitizedName = name.replace(/\.\./g, '').replace(/\//g, '')
    const res = await this.authedFetch(`${this.baseUrl}/api/resources${safePath}${sanitizedName}/`, {
      method: 'POST',
    })
    if (!res.ok) throw new Error(`Create folder failed: ${res.status}`)
  }

  async deleteItem(path: string): Promise<void> {
    const safePath = sanitizePath(path)
    const res = await this.authedFetch(`${this.baseUrl}/api/resources${safePath}`, {
      method: 'DELETE',
    })
    if (!res.ok) throw new Error(`Delete failed: ${res.status}`)
  }

  async getUsage(): Promise<{ totalSize: number; folderCount: number; fileCount: number }> {
    let res: Response
    try {
      res = await this.authedFetch(`${this.baseUrl}/api/resources/`)
    } catch {
      // Not installed / login cooling down — the Files card shows zeros.
      return { totalSize: 0, folderCount: 0, fileCount: 0 }
    }
    if (!res.ok) return { totalSize: 0, folderCount: 0, fileCount: 0 }
    const data: FileBrowserListResponse = await res.json()
    const items = data.items || []
    const folderCount = items.filter(i => i.isDir).length
    const fileCount = items.filter(i => !i.isDir).length
    const totalSize = items.reduce((sum, i) => sum + (i.size || 0), 0)
    return { totalSize, folderCount, fileCount }
  }

  private static TEXT_EXTENSIONS = new Set([
    'txt', 'md', 'json', 'csv', 'log', 'conf', 'yaml', 'yml', 'toml', 'xml',
    'html', 'css', 'js', 'ts', 'py', 'sh', 'bash', 'env', 'ini', 'cfg',
    'sql', 'rs', 'go', 'java', 'c', 'h', 'cpp', 'hpp', 'rb', 'php',
    'dockerfile', 'makefile', 'gitignore', 'editorconfig',
  ])

  isTextFile(path: string): boolean {
    const ext = path.includes('.') ? path.split('.').pop()!.toLowerCase() : ''
    const name = path.split('/').pop()?.toLowerCase() || ''
    return FileBrowserClient.TEXT_EXTENSIONS.has(ext) || FileBrowserClient.TEXT_EXTENSIONS.has(name)
  }

  async readFileAsText(path: string, maxBytes = 102400): Promise<{ content: string; truncated: boolean; size: number }> {
    if (!this.isTextFile(path)) {
      throw new Error(`Cannot read binary file: ${path}`)
    }
    const safePath = sanitizePath(path)
    const res = await this.authedFetch(`${this.baseUrl}/api/raw${safePath}`)
    if (!res.ok) throw new Error(`Failed to read file: ${res.status}`)
    const blob = await res.blob()
    const size = blob.size
    const truncated = size > maxBytes
    const slice = truncated ? blob.slice(0, maxBytes) : blob
    const content = await slice.text()
    return { content, truncated, size }
  }

  async rename(oldPath: string, newName: string): Promise<void> {
    const safePath = sanitizePath(oldPath)
    const dir = safePath.substring(0, safePath.lastIndexOf('/') + 1)
    const sanitizedName = newName.replace(/\.\./g, '').replace(/\//g, '')
    const res = await this.authedFetch(`${this.baseUrl}/api/resources${safePath}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ destination: `${dir}${sanitizedName}` }),
    })
    if (!res.ok) throw new Error(`Rename failed: ${res.status}`)
  }
}

export const fileBrowserClient = new FileBrowserClient()
