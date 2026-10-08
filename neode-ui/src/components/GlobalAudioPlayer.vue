<template>
  <Teleport to="body">
    <Transition name="slide-up">
      <div
        v-if="audioPlayer.currentName.value && !audioPlayer.externalVisible.value"
        ref="barEl"
        class="fixed left-0 right-0 z-40 audio-player-bar"
      >
        <img v-if="audioPlayer.externalArtwork.value" :src="audioPlayer.externalArtwork.value" alt="" class="player-artwork" referrerpolicy="no-referrer" />
        <div class="player-artwork-shade" aria-hidden="true" />
        <!-- Progress bar (clickable) -->
        <div
          class="relative h-1 bg-white/10 cursor-pointer"
          @click="onProgressClick"
        >
          <div
            class="h-full bg-orange-500 transition-all duration-200"
            :style="{ width: audioPlayer.progress.value + '%' }"
          />
        </div>

        <div class="player-controls relative flex items-center gap-2 px-3 py-2.5">
          <button v-if="audioPlayer.externalApp.value" class="player-control" aria-label="Previous track" @click="audioPlayer.previousExternal()"><svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 5h2v14H6zm13 0v14L9 12z" /></svg></button>
          <!-- Play/Pause -->
          <button
            class="flex-shrink-0 w-9 h-9 rounded-full bg-white/10 hover:bg-white/20 flex items-center justify-center transition-colors"
            :aria-label="audioPlayer.playing.value ? 'Pause audio' : 'Play audio'"
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

          <button v-if="audioPlayer.externalApp.value" class="player-control" aria-label="Next track" @click="audioPlayer.nextExternal()"><svg viewBox="0 0 24 24" fill="currentColor"><path d="M16 5h2v14h-2zM5 5l10 7-10 7z" /></svg></button>
          <!-- Track info -->
          <div class="flex-1 min-w-0">
            <p v-if="audioPlayer.error.value" class="text-sm text-red-400 truncate">{{ audioPlayer.error.value }}</p>
            <p v-else class="text-sm font-medium text-white/90 truncate">{{ audioPlayer.currentName.value }}</p>
            <p class="text-xs text-white/40">{{ formatTime(audioPlayer.currentTime.value) }} / {{ formatTime(audioPlayer.duration.value) }}</p>
          </div>

          <button v-if="audioPlayer.externalApp.value" class="player-control" aria-label="Shuffle" :aria-pressed="audioPlayer.externalShuffle.value" @click="audioPlayer.shuffleExternal()"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m17 3 4 4-4 4M3 17h3c4 0 8-10 12-10h3M17 13l4 4-4 4M3 7h3c1.5 0 3 1.4 4.5 3.3M14 14c1.4 1.7 2.7 3 4 3h3" /></svg></button>
          <button v-if="audioPlayer.externalApp.value" class="glass-button px-3 py-2 rounded-lg text-sm text-white shrink-0" @click="audioPlayer.openExternal()" aria-label="Open playing app"><span class="hidden sm:inline">Open app</span><svg class="w-5 h-5 sm:hidden" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M14 3h7v7m0-7L10 14M9 3H3v18h18v-6" /></svg></button>

          <!-- Close -->
          <button
            class="flex-shrink-0 w-8 h-8 rounded-full hover:bg-white/10 flex items-center justify-center transition-colors"
            aria-label="Stop audio"
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
import { useCompanionAudio } from '@/composables/useCompanionAudio'

const audioPlayer = useAudioPlayer()
useCompanionAudio(audioPlayer)
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
.player-artwork { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: .45; pointer-events: none; }
.player-artwork-shade { position: absolute; inset: 0; background: rgba(8,10,14,.65); pointer-events: none; }
.player-control { width: 36px; height: 44px; display: grid; place-items: center; flex-shrink: 0; color: white; border-radius: 8px; }
.player-control svg { width: 20px; height: 20px; }
.player-control[aria-pressed="true"] { color: #fb923c; background: rgba(251,146,60,.15); }
@media (max-width: 400px) { .player-controls { gap: 2px; padding-inline: 6px; } .player-control { width: 32px; } }
@media (prefers-reduced-motion: reduce) { .slide-up-enter-active, .slide-up-leave-active { transition: none !important; } }
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
