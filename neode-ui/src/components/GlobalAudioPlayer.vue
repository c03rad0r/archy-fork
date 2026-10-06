<template>
  <Teleport to="body">
    <Transition name="slide-up">
      <div
        v-if="audioPlayer.currentName.value && !audioPlayer.externalVisible.value"
        ref="barEl"
        class="fixed left-0 right-0 z-40 audio-player-bar"
      >
        <!-- Progress bar (clickable) -->
        <div
          class="h-1 bg-white/10 cursor-pointer"
          @click="onProgressClick"
        >
          <div
            class="h-full bg-orange-500 transition-all duration-200"
            :style="{ width: audioPlayer.progress.value + '%' }"
          />
        </div>

        <div class="flex items-center gap-3 px-4 py-2.5">
          <!-- Play/Pause -->
          <button
            class="flex-shrink-0 w-9 h-9 rounded-full bg-white/10 hover:bg-white/20 flex items-center justify-center transition-colors"
            @click="togglePlay"
          >
            <svg v-if="audioPlayer.loading.value" class="w-5 h-5 animate-spin text-white" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.4 0 0 5.4 0 12h4z" />
            </svg>
            <svg v-else-if="!audioPlayer.playing.value" class="w-5 h-5 text-white ml-0.5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7L8 5z" />
            </svg>
            <svg v-else class="w-5 h-5 text-white" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
            </svg>
          </button>

          <!-- Track info -->
          <div class="flex-1 min-w-0">
            <p v-if="audioPlayer.error.value" class="text-sm text-red-400 truncate">{{ audioPlayer.error.value }}</p>
            <p v-else class="text-sm font-medium text-white/90 truncate">{{ audioPlayer.currentName.value }}</p>
            <p class="text-xs text-white/40">{{ formatTime(audioPlayer.currentTime.value) }} / {{ formatTime(audioPlayer.duration.value) }}</p>
          </div>

          <button v-if="audioPlayer.externalApp.value" class="glass-button px-3 py-2 rounded-lg text-sm text-white shrink-0" @click="audioPlayer.openExternal()" aria-label="Open playing app">Open app</button>

          <!-- Close -->
          <button
            class="flex-shrink-0 w-8 h-8 rounded-full hover:bg-white/10 flex items-center justify-center transition-colors"
            @click="audioPlayer.stop()"
          >
            <svg class="w-4 h-4 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, onBeforeUnmount } from 'vue'
import { useAudioPlayer } from '@/composables/useAudioPlayer'

const audioPlayer = useAudioPlayer()
const barEl = ref<HTMLElement | null>(null)

// Publish the player's height as a CSS variable so page scroll containers can
// reserve space for it (the same mechanism the mobile tab bar uses). This is
// what pushes the rest of the site up instead of letting the fixed bar overlap
// and block the bottom controls — on desktop AND mobile, on every page.
function setPlayerHeightVar() {
  if (typeof document === 'undefined') return
  const h = barEl.value?.offsetHeight || 60
  document.documentElement.style.setProperty('--audio-player-height', `${h}px`)
  document.documentElement.classList.add('audio-active')
}

function clearPlayerHeightVar() {
  if (typeof document === 'undefined') return
  document.documentElement.style.setProperty('--audio-player-height', '0px')
  document.documentElement.classList.remove('audio-active')
}

watch(() => audioPlayer.currentName.value && !audioPlayer.externalVisible.value, (name) => {
  if (name) {
    nextTick(setPlayerHeightVar)
  } else {
    clearPlayerHeightVar()
  }
}, { immediate: true })

onBeforeUnmount(clearPlayerHeightVar)

function togglePlay() {
  if (audioPlayer.playing.value) {
    audioPlayer.pause()
  } else if (audioPlayer.currentSrc.value) {
    audioPlayer.play(audioPlayer.currentSrc.value, audioPlayer.currentName.value)
  }
}

function onProgressClick(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const rect = el.getBoundingClientRect()
  const ratio = (e.clientX - rect.left) / rect.width
  const time = ratio * audioPlayer.duration.value
  audioPlayer.seek(time)
}

function formatTime(seconds: number): string {
  if (!seconds || !isFinite(seconds)) return '0:00'
  const m = Math.floor(seconds / 60)
  const s = Math.floor(seconds % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}
</script>

<style scoped>
.audio-player-bar {
  /* Sit directly above the mobile tab bar (its height is published as
     --mobile-tab-bar-height). On desktop the tab bar is hidden so the variable
     resolves to 0px and the bar docks flush to the bottom of the viewport. */
  bottom: var(--mobile-tab-bar-height, 0px);
  background: rgba(15, 15, 15, 0.55);
  backdrop-filter: blur(24px) saturate(1.4);
  -webkit-backdrop-filter: blur(24px) saturate(1.4);
  border-top: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: 0 -4px 30px rgba(0, 0, 0, 0.3);
}

.slide-up-enter-active,
.slide-up-leave-active {
  transition: transform 0.3s ease, opacity 0.3s ease;
}

.slide-up-enter-from,
.slide-up-leave-to {
  transform: translateY(100%);
  opacity: 0;
}
</style>
