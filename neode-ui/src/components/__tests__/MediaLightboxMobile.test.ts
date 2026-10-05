import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import MediaLightbox from '../cloud/MediaLightbox.vue'
import type { FileBrowserItem } from '../../api/filebrowser-client'
const items = ['first.jpg', 'second.jpg', 'clip.mp4'].map(name => ({ name, path: '/' + name, isDir: false }) as FileBrowserItem)
const mounted: ReturnType<typeof mount>[] = []
async function viewer(extra = {}) {
  const w = mount(MediaLightbox, { props: { items, startIndex: 0, show: true, fetchBlobUrl: vi.fn(async (p: string) => 'blob:' + p), ...extra }, attachTo: document.body })
  mounted.push(w); await flushPromises(); return w
}
const button = (name: string) => document.querySelector(`[aria-label="${name}"]`) as HTMLButtonElement
beforeEach(() => { URL.revokeObjectURL = vi.fn() })
afterEach(() => { mounted.forEach(w => w.unmount()); mounted.length = 0; document.body.innerHTML = ''; vi.restoreAllMocks() })
describe('mobile media viewer', () => {
  it('requests fullscreen on the whole viewer for photos and exits it', async () => {
    await viewer()
    const backdrop = document.querySelector('.lightbox-backdrop') as HTMLElement
    const request = vi.fn(async () => {})
    backdrop.requestFullscreen = request
    button('Fullscreen').click(); await flushPromises()
    expect(request).toHaveBeenCalledOnce()
    expect(button('Exit fullscreen')).toBeTruthy()
    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: backdrop })
    document.exitFullscreen = vi.fn(async () => {})
    button('Exit fullscreen').click(); await flushPromises()
    expect(document.exitFullscreen).toHaveBeenCalledOnce()
    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: null })
  })
  it('offers an exit when an embedded browser denies native fullscreen', async () => {
    await viewer()
    const backdrop = document.querySelector('.lightbox-backdrop') as HTMLElement
    backdrop.requestFullscreen = vi.fn(async () => { throw new Error('Denied') })
    button('Fullscreen').click(); await flushPromises()
    expect(backdrop.classList.contains('lightbox-expanded')).toBe(true)
    button('Exit fullscreen').click(); await flushPromises()
    expect(backdrop.classList.contains('lightbox-expanded')).toBe(false)
  })
  it('uses the Safari video fullscreen fallback', async () => {
    await viewer({ startIndex: 2 })
    const video = document.querySelector('video')! as HTMLVideoElement & { webkitEnterFullscreen: () => void }
    video.webkitEnterFullscreen = vi.fn()
    button('Fullscreen').click(); await flushPromises()
    expect(video.webkitEnterFullscreen).toHaveBeenCalledOnce()
    expect(video.hasAttribute('playsinline')).toBe(true)
  })
  it('shows decode errors instead of leaving a broken video and can retry', async () => {
    const streamUrl = vi.fn(async () => 'https://example.test/clip.mp4')
    await viewer({ startIndex: 2, streamUrl })
    document.querySelector('video')!.dispatchEvent(new Event('error')); await flushPromises()
    expect(document.querySelector('video')).toBeNull()
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Unable to load')
    ;(document.querySelector('.lightbox-retry') as HTMLButtonElement).click(); await flushPromises()
    expect(streamUrl).toHaveBeenCalledTimes(2)
    expect(document.querySelector('video')).toBeTruthy()
  })
  it('ignores a late fetch from the previous photo and revokes its blob', async () => {
    let resolveFirst!: (url: string) => void
    const fetchBlobUrl = vi.fn((path: string) => path === '/first.jpg' ? new Promise<string>(resolve => { resolveFirst = resolve }) : Promise.resolve('blob:second'))
    await viewer({ fetchBlobUrl })
    button('Next file').click(); await flushPromises()
    resolveFirst('blob:stale'); await flushPromises()
    expect(document.querySelector('img')?.getAttribute('src')).toBe('blob:second')
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:stale')
  })
  it('restores focus and page scrolling after closing', async () => {
    const origin = document.createElement('button'); document.body.append(origin); origin.focus()
    document.body.style.overflow = 'auto'
    const w = await viewer()
    expect(document.body.style.overflow).toBe('hidden')
    w.unmount(); mounted.splice(mounted.indexOf(w), 1)
    expect(document.activeElement).toBe(origin)
    expect(document.body.style.overflow).toBe('auto')
  })
})
