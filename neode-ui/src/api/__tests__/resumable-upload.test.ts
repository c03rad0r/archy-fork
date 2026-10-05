import { afterEach, describe, expect, it, vi } from 'vitest'
import { resumableUpload } from '../resumable-upload'

const MiB = 1024 * 1024
const hash = 'a'.repeat(64)
function fixture(size = 5 * MiB, name = 'resume.txt') {
  const file = new File([new Uint8Array(size)], name)
  const controller = new AbortController()
  const progress: number[] = []
  const paused: boolean[] = []
  let stage = false, active = false, bytes = 0, target = false
  const patches: number[] = []
  const methods: string[] = []
  let hook: ((url: URL, init: RequestInit) => Response | void | Promise<Response | void>) | undefined
  const response = (status: number, data?: unknown, headers?: Record<string, string>) => new Response(data === undefined ? null : JSON.stringify(data), { status, headers: { ...(data ? { 'content-type': 'application/json' } : {}), ...headers } })
  const transport = vi.fn(async (input: string, init: RequestInit = {}) => {
    const url = new URL(input)
    methods.push(init.method || 'GET')
    const special = await hook?.(url, init)
    if (special) return special
    if (url.pathname.includes('/api/tus/')) {
      if (init.method === 'HEAD') return active ? response(200, undefined, { 'Upload-Offset': String(bytes), 'Upload-Length': String(size) }) : response(404)
      if (init.method === 'POST') {
        expect(url.searchParams.get('override')).not.toBe('true')
        if (stage) return response(409)
        stage = active = true; return response(201)
      }
      if (init.method === 'PATCH') {
        const offset = Number((init.headers as Record<string, string>)['Upload-Offset'])
        patches.push(offset)
        if (!active) return response(404)
        if (offset !== bytes) return response(409)
        bytes += (init.body as Blob).size
        if (bytes === size) active = false
        return response(204, undefined, { 'Upload-Offset': String(bytes) })
      }
      if (init.method === 'DELETE') { if (!active) return response(404); stage = active = false; return response(204) }
    }
    if (init.method === 'PATCH') { expect(decodeURIComponent(url.searchParams.get('destination')!)).toBe('/folder/' + name); target = true; stage = false; return response(200) }
    if (init.method === 'DELETE') { expect(url.pathname).toContain('.archy-upload-'); stage = false; return response(200) }
    if (url.pathname.includes('.archy-upload-') ? stage : target) return response(200, { size: bytes, isDir: false, checksums: { sha256: hash } })
    return response(404)
  })
  return {
    file, controller, progress, paused, patches, methods, transport, response,
    setHook: (fn: typeof hook) => { hook = fn },
    createStage: () => { stage = active = true },
    setBytes: (n: number) => { bytes = n },
    finishStage: () => { bytes = size; active = false },
    rename: () => { target = true; stage = false },
    hasStage: () => stage, hasTarget: () => target,
    run: () => resumableUpload('https://node/app/filebrowser', '/folder', file, transport, { signal: controller.signal, onProgress: n => progress.push(n), onPaused: p => paused.push(p) }),
  }
}
afterEach(() => vi.useRealTimers())
describe('resumable Cloud upload', () => {
  it('uploads bounded chunks and publishes only complete verified bytes', async () => {
    const f = fixture(); await f.run()
    expect(f.patches).toEqual([0, 2 * MiB, 4 * MiB])
    expect(f.hasStage()).toBe(false); expect(f.hasTarget()).toBe(true)
    expect(f.progress[f.progress.length - 1]).toBe(f.file.size)
  })
  it('resumes after partial network write at the server offset, never truncating', async () => {
    vi.useFakeTimers(); const f = fixture(); let lost = false
    f.setHook(async (url, init) => {
      if (url.pathname.includes('/api/tus/') && init.method === 'PATCH' && !lost) {
        lost = true; f.setBytes(123456); throw new TypeError('Network lost')
      }
    })
    const run = f.run(); await vi.runAllTimersAsync(); await run
    expect(f.patches[0]).toBe(123456)
    expect(f.methods.filter(m => m === 'POST')).toHaveLength(1)
    expect(f.paused).toContain(true); expect(f.paused[f.paused.length - 1]).toBe(false)
  })
  it('recovers a lost final chunk response after TUS completion removes the session', async () => {
    vi.useFakeTimers(); const f = fixture(1024); let lost = false
    f.setHook(async (url, init) => {
      if (url.pathname.includes('/api/tus/') && init.method === 'PATCH' && !lost) {
        lost = true; f.finishStage(); throw new TypeError('Reply lost')
      }
    })
    const run = f.run(); await vi.runAllTimersAsync(); await run
    expect(f.methods.filter(m => m === 'POST')).toHaveLength(1); expect(f.hasTarget()).toBe(true)
  })
  it('confirms a lost rename response using exact checksum', async () => {
    vi.useFakeTimers(); const f = fixture(1024); let lost = false
    f.setHook(async (url, init) => {
      if (url.pathname.includes('/api/resources/') && init.method === 'PATCH' && !lost) {
        lost = true; f.rename(); throw new TypeError('Reply lost')
      }
    })
    const run = f.run(); await vi.runAllTimersAsync(); await run
    expect(f.hasTarget()).toBe(true)
    expect(f.transport.mock.calls.some(([url]) => url.includes('resume.txt?checksum=sha256'))).toBe(true)
  })
  it('never accepts a same-size destination with different content after ambiguous rename', async () => {
    vi.useFakeTimers(); const f = fixture(1024)
    f.setHook(async (url, init) => {
      if (url.pathname.includes('/api/resources/') && init.method === 'PATCH') { f.rename(); throw new TypeError('Reply lost') }
      if (url.pathname.endsWith('/resume.txt')) return f.response(200, { size: 1024, checksums: { sha256: 'b'.repeat(64) } })
    })
    const result = expect(f.run()).rejects.toThrow('Cannot confirm'); await vi.runAllTimersAsync(); await result
  })
  it('reconciles a lost creation reply without creating or truncating a second upload', async () => {
    vi.useFakeTimers(); const f = fixture(1024); let created = false
    f.setHook(async (_, init) => {
      if (init.method === 'POST' && !created) { created = true; f.createStage(); throw new TypeError('Creation reply lost') }
    })
    const run = f.run(); await vi.runAllTimersAsync(); await run
    expect(f.hasTarget()).toBe(true); expect(f.patches).toEqual([0]); expect(f.methods.filter(m => m === 'POST')).toHaveLength(1)
  })
  it('does not silently restart an expired partially saved session', async () => {
    const f = fixture(); let patched = false
    f.setHook(async (_, init) => {
      if (init.method === 'PATCH') patched = true
      if (init.method === 'HEAD' && patched) return f.response(404)
    })
    await expect(f.run()).rejects.toThrow('session expired')
    expect(f.methods.filter(m => m === 'POST')).toHaveLength(1)
    expect(f.hasTarget()).toBe(false)
  })
  it('wakes immediately when the network returns and cleans up wake listeners', async () => {
    vi.useFakeTimers(); const f = fixture(1024); let offline = true
    f.setHook(async (_, init) => { if (init.method === 'PATCH' && offline) throw new TypeError('Offline') })
    const run = f.run(); await vi.advanceTimersByTimeAsync(0)
    expect(f.paused).toContain(true)
    offline = false; window.dispatchEvent(new Event('online'))
    await vi.advanceTimersByTimeAsync(0); await run
    expect(vi.getTimerCount()).toBe(0); expect(f.hasTarget()).toBe(true)
  })
  it('handles empty files and encoded filenames', async () => {
    const f = fixture(0, 'a + 100% ? ü.txt'); await f.run(); expect(f.hasTarget()).toBe(true); expect(f.patches).toEqual([])
  })
  it.each([401, 403, 507])('stops on permanent HTTP %s without endless retry', async status => {
    const f = fixture(); f.setHook(async () => f.response(status))
    await expect(f.run()).rejects.toThrow(status === 507 ? 'storage' : 'access denied')
    expect(f.paused).not.toContain(true)
  })
  it('rejects HTML login interception', async () => {
    const f = fixture(); f.setHook(async () => new Response('<html>', { headers: { 'content-type': 'text/html' } }))
    await expect(f.run()).rejects.toThrow('working File Browser')
  })
  it('rejects corrupt server offsets', async () => {
    const f = fixture(); f.setHook(async (_, init) => init.method === 'HEAD' ? f.response(200, undefined, { 'Upload-Offset': '-1', 'Upload-Length': String(f.file.size) }) : undefined)
    await expect(f.run()).rejects.toThrow('invalid upload position')
  })
  it('cancels during retry and removes only its staging file', async () => {
    const f = fixture(); f.setHook(async (_, init) => {
      if (init.method === 'PATCH') { queueMicrotask(() => f.controller.abort()); throw new TypeError('Offline') }
    })
    await expect(f.run()).rejects.toMatchObject({ name: 'AbortError' })
    expect(f.hasStage()).toBe(false); expect(f.hasTarget()).toBe(false)
  })
})
