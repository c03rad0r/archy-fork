import { onBeforeUnmount, ref, type Ref } from 'vue'
interface NativeVideoBridge { postMessage(message: string): void; onmessage: ((event: { data: string }) => void) | null }
type NativeWindow = Window & { ArchipelagoCloudVideo?: NativeVideoBridge; ArchipelagoNative?: unknown }
interface Channel { call(action: string, values?: Record<string, unknown>, id?: string): Promise<Record<string, unknown>>; subscribe(listener: (event: Record<string, unknown>) => void): () => void }
const channels = new WeakMap<NativeVideoBridge, Channel>()
function identifier() {
  const bytes = crypto.getRandomValues(new Uint8Array(16)); bytes[6] = (bytes[6]! & 15) | 64; bytes[8] = (bytes[8]! & 63) | 128
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0,8)}-${hex.slice(8,12)}-${hex.slice(12,16)}-${hex.slice(16,20)}-${hex.slice(20)}`
}
function nativeChannel(): Channel | null {
  const bridge = (window as NativeWindow).ArchipelagoCloudVideo
  if (!bridge || typeof bridge.postMessage !== 'function') return null
  const existing = channels.get(bridge); if (existing) return existing
  const waiting = new Map<string, {resolve(value: Record<string, unknown>): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout>}>()
  const listeners = new Set<(event: Record<string, unknown>) => void>()
  bridge.onmessage = event => {
    let data: Record<string, unknown>
    try { data = JSON.parse(event.data) as Record<string, unknown> } catch { return }
    if (!data || typeof data !== 'object') return
    if (data.type === 'event') { for (const listener of listeners) listener(data); return }
    if (typeof data.id !== 'string') return
    const call = waiting.get(data.id); if (!call) return
    waiting.delete(data.id); clearTimeout(call.timer)
    if (typeof data.error === 'string') call.reject(new Error(data.error)); else call.resolve(data)
  }
  const channel: Channel = {
    call(action, values = {}, id = identifier()) {
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { waiting.delete(id); reject(new Error('The companion did not confirm picture-in-picture. Return to the viewer and retry.')) }, 5000)
        waiting.set(id, {resolve,reject,timer})
        try { bridge.postMessage(JSON.stringify({id,action,...values})) }
        catch { waiting.delete(id); clearTimeout(timer); reject(new Error('Picture-in-picture is unavailable in this companion.')) }
      })
    },
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener) },
  }
  channels.set(bridge,channel); return channel
}
/** Keeps the original Cloud video and authenticated WebView. No stream address
 * crosses the native bridge and no second player is created. */
export function useCompanionVideoPip(video: Ref<HTMLVideoElement | null>) {
  const available = Boolean((window as NativeWindow).ArchipelagoCloudVideo || (window as NativeWindow).ArchipelagoNative)
  const active = ref(false), error = ref(''), busy = ref(false)
  let session: string | null = null, selected: HTMLVideoElement | null = null, generation = 0
  let unsubscribe: (() => void) | null = null
  const exitFullscreen = () => { if (document.fullscreenElement === selected) void document.exitFullscreen?.().catch(() => {}) }
  function publish() { if (session && selected) void nativeChannel()?.call('state',{session,playing:!selected.paused && !selected.ended}).catch(() => {}) }
  function clear() {
    const original = session; session = null; active.value = false; busy.value = false; ++generation
    exitFullscreen()
    selected?.removeEventListener('play',publish); selected?.removeEventListener('pause',publish)
    selected?.removeEventListener('ended',stop); selected?.removeEventListener('error',stop)
    unsubscribe?.(); unsubscribe = null; selected = null
    if (original) void nativeChannel()?.call('release',{session:original}).catch(() => {})
  }
  function stop() { selected?.pause(); clear() }
  async function enter() {
    if (busy.value || active.value) return
    const channel = nativeChannel(), target = video.value
    if (!channel || !target || typeof target.requestFullscreen !== 'function') { error.value = 'Picture-in-picture is unavailable in this companion or system WebView.'; return }
    if (!target.videoWidth || !target.videoHeight) { error.value = 'Wait for the video to load before opening picture-in-picture.'; return }
    clear(); const scope = generation
    session = identifier(); selected = target; error.value = ''; busy.value = true
    const original = session
    unsubscribe = channel.subscribe(event => {
      if (event.session !== session || selected !== target) return
      if (event.state === 'entered') active.value = true
      if (event.state === 'command') {
        if (event.command === 'pause') target.pause()
        else if (event.command === 'play') void target.play().catch(() => { error.value = 'Playback could not resume. Return to the viewer.' })
      }
      if (event.state === 'closed') stop()
      if (event.state === 'restored') clear()
    })
    for (const name of ['play','pause']) target.addEventListener(name,publish)
    target.addEventListener('ended',stop); target.addEventListener('error',stop)
    try {
      // Start fullscreen in the original user gesture; arm and fullscreen run
      // together. Native entry additionally requires the real custom view.
      const armed = channel.call('arm',{width:target.videoWidth,height:target.videoHeight,playing:!target.paused},original)
      let fullscreen: Promise<void>
      try { fullscreen = target.requestFullscreen() } catch (cause) { fullscreen = Promise.reject(cause) }
      await Promise.all([armed,fullscreen])
      if (generation !== scope || video.value !== target) return
      const result = await channel.call('enter',{session:original})
      if (generation !== scope) return
      if (result.active !== true) throw new Error('The companion did not enter picture-in-picture.')
      active.value = true
    } catch (cause) {
      if (generation === scope) { error.value = cause instanceof Error ? cause.message : 'Picture-in-picture is unavailable.'; clear() }
    } finally { if (generation === scope) busy.value = false }
  }
  onBeforeUnmount(stop)
  return {available,active,error,busy,enter,stop}
}
