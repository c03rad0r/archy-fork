/** File Browser 2.63.23 TUS uploads. Keep partial data under a unique name;
 * only publish the requested filename after the server has all the bytes.
 * The File remains in memory: this recovers a suspended page, not a killed page.
 */
export interface ResumableUploadOptions {
  signal: AbortSignal
  onProgress: (bytes: number) => void
  onPaused?: (paused: boolean) => void
}
type Transport = (url: string, init?: RequestInit) => Promise<Response>
class Retryable extends Error {}
const abortError = () => new DOMException('Upload cancelled', 'AbortError')
const encodePath = (path: string) => path.split('/').map(encodeURIComponent).join('/')

function waitForConnection(signal: AbortSignal, delay: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      clearTimeout(timer)
      window.removeEventListener('online', wake)
      document.removeEventListener('visibilitychange', visible)
      signal.removeEventListener('abort', abort)
    }
    const wake = () => { cleanup(); resolve() }
    const visible = () => { if (document.visibilityState === 'visible') wake() }
    const abort = () => { cleanup(); reject(abortError()) }
    const timer = setTimeout(wake, delay)
    window.addEventListener('online', wake)
    document.addEventListener('visibilitychange', visible)
    signal.addEventListener('abort', abort, { once: true })
    if (signal.aborted) abort()
  })
}

export async function resumableUpload(
  baseUrl: string, folder: string, file: File, transport: Transport,
  options: ResumableUploadOptions,
): Promise<void> {
  if (!file.name || /[/\\\0]/.test(file.name) || file.name === '.' || file.name === '..') {
    throw new Error('Invalid upload filename')
  }
  const nonce = Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join('')
  const stage = `${folder.replace(/\/$/, '')}/.archy-upload-${nonce}.part`
  const destination = `${folder.replace(/\/$/, '')}/${file.name}`
  const tus = `${baseUrl}/api/tus${encodePath(stage)}`
  const resource = (path: string) => `${baseUrl}/api/resources${encodePath(path)}`
  let created = false
  let checksum: string | undefined
  let retry = 0
  let finished = false

  const request = async (url: string, init: RequestInit = {}, signal = options.signal) => {
    if (signal.aborted) throw abortError()
    const controller = new AbortController()
    const abort = () => controller.abort()
    signal.addEventListener('abort', abort, { once: true })
    const timer = setTimeout(abort, 60_000)
    try {
      const response = await transport(url, { ...init, signal: controller.signal, cache: 'no-store' })
      if (response.status === 408 || response.status === 429 || (response.status >= 500 && response.status !== 507)) {
        throw new Retryable('Server temporarily unavailable')
      }
      if (response.headers.get('content-type')?.includes('text/html')) {
        throw new Error('Upload needs a working File Browser connection. Reopen Cloud and try again.')
      }
      if (response.status === 401 || response.status === 403) throw new Error('Upload access denied. Sign in again or check folder permissions.')
      if (response.status === 507) throw new Error('Upload stopped: the server has insufficient storage.')
      return response
    } catch (error) {
      if (signal.aborted) throw abortError()
      if (controller.signal.aborted || error instanceof TypeError) throw new Retryable('Connection interrupted')
      throw error
    } finally {
      clearTimeout(timer)
      signal.removeEventListener('abort', abort)
    }
  }
  const metadata = async (path: string, hash = false) => {
    const response = await request(`${resource(path)}${hash ? '?checksum=sha256' : ''}`)
    if (response.status === 404) return null
    if (response.status !== 200) throw new Error(`Cannot verify uploaded file (HTTP ${response.status})`)
    const data = await response.json()
    if (data.isDir || !Number.isSafeInteger(data.size) || data.size < 0) throw new Error('Invalid upload verification response')
    return data as { size: number; checksums?: Record<string, string> }
  }
  try {
    while (!finished) {
      if (options.signal.aborted) throw abortError()
      try {
        if (checksum) {
          // A rename can succeed while its response is lost. Verify exact server
          // content, not just size (an old destination might have the same size).
          const source = await metadata(stage)
          if (!source) {
            const target = await metadata(destination, true)
            if (target?.size !== file.size || target.checksums?.sha256 !== checksum) {
              throw new Error('Cannot confirm the saved upload. Check the destination before trying again.')
            }
            finished = true
          } else {
            if (source.size !== file.size) throw new Error('Upload changed before it could be saved')
            // File Browser decodes destination twice: protect literal %, + and ?.
            const query = new URLSearchParams({ action: 'rename', destination: encodeURIComponent(destination), override: 'true', rename: 'false' })
            const saved = await request(`${resource(stage)}?${query}`, { method: 'PATCH' })
            if (saved.status === 404) throw new Retryable('Checking completed upload')
            if (!saved.ok) throw new Error(`Could not save uploaded file (HTTP ${saved.status})`)
            finished = true
          }
        } else {
          const head = await request(tus, { method: 'HEAD' })
          let offset: number
          if (head.status === 404) {
            const existing = await metadata(stage)
            if (existing?.size === file.size) {
              offset = file.size // final PATCH acknowledged on server, reply lost
            } else if (existing || created) {
              throw new Error('The server upload session expired. Please select the file again.')
            } else {
              const post = await request(tus, { method: 'POST', headers: { 'Upload-Length': String(file.size), 'Tus-Resumable': '1.0.0' } })
              if (post.status === 409) throw new Retryable('Checking existing upload')
              if (post.status !== 201) throw new Error(`Could not start resumable upload (HTTP ${post.status})`)
              created = true
              offset = 0
            }
          } else {
            const rawOffset = head.headers.get('Upload-Offset')
            const rawLength = head.headers.get('Upload-Length')
            offset = Number(rawOffset)
            if (head.status !== 200 || rawOffset === null || rawLength === null || Number(rawLength) !== file.size || !Number.isSafeInteger(offset) || offset < 0 || offset > file.size) {
              throw new Error('The server returned an invalid upload position')
            }
            created = true
          }
          options.onProgress(offset)
          options.onPaused?.(false)
          if (offset < file.size) {
            const end = Math.min(offset + 2 * 1024 * 1024, file.size)
            const patch = await request(tus, { method: 'PATCH', headers: { 'Content-Type': 'application/offset+octet-stream', 'Upload-Offset': String(offset), 'Tus-Resumable': '1.0.0' }, body: file.slice(offset, end) })
            if (patch.status === 409) throw new Retryable('Checking saved upload position')
            if (patch.status !== 204 || Number(patch.headers.get('Upload-Offset')) !== end) throw new Error(`Server did not confirm the upload chunk (HTTP ${patch.status})`)
            options.onProgress(end)
          } else {
            const saved = await metadata(stage, true)
            checksum = saved?.checksums?.sha256
            if (saved?.size !== file.size || !checksum || !/^[a-f0-9]{64}$/i.test(checksum)) throw new Error('Could not verify the completed upload')
          }
        }
        retry = 0
      } catch (error) {
        if (!(error instanceof Retryable)) throw error
        options.onPaused?.(true)
        await waitForConnection(options.signal, Math.min(20_000, 1000 * 2 ** Math.min(retry++, 5)))
      }
    }
    options.onProgress(file.size)
    options.onPaused?.(false)
  } finally {
    if (!finished) {
      // Never delete the destination. TUS deletion also removes its cache entry;
      // resource deletion covers an already-completed staging file. Offline
      // partial sessions are subsequently removed by File Browser's expiry.
      const cleanup = new AbortController()
      const timer = setTimeout(() => cleanup.abort(), 5000)
      try {
        const deleted = await request(tus, { method: 'DELETE' }, cleanup.signal)
        if (deleted.status === 404) await request(resource(stage), { method: 'DELETE' }, cleanup.signal)
      } catch { /* original error remains the user-visible result */ }
      finally { clearTimeout(timer) }
    }
  }
}
