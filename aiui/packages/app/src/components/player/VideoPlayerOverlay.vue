<template>
  <Teleport to="body">
    <Transition name="video-player">
      <div
        v-if="store.isOpen"
        ref="containerRef"
        class="fixed inset-0 z-[2500] flex flex-col bg-black"
        :class="controlsVisible ? '' : 'cursor-none'"
        @mousemove="showControls"
        @touchstart.passive="showControls"
        @keydown="onKeydown"
        @click="onContainerClick"
        tabindex="0"
      >
        <!-- Video area -->
        <div class="flex-1 min-h-0 relative flex items-center justify-center">
          <div
            ref="playerRef"
            class="w-full h-full"
          />

          <!-- Big center play/pause indicator (flashes on toggle) -->
          <Transition name="center-icon">
            <div
              v-if="showCenterIcon"
              class="absolute inset-0 flex items-center justify-center pointer-events-none"
            >
              <div class="w-20 h-20 rounded-full flex items-center justify-center bg-black/40 backdrop-blur-sm">
                <svg v-if="isPlaying" class="w-10 h-10 text-white" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
                </svg>
                <svg v-else class="w-10 h-10 text-white" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M8 5v14l11-7L8 5z" />
                </svg>
              </div>
            </div>
          </Transition>

          <!-- Loading spinner -->
          <div v-if="isBuffering" class="absolute inset-0 flex items-center justify-center pointer-events-none">
            <svg class="w-12 h-12 animate-spin text-white/60" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </div>
        </div>

        <!-- Controls overlay — bottom bar -->
        <div
          class="absolute bottom-0 left-0 right-0 transition-all duration-300 z-20"
          :class="controlsVisible ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-4'"
        >
          <!-- Gradient fade above controls -->
          <div class="h-24 bg-gradient-to-t from-black/80 to-transparent pointer-events-none" />

          <div class="path-glass-card !rounded-none px-4 py-3 space-y-2">
            <!-- Scrubber -->
            <div
              ref="scrubberRef"
              class="group w-full h-1.5 rounded-full cursor-pointer bg-white/15 transition-all hover:h-2.5"
              @click="onScrubberClick"
              @mousedown="onScrubberDragStart"
            >
              <div
                class="h-full bg-accent rounded-full transition-[width] duration-75 relative"
                :style="{ width: `${progress}%` }"
              >
                <div class="absolute right-0 top-1/2 -translate-y-1/2 w-3 h-3 rounded-full bg-white shadow-md opacity-0 group-hover:opacity-100 transition-opacity" />
              </div>
            </div>

            <!-- Controls row -->
            <div class="flex items-center gap-3">
              <!-- Title -->
              <div class="min-w-0 flex-1 max-w-[280px]">
                <p class="text-sm font-semibold truncate text-white/90">{{ store.title }}</p>
                <p class="text-xs truncate text-white/40">Free Documentary</p>
              </div>

              <!-- Center controls -->
              <div class="flex items-center gap-2 flex-1 justify-center">
                <!-- Rewind 10s -->
                <button
                  class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center text-white/70 hover:text-white/90 transition-all hover:scale-105 active:scale-95"
                  aria-label="Rewind 10 seconds"
                  @click.stop="seek(-10)"
                >
                  <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M12.5 8c-2.65 0-5.05.99-6.9 2.6L2 7v9h9l-3.62-3.62c1.39-1.16 3.16-1.88 5.12-1.88 3.54 0 6.55 2.31 7.6 5.5l2.37-.78C21.08 11.03 17.15 8 12.5 8z" />
                  </svg>
                </button>

                <!-- Play/Pause -->
                <button
                  class="w-12 h-12 rounded-full flex items-center justify-center glass-button shrink-0 transition-all hover:scale-105 active:scale-95"
                  aria-label="Play or pause"
                  @click.stop="toggle"
                >
                  <svg v-if="isPlaying" class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
                  </svg>
                  <svg v-else class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M8 5v14l11-7L8 5z" />
                  </svg>
                </button>

                <!-- Forward 10s -->
                <button
                  class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center text-white/70 hover:text-white/90 transition-all hover:scale-105 active:scale-95"
                  aria-label="Forward 10 seconds"
                  @click.stop="seek(10)"
                >
                  <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M18 13c0 3.31-2.69 6-6 6s-6-2.69-6-6 2.69-6 6-6v4l5-5-5-5v4c-4.42 0-8 3.58-8 8s3.58 8 8 8 8-3.58 8-8h-2z" />
                  </svg>
                </button>

                <!-- Time -->
                <span class="text-xs font-mono tabular-nums text-white/40 ml-1">
                  {{ formatTime(currentTime) }} / {{ formatTime(duration) }}
                </span>
              </div>

              <!-- Right controls -->
              <div class="flex items-center gap-1 shrink-0">
                <!-- Fullscreen -->
                <button
                  class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center text-white/60 hover:text-white/90 transition-colors"
                  aria-label="Toggle fullscreen"
                  @click.stop="toggleFullscreen"
                >
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
                  </svg>
                </button>

                <!-- Close -->
                <button
                  class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center text-white/60 hover:text-white/90 transition-colors"
                  aria-label="Close video player"
                  @click.stop="closePlayer"
                >
                  <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Top bar — close button (visible with controls) -->
        <div
          class="absolute top-0 left-0 right-0 transition-all duration-300 z-20"
          :class="controlsVisible ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-4'"
        >
          <div class="h-16 bg-gradient-to-b from-black/60 to-transparent flex items-start justify-end px-4 pt-3">
            <button
              class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center text-white/70 hover:text-white/90 hover:bg-white/10 transition-colors"
              aria-label="Close"
              @click.stop="closePlayer"
            >
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useVideoPlayerStore } from '@/stores/videoPlayer'

const store = useVideoPlayerStore()

const containerRef = ref<HTMLElement | null>(null)
const playerRef = ref<HTMLElement | null>(null)
const scrubberRef = ref<HTMLElement | null>(null)

const isPlaying = ref(false)
const isBuffering = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const progress = ref(0)
const controlsVisible = ref(true)
const showCenterIcon = ref(false)

let hideTimer: ReturnType<typeof setTimeout> | null = null
let rafId: number | null = null
let ytPlayer: any = null
let centerIconTimer: ReturnType<typeof setTimeout> | null = null

// YouTube IFrame API
let ytApiReady = false
const ytApiCallbacks: (() => void)[] = []

function loadYouTubeApi(): Promise<void> {
  if (ytApiReady) return Promise.resolve()
  return new Promise((resolve) => {
    if ((window as any).YT?.Player) {
      ytApiReady = true
      resolve()
      return
    }
    ytApiCallbacks.push(resolve)
    if (!document.getElementById('yt-iframe-api')) {
      const tag = document.createElement('script')
      tag.id = 'yt-iframe-api'
      tag.src = 'https://www.youtube.com/iframe_api'
      document.head.appendChild(tag)
      ;(window as any).onYouTubeIframeAPIReady = () => {
        ytApiReady = true
        ytApiCallbacks.forEach(cb => cb())
        ytApiCallbacks.length = 0
      }
    }
  })
}

function extractYouTubeId(url: string): string | null {
  // Handle embed URLs: youtube.com/embed/VIDEO_ID
  const embedMatch = url.match(/\/embed\/([a-zA-Z0-9_-]{11})/)
  if (embedMatch) return embedMatch[1]
  // Handle watch URLs: youtube.com/watch?v=VIDEO_ID
  const watchMatch = url.match(/[?&]v=([a-zA-Z0-9_-]{11})/)
  if (watchMatch) return watchMatch[1]
  // Handle youtu.be/VIDEO_ID
  const shortMatch = url.match(/youtu\.be\/([a-zA-Z0-9_-]{11})/)
  if (shortMatch) return shortMatch[1]
  return null
}

async function initPlayer(url: string) {
  const videoId = extractYouTubeId(url)
  if (!videoId || !playerRef.value) return

  isBuffering.value = true
  await loadYouTubeApi()

  const YT = (window as any).YT

  // Create a div target inside playerRef
  const target = document.createElement('div')
  target.id = 'yt-video-player'
  playerRef.value.innerHTML = ''
  playerRef.value.appendChild(target)

  ytPlayer = new YT.Player('yt-video-player', {
    videoId,
    width: '100%',
    height: '100%',
    playerVars: {
      autoplay: 1,
      controls: 0,
      modestbranding: 1,
      rel: 0,
      showinfo: 0,
      iv_load_policy: 3,
      fs: 0,
      playsinline: 1,
    },
    events: {
      onReady: () => {
        isBuffering.value = false
        isPlaying.value = true
        startPolling()
        showControls()
      },
      onStateChange: (e: any) => {
        const state = e.data
        // -1: unstarted, 0: ended, 1: playing, 2: paused, 3: buffering, 5: cued
        isPlaying.value = state === 1
        isBuffering.value = state === 3
        if (state === 0) {
          // Video ended
          isPlaying.value = false
          controlsVisible.value = true
        }
      },
    },
  })
}

function startPolling() {
  if (rafId) cancelAnimationFrame(rafId)
  function poll() {
    if (ytPlayer?.getCurrentTime && ytPlayer?.getDuration) {
      currentTime.value = ytPlayer.getCurrentTime() ?? 0
      duration.value = ytPlayer.getDuration() ?? 0
      progress.value = duration.value > 0 ? (currentTime.value / duration.value) * 100 : 0
    }
    rafId = requestAnimationFrame(poll)
  }
  poll()
}

function stopPolling() {
  if (rafId) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
}

function destroyPlayer() {
  stopPolling()
  if (ytPlayer?.destroy) {
    try { ytPlayer.destroy() } catch {}
  }
  ytPlayer = null
  if (playerRef.value) playerRef.value.innerHTML = ''
  isPlaying.value = false
  isBuffering.value = false
  currentTime.value = 0
  duration.value = 0
  progress.value = 0
}

// Controls
function toggle() {
  if (!ytPlayer) return
  if (isPlaying.value) {
    ytPlayer.pauseVideo()
  } else {
    ytPlayer.playVideo()
  }
  flashCenterIcon()
}

function seek(seconds: number) {
  if (!ytPlayer?.seekTo) return
  const target = Math.max(0, Math.min(duration.value, currentTime.value + seconds))
  ytPlayer.seekTo(target, true)
  showControls()
}

function seekToPercent(percent: number) {
  if (!ytPlayer?.seekTo || duration.value <= 0) return
  const target = (percent / 100) * duration.value
  ytPlayer.seekTo(target, true)
}

function onScrubberClick(e: MouseEvent) {
  if (!scrubberRef.value) return
  const rect = scrubberRef.value.getBoundingClientRect()
  const percent = Math.max(0, Math.min(100, ((e.clientX - rect.left) / rect.width) * 100))
  seekToPercent(percent)
}

let isDragging = false

function onScrubberDragStart(e: MouseEvent) {
  isDragging = true
  onScrubberClick(e)
  const onMove = (ev: MouseEvent) => { if (isDragging) onScrubberClick(ev) }
  const onUp = () => {
    isDragging = false
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

function toggleFullscreen() {
  if (!containerRef.value) return
  if (document.fullscreenElement) {
    document.exitFullscreen()
  } else {
    containerRef.value.requestFullscreen()
  }
}

function closePlayer() {
  destroyPlayer()
  store.close()
}

function flashCenterIcon() {
  showCenterIcon.value = true
  if (centerIconTimer) clearTimeout(centerIconTimer)
  centerIconTimer = setTimeout(() => {
    showCenterIcon.value = false
  }, 600)
}

function onContainerClick(e: MouseEvent) {
  // Only toggle on direct click on video area (not controls)
  const target = e.target as HTMLElement
  if (target === containerRef.value || target.closest('.flex-1.min-h-0')) {
    toggle()
  }
}

// Auto-hide controls
function showControls() {
  controlsVisible.value = true
  resetHideTimer()
}

function resetHideTimer() {
  if (hideTimer) clearTimeout(hideTimer)
  hideTimer = setTimeout(() => {
    if (isPlaying.value) controlsVisible.value = false
  }, 3000)
}

// Keyboard shortcuts
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    closePlayer()
    return
  }
  if (e.key === ' ' || e.key === 'k') {
    e.preventDefault()
    toggle()
    showControls()
    return
  }
  if (e.key === 'ArrowLeft') {
    e.preventDefault()
    seek(-10)
    return
  }
  if (e.key === 'ArrowRight') {
    e.preventDefault()
    seek(10)
    return
  }
  if (e.key === 'f') {
    e.preventDefault()
    toggleFullscreen()
    return
  }
}

function formatTime(seconds: number): string {
  const s = Math.floor(seconds)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = s % 60
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
  return `${m}:${String(sec).padStart(2, '0')}`
}

// Global escape handler (captures even when focus is elsewhere)
function onGlobalKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && store.isOpen) {
    e.preventDefault()
    e.stopPropagation()
    closePlayer()
  }
}

// Watch store open/close
watch(() => store.isOpen, async (open) => {
  if (open) {
    await nextTick()
    containerRef.value?.focus()
    initPlayer(store.videoUrl)
    showControls()
  } else {
    destroyPlayer()
  }
})

onMounted(() => {
  window.addEventListener('keydown', onGlobalKeydown, true)
})

onBeforeUnmount(() => {
  destroyPlayer()
  window.removeEventListener('keydown', onGlobalKeydown, true)
  if (hideTimer) clearTimeout(hideTimer)
  if (centerIconTimer) clearTimeout(centerIconTimer)
})
</script>

<style scoped>
.video-player-enter-active,
.video-player-leave-active {
  transition: opacity 0.25s ease;
}
.video-player-enter-from,
.video-player-leave-to {
  opacity: 0;
}

.center-icon-enter-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.center-icon-leave-active {
  transition: opacity 0.4s ease, transform 0.4s ease;
}
.center-icon-enter-from {
  opacity: 0;
  transform: scale(0.8);
}
.center-icon-leave-to {
  opacity: 0;
  transform: scale(1.3);
}

/* Make YouTube iframe fill container */
:deep(iframe) {
  width: 100% !important;
  height: 100% !important;
  border: 0;
  pointer-events: none;
}
</style>
