import { onBeforeUnmount, watch } from 'vue'
import { useAudioPlayer } from './useAudioPlayer'

type AudioBridge = { postMessage: (message: string) => void; onmessage: ((event: { data: string }) => void) | null }
type NativeWindow = Window & { ArchipelagoAudio?: AudioBridge }

/** LAN HTTP WebViews expose getRandomValues, but not secure-context randomUUID. */
function audioSessionId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = (bytes[6]! & 0x0f) | 0x40
  bytes[8] = (bytes[8]! & 0x3f) | 0x80
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

/** Send a small decoded thumbnail, never a protected URL or authorization data. */
async function thumbnail(url: string, signal: AbortSignal): Promise<string> {
  if (!url) return ''
  const source = new URL(url, window.location.href)
  if (source.username || source.password || (source.protocol !== 'https:' && source.origin !== window.location.origin)) return ''
  const response = await fetch(source.href, { signal, credentials: 'same-origin', referrerPolicy: 'no-referrer' })
  if (!response.ok || !response.headers.get('content-type')?.startsWith('image/')) return ''
  const reader = response.body?.getReader()
  if (!reader) return ''
  const chunks: Uint8Array[] = []
  let size = 0
  try {
    while (true) {
      const chunk = await reader.read()
      if (chunk.done) break
      size += chunk.value.byteLength
      if (size > 512 * 1024) { await reader.cancel(); return '' }
      chunks.push(chunk.value)
    }
  } finally { reader.releaseLock() }
  const bytes = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length }
  const picture = await createImageBitmap(new Blob([bytes]), { resizeWidth: 192, resizeHeight: 192, resizeQuality: 'medium' })
  try {
    const canvas = document.createElement('canvas'); canvas.width = 192; canvas.height = 192
    const context = canvas.getContext('2d'); if (!context) return ''
    context.drawImage(picture, 0, 0, 192, 192)
    const result = canvas.toDataURL('image/jpeg', 0.75)
    return result.length <= 90000 ? result : ''
  } finally { picture.close() }
}

export function useCompanionAudio(player = useAudioPlayer()) {
  let bridge: AudioBridge | undefined
  let session = '', source = '', sequence = 0
  let artwork = '', sentArtwork = ''
  let imageRequest: AbortController | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let disposed = false
  let previousHandler: AudioBridge['onmessage'] = null
  function post(value: object) { try { bridge?.postMessage(JSON.stringify(value)) } catch { /* Lifecycle cleanup can close the document channel. */ } }
  function release() {
    if (session) post({ version: 1, action: 'release', session })
    session = ''; source = ''; artwork = ''; sentArtwork = ''
    imageRequest?.abort(); imageRequest = null
    if (timer) clearTimeout(timer)
    timer = undefined
  }
  function receive(event: { data: string }) {
    let value: any
    try { value = JSON.parse(event.data) } catch { return }
    if (value.version !== 1 || !session || value.session !== session) return
    if (value.type === 'error') {
      player.pause(); player.error.value = 'Background playback could not start. Reopen the companion and press Play.'
      release(); return
    }
    if (value.type !== 'command') return
    switch (value.command) {
      case 'play': if (player.currentSrc.value && !player.playing.value) player.play(player.currentSrc.value, player.currentName.value); break
      case 'pause': player.pause(); break
      case 'stop': player.stop(); release(); break
      case 'seek': if (Number.isFinite(value.position)) player.seek(Math.max(0, Math.min(value.position, player.duration.value))); break
      case 'next': player.nextExternal(); break
      case 'previous': player.previousExternal(); break
      case 'shuffle': player.shuffleExternal(); break
      case 'sync': publish(); break
    }
  }
  function connect() {
    const candidate = (window as NativeWindow).ArchipelagoAudio
    if (!candidate) return false
    if (bridge !== candidate) {
      release()
      if (bridge?.onmessage === receive) bridge.onmessage = previousHandler
      bridge = candidate; previousHandler = candidate.onmessage; candidate.onmessage = receive
    }
    return true
  }
  function publish() {
    if (disposed || !connect()) return
    if (!player.currentSrc.value || !player.currentName.value) { release(); return }
    if (!session || source !== player.currentSrc.value) {
      if (!player.playing.value) { release(); return }
      release(); session = audioSessionId(); source = player.currentSrc.value; sequence = 0
      loadArtwork()
    }
    const duration = Number.isFinite(player.duration.value) ? Math.max(0, Math.min(player.duration.value, 604800)) : 0
    const position = Number.isFinite(player.currentTime.value) ? Math.max(0, Math.min(player.currentTime.value, duration)) : 0
    const message: Record<string, unknown> = { version: 1, action: 'state', session, sequence: ++sequence,
      title: player.currentName.value.slice(0, 512), playing: player.playing.value, position, duration,
      previous: player.externalApp.value, next: player.externalApp.value,
      shuffle: player.externalApp.value, shuffled: player.externalShuffle.value }
    if (artwork !== sentArtwork || sequence === 1) { message.artwork = artwork; sentArtwork = artwork }
    post(message)
  }
  function loadArtwork() {
    imageRequest?.abort(); artwork = ''
    if (!session || !player.externalArtwork.value) return
    const target = session; const request = new AbortController(); imageRequest = request
    void thumbnail(player.externalArtwork.value, request.signal).then(value => {
      if (!disposed && !request.signal.aborted && target === session) { artwork = value; publish() }
    }).catch(() => { /* Optional image failure never interrupts music. */ })
  }
  // Playback transitions/track changes are immediate; position updates are bounded.
  watch([player.currentSrc, player.currentName, player.playing, player.externalShuffle], publish, { immediate: true, flush: 'post' })
  watch([player.currentTime, player.duration], () => {
    if (!timer) timer = setTimeout(() => { timer = undefined; publish() }, 1000)
  })
  watch(player.externalArtwork, () => { loadArtwork(); publish() })
  const sync = () => publish()
  window.addEventListener('pageshow', sync)
  document.addEventListener('visibilitychange', sync)
  const heartbeat = setInterval(publish, 10000)
  onBeforeUnmount(() => {
    disposed = true; release(); clearInterval(heartbeat)
    window.removeEventListener('pageshow', sync); document.removeEventListener('visibilitychange', sync)
    if (bridge?.onmessage === receive) bridge.onmessage = previousHandler
  })
  return { sync }
}
