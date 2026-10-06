import { ref, computed } from 'vue'

const audio = ref<HTMLAudioElement | null>(null)
const currentSrc = ref<string | null>(null)
const currentName = ref('')
const playing = ref(false)
const loading = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const error = ref<string | null>(null)
let initialized = false

export interface ExternalAudioState { title: string; artist?: string; playing: boolean; position: number; duration: number }
export interface ExternalAudioController {
  id: string
  play: () => void
  pause: () => void
  seek: (position: number) => void
  open: () => void
  close: () => void
}
let external: ExternalAudioController | null = null
const suppressedExternal = new Set<string>()
const externalVisible = ref(false)
const externalApp = ref(false)

function updateExternal(controller: ExternalAudioController, state: ExternalAudioState, visible: boolean) {
  if (suppressedExternal.has(controller.id)) { if (!state.playing) suppressedExternal.delete(controller.id); return }
  if (external?.id !== controller.id) {
    if (!state.playing) return
    audio.value?.pause()
    if (external) { suppressedExternal.add(external.id); external.pause() }
    external = controller
  }
  externalApp.value = true
  externalVisible.value = visible
  currentSrc.value = `app:${controller.id}`
  currentName.value = [state.title, state.artist].filter(Boolean).join(' · ')
  playing.value = state.playing
  currentTime.value = state.position
  duration.value = state.duration
  loading.value = false
  error.value = null
}
function detachExternal(id: string) {
  if (external?.id !== id) return
  external = null
  externalApp.value = false
  externalVisible.value = false
  currentSrc.value = null
  currentName.value = ''
  playing.value = false
  currentTime.value = 0
  duration.value = 0
  loading.value = false
  error.value = null
}
function releaseExternal(id: string) { detachExternal(id); suppressedExternal.delete(id) }
function setExternalVisible(id: string, visible: boolean) { if (external?.id === id) externalVisible.value = visible }
function openExternal() { external?.open() }

/** Create the Audio element and attach listeners once */
function init() {
  if (initialized) return
  initialized = true
  audio.value = new Audio()
  const listen = (event: string, callback: () => void) => audio.value!.addEventListener(event, () => { if (!external && currentSrc.value) callback() })
  listen('timeupdate', () => {
    currentTime.value = audio.value?.currentTime ?? 0
  })
  listen('loadedmetadata', () => {
    duration.value = audio.value?.duration ?? 0
    error.value = null
  })
  // Buffering / connecting over mesh|Tor → show a loader until it can play.
  listen('loadstart', () => {
    loading.value = true
  })
  listen('waiting', () => {
    loading.value = true
  })
  listen('canplay', () => {
    loading.value = false
  })
  listen('playing', () => {
    loading.value = false
  })
  listen('ended', () => {
    playing.value = false
    loading.value = false
  })
  listen('pause', () => {
    playing.value = false
  })
  listen('play', () => {
    playing.value = true
    error.value = null
  })
  listen('error', () => {
    playing.value = false
    loading.value = false
    error.value = 'Could not play this audio file. The peer may be offline, or the file may be unavailable.'
  })
}

function play(src: string, name: string) {
  if (external && currentSrc.value === src) { external.play(); return }
  if (external) { suppressedExternal.add(external.id); external.pause(); detachExternal(external.id) }
  init()
  error.value = null

  if (currentSrc.value === src && playing.value) {
    audio.value!.pause()
    return
  }

  if (currentSrc.value !== src) {
    loading.value = true
    currentTime.value = 0
    duration.value = 0
    audio.value!.src = src
    currentSrc.value = src
    currentName.value = name
  }

  // play() rejects (e.g. NotSupportedError when the peer 404s and there's no
  // decodable source) independently of the 'error' event above — uncaught,
  // this surfaces as a raw console error instead of the friendly message
  // already wired up there. The 'error' listener sets the same state, so
  // this just needs to stop the rejection from going unhandled.
  audio.value!.play().catch(() => {
    if (external || currentSrc.value !== src) return
    playing.value = false
    loading.value = false
  })
}

function pause() {
  if (external) { external.pause(); return }
  audio.value?.pause()
}

function seek(time: number) {
  if (external) { external.seek(time); return }
  if (audio.value) {
    audio.value.currentTime = time
  }
}

function stop() {
  if (external) { const current = external; suppressedExternal.add(current.id); current.pause(); detachExternal(current.id); current.close(); return }
  if (audio.value) {
    audio.value.pause()
    audio.value.currentTime = 0
  }
  playing.value = false
  currentSrc.value = null
  currentName.value = ''
  currentTime.value = 0
  duration.value = 0
  loading.value = false
  error.value = null
}

const progress = computed(() => {
  if (duration.value === 0) return 0
  return (currentTime.value / duration.value) * 100
})

export function useAudioPlayer() {
  return {
    updateExternal, detachExternal, releaseExternal, setExternalVisible, openExternal, externalVisible, externalApp,
    play,
    pause,
    seek,
    stop,
    playing,
    loading,
    currentName,
    currentTime,
    duration,
    progress,
    currentSrc,
    error,
  }
}
