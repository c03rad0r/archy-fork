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

/** Create the Audio element and attach listeners once */
function init() {
  if (initialized) return
  initialized = true
  audio.value = new Audio()
  audio.value.addEventListener('timeupdate', () => {
    currentTime.value = audio.value?.currentTime ?? 0
  })
  audio.value.addEventListener('loadedmetadata', () => {
    duration.value = audio.value?.duration ?? 0
    error.value = null
  })
  // Buffering / connecting over mesh|Tor → show a loader until it can play.
  audio.value.addEventListener('loadstart', () => {
    loading.value = true
  })
  audio.value.addEventListener('waiting', () => {
    loading.value = true
  })
  audio.value.addEventListener('canplay', () => {
    loading.value = false
  })
  audio.value.addEventListener('playing', () => {
    loading.value = false
  })
  audio.value.addEventListener('ended', () => {
    playing.value = false
    loading.value = false
  })
  audio.value.addEventListener('pause', () => {
    playing.value = false
  })
  audio.value.addEventListener('play', () => {
    playing.value = true
    error.value = null
  })
  audio.value.addEventListener('error', () => {
    playing.value = false
    loading.value = false
    error.value = 'Could not play this audio file. The peer may be offline, or the file may be unavailable.'
  })
}

function play(src: string, name: string) {
  init()
  error.value = null

  if (currentSrc.value === src && playing.value) {
    audio.value!.pause()
    return
  }

  if (currentSrc.value !== src) {
    loading.value = true
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
    playing.value = false
    loading.value = false
  })
}

function pause() {
  audio.value?.pause()
}

function seek(time: number) {
  if (audio.value) {
    audio.value.currentTime = time
  }
}

function stop() {
  if (audio.value) {
    audio.value.pause()
    audio.value.currentTime = 0
  }
  playing.value = false
  currentSrc.value = null
  currentName.value = ''
}

const progress = computed(() => {
  if (duration.value === 0) return 0
  return (currentTime.value / duration.value) * 100
})

export function useAudioPlayer() {
  return {
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
