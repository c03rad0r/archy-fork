import { onBeforeUnmount, watch, type Ref } from 'vue'
import { appHasMediaBridge } from '@/views/discover/curatedApps'
import { useAudioPlayer, type ExternalAudioState } from './useAudioPlayer'
import { useAppStore } from '@/stores/app'
import { useAppLauncherStore } from '@/stores/appLauncher'

export function useAppMediaBridge(appId: Ref<string>, url: Ref<string>, frame: Ref<HTMLIFrameElement | null>, visible: Ref<boolean>) {
  const player = useAudioPlayer()
  const launcher = useAppLauncherStore()
  const app = useAppStore()
  const installedVersion = () => app.data?.['package-data']?.[appId.value]?.manifest?.version
  let admitted: { source: Window; origin: string; appId: string; version: string } | null = null
  const session = Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, '0')).join('')
  const id = `${appId.value}:${session}`
  const origin = () => { try { return new URL(url.value, window.location.origin).origin } catch { return '' } }
  function matchesAdmission() {
    return admitted !== null && admitted.source === frame.value?.contentWindow && admitted.origin === origin()
      && admitted.appId === appId.value && admitted.version === installedVersion()
  }
  function release() {
    // Catalog expiry/removal cannot revoke our ability to stop an already
    // admitted session. The old nonce/source/origin still constrain cleanup.
    admitted?.source.postMessage({type:'archipelago:media-control',version:1,session,command:'pause'}, admitted.origin)
    admitted = null
    player.releaseExternal(id)
  }
  function command(command: string, position?: number) {
    if (!matchesAdmission()) return
    admitted!.source.postMessage({ type: 'archipelago:media-control', version: 1, session, command, position }, admitted!.origin)
  }
  function connect() {
    if (admitted && !matchesAdmission()) release()
    if (!admitted) {
      const version = installedVersion(), source = frame.value?.contentWindow
      if (!version || !source || !origin() || !appHasMediaBridge(appId.value, version)) return
      admitted = {source,origin:origin(),appId:appId.value,version}
    }
    admitted.source.postMessage({ type: 'archipelago:media-connect', version: 1, session }, admitted.origin)
  }
  const controller = { id, play: () => command('play'), pause: () => command('pause'), seek: (position: number) => command('seek', position),
    next: () => command('next'), previous: () => command('previous'), shuffle: () => command('shuffle'),
    open: () => launcher.openSession(appId.value), close: () => {
      if (launcher.mediaAppId === appId.value && launcher.panelAppId !== appId.value) launcher.mediaAppId = null
    },
  }
  function handle(event: MessageEvent) {
    if (event.source !== frame.value?.contentWindow || event.origin !== origin()) return
    const data = event.data
    if (data?.version !== 1) return
    if (data.type === 'archipelago:media-ready') { connect(); return }
    if (!matchesAdmission() || data.type !== 'archipelago:media-state' || data.session !== session) return
    if (data.available !== true) { player.detachExternal(id); return }
    if (typeof data.title !== 'string' || typeof data.artist !== 'string' || typeof data.playing !== 'boolean'
      || !Number.isFinite(data.position) || !Number.isFinite(data.duration) || data.position < 0 || data.duration < 0) return
    let artwork = ''
    try {
      if (typeof data.artwork === 'string' && data.artwork.length <= 2048 && data.artwork) {
        const candidate = new URL(data.artwork, url.value)
        if ((candidate.protocol === 'https:' || candidate.origin === origin()) && !candidate.username && !candidate.password) artwork = candidate.href
      }
    } catch { /* Invalid optional artwork never prevents playback controls. */ }
    const state: ExternalAudioState = { title: data.title.slice(0, 256), artist: data.artist.slice(0, 256), playing: data.playing,
      position: Math.min(data.position, data.duration), duration: data.duration, artwork, shuffle: data.shuffle === true }
    player.updateExternal(controller, state, visible.value)
  }
  watch(visible, shown => player.setExternalVisible(id, shown))
  watch([appId, url, frame, installedVersion], () => { if (admitted && !matchesAdmission()) release() }, {flush:'sync'})
  onBeforeUnmount(release)
  return { connect, handle }
}
