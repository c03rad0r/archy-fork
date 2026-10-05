<template>
  <Teleport to="body">
    <Transition name="lightbox-fade">
      <div
        v-if="show"
        class="lightbox-backdrop"
        :class="{ 'lightbox-pip-handoff': pipHandoff, 'lightbox-expanded': expanded }"
        role="dialog" aria-modal="true" aria-label="Media viewer"
        @click.self="close"
        @keydown="onKeydown"
        tabindex="0"
        ref="backdropEl"
      >
        <!-- Top bar -->
        <div class="lightbox-topbar">
          <div class="lightbox-heading">
            <span v-if="mediaItems.length > 1" class="lightbox-count">
              {{ currentIndex + 1 }} / {{ mediaItems.length }}
            </span>
            <p class="lightbox-title" :title="currentItem?.name">{{ currentItem?.name }}</p>
          </div>
          <FileActionsMenu v-if="currentItem && (fileActions || downloadUrl)" :name="currentItem.name"
            :download-href="downloadUrl?.(currentItem.path)" :can-share="fileActions" :can-delete="fileActions"
            @share="emit('share', currentItem!); close()" @delete="emit('delete', currentItem!); close()" />
          <div class="lightbox-actions">
            <button class="lightbox-btn" :aria-label="expanded ? 'Exit fullscreen' : 'Fullscreen'"
              :title="expanded ? 'Exit fullscreen' : 'Fullscreen'" @click.stop="toggleFullscreen">
              <svg width="20" height="20" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                <path v-if="!expanded" d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                <path v-else d="M3 8h5V3m8 0v5h5M8 21v-5H3m13 5v-5h5" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
            <button
              v-if="pipAvailable && currentItem && isVideoFile(currentItem)"
              class="lightbox-btn"
              title="Picture-in-picture" aria-label="Picture-in-picture"
              @click.stop="togglePip(videoEl)"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <rect x="3" y="5" width="18" height="14" rx="2" stroke-width="2" />
                <rect x="12" y="12" width="7" height="5" rx="1" stroke-width="2" />
              </svg>
            </button>
            <button class="lightbox-btn" aria-label="Close viewer" title="Close" @click="close">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Navigation arrows -->
        <button
          v-if="mediaItems.length > 1"
          class="lightbox-nav lightbox-nav-prev" aria-label="Previous file"
          @click.stop="prev"
        >
          <svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <button
          v-if="mediaItems.length > 1"
          class="lightbox-nav lightbox-nav-next" aria-label="Next file"
          @click.stop="next"
        >
          <svg class="w-7 h-7" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <!-- Media content -->
        <div class="lightbox-content" @click.stop>
          <!-- Loading -->
          <div v-if="loading" class="flex items-center justify-center h-full">
            <div class="w-12 h-12 border-3 border-white/10 border-t-white/70 rounded-full animate-spin"></div>
          </div>

          <!-- Image -->
          <img
            v-else-if="!mediaError && currentItem && currentUrl && isImageFile(currentItem)"
            :src="currentUrl"
            :alt="currentItem.name"
            class="lightbox-media-img"
            @dblclick="toggleFullscreen"
            @error="onMediaError"
          />

          <!-- Video -->
          <video
            v-else-if="!mediaError && currentItem && currentUrl && isVideoFile(currentItem)"
            ref="videoEl"
            :src="currentUrl"
            :key="currentUrl"
            class="lightbox-media-video"
            controls
            playsinline
            autoplay
            @dblclick="toggleFullscreen"
            @error="onMediaError"
            @enterpictureinpicture="onEnterPip"
            @leavepictureinpicture="onLeavePip"
          />

          <!-- Audio -->
          <div
            v-else-if="!mediaError && currentItem && currentUrl && isAudioFile(currentItem)"
            class="lightbox-audio-container"
          >
            <div class="lightbox-audio-artwork">
              <svg class="w-20 h-20 text-orange-400/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
              </svg>
            </div>
            <p class="text-white/70 text-sm mt-4 truncate max-w-xs">{{ currentItem.name }}</p>
            <audio
              :src="currentUrl"
              :key="currentUrl"
              controls
              autoplay
              class="lightbox-audio-player"
              @error="onMediaError"
            />
          </div>

          <!-- Error -->
          <div v-else-if="mediaError" class="flex flex-col items-center justify-center gap-3">
            <svg class="w-12 h-12 text-white/20" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
            </svg>
            <p class="text-white/70 text-sm" role="alert">Unable to load this file.</p>
            <button class="lightbox-retry" @click="retryMedia">Try again</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted, nextTick } from 'vue'
import type { FileBrowserItem } from '@/api/filebrowser-client'
import { getFileCategory } from '@/composables/useFileType'
import { isPipSupported, togglePip } from '@/utils/pip'
import FileActionsMenu from './FileActionsMenu.vue'
import { usePipSession } from '@/composables/usePipSession'

const props = defineProps<{
  items: FileBrowserItem[]
  startIndex: number
  show: boolean
  fetchBlobUrl: (path: string) => Promise<string>
  streamUrl?: (path: string) => Promise<string>
  downloadUrl?: (path: string) => string
  fileActions?: boolean
}>()

const emit = defineEmits<{
  close: []
  share: [item: FileBrowserItem]
  delete: [item: FileBrowserItem]
}>()

const currentIndex = ref(0)
const loading = ref(false)
const mediaError = ref(false)
const currentUrl = ref<string | null>(null)
const backdropEl = ref<HTMLElement | null>(null)
const videoEl = ref<HTMLVideoElement | null>(null)

const expanded = ref(false)
let loadId = 0
let disposed = false
let previousFocus: HTMLElement | null = null
let previousOverflow: string | null = null

const pipAvailable = isPipSupported()
const pipSession = usePipSession()
const pipHandoff = ref(false)
// Bounded fallback for the transitionend-driven close below — covers a
// browser that skips the transition entirely (including the reduced-motion
// path, where the CSS transition duration is zero and transitionend never
// fires).
const PIP_HANDOFF_FALLBACK_MS = 350
let handoffTimer: ReturnType<typeof setTimeout> | null = null

const urlCache = new Map<string, string>()

const mediaItems = computed(() =>
  props.items.filter(item => {
    const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
    const cat = getFileCategory(ext, item.isDir)
    return cat === 'image' || cat === 'video' || cat === 'audio'
  })
)

const currentItem = computed(() => mediaItems.value[currentIndex.value] ?? null)

function isImageFile(item: FileBrowserItem): boolean {
  const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
  return getFileCategory(ext, false) === 'image'
}

function isVideoFile(item: FileBrowserItem): boolean {
  const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
  return getFileCategory(ext, false) === 'video'
}

function isAudioFile(item: FileBrowserItem): boolean {
  const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
  return getFileCategory(ext, false) === 'audio'
}

async function loadMedia(item: FileBrowserItem) {
  const requestId = ++loadId
  loading.value = true
  mediaError.value = false
  currentUrl.value = null
  try {
    let url = urlCache.get(item.path)
    if (!url) {
      url = (isVideoFile(item) || isAudioFile(item)) && props.streamUrl
        ? await props.streamUrl(item.path) : await props.fetchBlobUrl(item.path)
      if (disposed || requestId !== loadId) {
        if (url.startsWith('blob:')) URL.revokeObjectURL(url)
        return
      }
      urlCache.set(item.path, url)
    }
    if (requestId === loadId) currentUrl.value = url
  } catch {
    if (requestId === loadId) mediaError.value = true
  } finally {
    if (requestId === loadId) loading.value = false
  }
}

function retryMedia() {
  const item = currentItem.value
  if (!item) return
  const old = urlCache.get(item.path)
  if (old?.startsWith('blob:')) URL.revokeObjectURL(old)
  urlCache.delete(item.path)
  void loadMedia(item)
}

function prev() {
  const len = mediaItems.value.length
  if (len <= 1) return
  currentIndex.value = (currentIndex.value - 1 + len) % len
}

function next() {
  const len = mediaItems.value.length
  if (len <= 1) return
  currentIndex.value = (currentIndex.value + 1) % len
}

function close() {
  if (document.fullscreenElement === backdropEl.value) void document.exitFullscreen?.().catch(() => {})
  emit('close')
}

function finishPipHandoff() {
  backdropEl.value?.removeEventListener('transitionend', finishPipHandoff)
  if (handoffTimer) {
    clearTimeout(handoffTimer)
    handoffTimer = null
  }
  emit('close')
}

// Listens for the enterpictureinpicture event itself, rather than inferring
// from the button click, so PiP entered by any route (the browser's own
// control, a keyboard shortcut) behaves the same. Order matters here and
// must stay exactly this: adopt first, animate second, emit last — adopting
// first is what makes the element survive the unmount the emit triggers.
function onEnterPip() {
  const video = videoEl.value
  if (!video) return
  pipSession.adopt(video)
  pipHandoff.value = true
  backdropEl.value?.addEventListener('transitionend', finishPipHandoff, { once: true })
  handoffTimer = setTimeout(finishPipHandoff, PIP_HANDOFF_FALLBACK_MS)
}

// The session's own leavepictureinpicture listener (attached in
// usePipSession at adopt time) is the primary release path, since the
// lightbox has normally already unmounted by the time PiP is exited. This
// handler only covers the case where the lightbox is somehow still
// mounted; release() is idempotent so calling it twice is harmless.
function onLeavePip() {
  pipSession.release()
}

type SafariVideo = HTMLVideoElement & { webkitEnterFullscreen?: () => void }
async function toggleFullscreen() {
  if (document.fullscreenElement === backdropEl.value) {
    try { await document.exitFullscreen() } catch { /* Keep the exit control available. */ }
    return
  }
  if (expanded.value) { expanded.value = false; return }
  const el = backdropEl.value
  if (!el) return
  try {
    if (el.requestFullscreen) {
      await el.requestFullscreen()
      expanded.value = true
      return
    }
    const video = videoEl.value as SafariVideo | null
    if (video?.webkitEnterFullscreen) { video.webkitEnterFullscreen(); return }
  } catch { /* Embedded browsers may deny native fullscreen; expand in-place. */ }
  expanded.value = true
}

function fullscreenChanged() {
  expanded.value = document.fullscreenElement === backdropEl.value
}

function restorePage() {
  if (previousOverflow !== null) document.body.style.overflow = previousOverflow
  previousOverflow = null
  previousFocus?.focus()
  previousFocus = null
  document.removeEventListener('fullscreenchange', fullscreenChanged)
}

function onMediaError() {
  mediaError.value = true
  loading.value = false
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    if (expanded.value) void toggleFullscreen()
    else close()
  } else if (e.key === 'Tab') {
    const controls = Array.from(backdropEl.value?.querySelectorAll<HTMLElement>('button:not(:disabled), video[controls], audio[controls]') ?? [])
    const first = controls[0], last = controls[controls.length - 1]
    if (e.shiftKey && (document.activeElement === first || document.activeElement === backdropEl.value)) { e.preventDefault(); last?.focus() }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus() }
  } else if (!(e.target instanceof HTMLMediaElement)) {
    if (e.key === 'ArrowLeft') { e.preventDefault(); prev() }
    else if (e.key === 'ArrowRight') { e.preventDefault(); next() }
  }
}

watch(() => props.show, async (visible) => {
  if (visible) {
    pipHandoff.value = false
    expanded.value = false
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    document.addEventListener('fullscreenchange', fullscreenChanged)
    currentIndex.value = Math.max(0, Math.min(props.startIndex, mediaItems.value.length - 1))
    await nextTick()
    backdropEl.value?.focus()
  } else {
    ++loadId
    restorePage()
  }
}, { immediate: true })

watch([currentItem, () => props.show], ([item, visible]) => {
  if (visible && item) void loadMedia(item)
}, { immediate: true })

onUnmounted(() => {
  disposed = true
  ++loadId
  if (handoffTimer) clearTimeout(handoffTimer)
  restorePage()
  for (const url of urlCache.values()) {
    URL.revokeObjectURL(url)
  }
  urlCache.clear()
})
</script>

<style scoped>
.lightbox-backdrop {
  position: fixed;
  inset: 0;
  z-index: 4000;
  height: 100dvh;
  overflow: hidden;
  overscroll-behavior: contain;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.92);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  outline: none;
}

.lightbox-topbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 12px) max(16px, env(safe-area-inset-right, 0px)) 12px max(16px, env(safe-area-inset-left, 0px));
  background: linear-gradient(to bottom, rgba(0,0,0,0.6) 0%, transparent 100%);
  z-index: 10;
}

.lightbox-heading { flex: 1; display: flex; align-items: center; gap: 12px; min-width: 0; }
.lightbox-title { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: #eee; font-size: 14px; }
.lightbox-count { flex: none; font-size: 12px; color: #aaa; font-variant-numeric: tabular-nums; }
.lightbox-actions { display: flex; flex: none; gap: 8px; }
.lightbox-btn:focus-visible, .lightbox-nav:focus-visible, .lightbox-retry:focus-visible { outline: 2px solid white; outline-offset: 3px; }
.lightbox-retry { padding: 12px 20px; border-radius: 24px; background: #292929; color: white; }
.lightbox-btn {
  width: 44px;
  height: 44px;
  flex: 0 0 44px;
  padding: 0;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: rgba(255, 255, 255, 0.7);
  background: rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  transition: all 0.2s;
  cursor: pointer;
}
.lightbox-btn:hover {
  background: rgba(255, 255, 255, 0.15);
  color: white;
}

.lightbox-nav {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 3rem;
  height: 3rem;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: rgba(255, 255, 255, 0.5);
  background: rgba(255, 255, 255, 0.06);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.08);
  transition: all 0.2s;
  cursor: pointer;
  z-index: 10;
}
.lightbox-nav:hover {
  background: rgba(255, 255, 255, 0.12);
  color: white;
  border-color: rgba(255, 255, 255, 0.15);
}
.lightbox-nav-prev { left: 1rem; }
.lightbox-nav-next { right: 1rem; }

.lightbox-content {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  min-height: 0;
  box-sizing: border-box;
  padding: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 76px) 76px calc(var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)) + 16px);
}

.lightbox-media-img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: 0.5rem;
  box-shadow: 0 25px 60px rgba(0, 0, 0, 0.5);
}

.lightbox-media-video {
  width: 100%;
  height: auto;
  max-height: 100%;
  object-fit: contain;
  border-radius: 0;
  background: black;
  cursor: pointer;
}

.lightbox-audio-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1rem;
  padding: 2rem;
}

.lightbox-audio-artwork {
  width: 12rem;
  height: 12rem;
  border-radius: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, rgba(251, 146, 60, 0.12) 0%, rgba(251, 146, 60, 0.04) 100%);
  border: 1px solid rgba(251, 146, 60, 0.15);
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.3);
}

.lightbox-audio-player {
  width: 100%;
  max-width: 24rem;
  margin-top: 1rem;
  border-radius: 2rem;
  filter: invert(1) hue-rotate(180deg) brightness(0.85) contrast(0.9);
}

/* Transitions */
.lightbox-fade-enter-active,
.lightbox-fade-leave-active {
  transition: opacity 0.25s ease;
}
.lightbox-fade-enter-from,
.lightbox-fade-leave-to {
  opacity: 0;
}

/* Picture-in-picture handoff (UIFIX-05): reads as the video moving into the
   PiP window rather than a dismissal. The backdrop's blur/opacity fall away
   while the content scales down and drifts toward the bottom-right corner
   most browsers default the PiP window to — a best-effort convention, since
   the actual corner is browser- and user-controlled. Applied only on the
   PiP path; a normal close (button or Escape) never gets this class. */
.lightbox-pip-handoff {
  transition: opacity 0.3s ease, backdrop-filter 0.3s ease;
  opacity: 0;
  backdrop-filter: blur(0px);
  -webkit-backdrop-filter: blur(0px);
}
.lightbox-pip-handoff .lightbox-topbar,
.lightbox-pip-handoff .lightbox-nav {
  transition: opacity 0.3s ease;
  opacity: 0;
}
.lightbox-pip-handoff .lightbox-content {
  transition: transform 0.3s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.3s ease;
  transform: scale(0.4) translate(40vw, 40vh);
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .lightbox-pip-handoff,
  .lightbox-pip-handoff .lightbox-topbar,
  .lightbox-pip-handoff .lightbox-nav,
  .lightbox-pip-handoff .lightbox-content {
    transition: none;
  }
}

/* Mobile controls have their own safe-area row, never covering video controls. */
@media (max-width: 768px), (max-height: 500px) and (pointer: coarse) {
  .lightbox-topbar { padding-bottom: 8px; }
  .lightbox-heading { flex: 1; }
  .lightbox-actions {
    position: fixed;
    bottom: calc(var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)) + 12px);
    left: 50%;
    transform: translateX(-50%);
    gap: 8px;
  }
  .lightbox-content {
    padding: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 48px) max(8px, env(safe-area-inset-right, 0px)) calc(var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)) + 72px) max(8px, env(safe-area-inset-left, 0px));
  }
  .lightbox-nav {
    top: auto;
    bottom: calc(var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)) + 12px);
    transform: none;
    width: 44px; height: 44px; padding: 0;
  }
  .lightbox-nav-prev { left: max(12px, env(safe-area-inset-left, 0px)); }
  .lightbox-nav-next { right: max(12px, env(safe-area-inset-right, 0px)); }
  .lightbox-audio-artwork { width: 8rem; height: 8rem; }
}
.lightbox-expanded .lightbox-content { padding-top: var(--safe-area-top, env(safe-area-inset-top, 0px)); }
.lightbox-expanded .lightbox-heading { visibility: hidden; }
@media (prefers-reduced-motion: reduce) {
  .lightbox-fade-enter-active, .lightbox-fade-leave-active { transition: none; }
}
</style>
