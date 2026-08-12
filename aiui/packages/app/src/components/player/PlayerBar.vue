<template>
  <Transition name="player-slide">
    <div
      v-if="hasTrack"
      :class="[
        props.variant === 'fixed'
          ? 'fixed bottom-0 left-0 right-0 z-[999]'
          : 'w-full shrink-0',
        'path-glass-card !rounded-none'
      ]"
    >
      <!-- Plyr container: YouTube requires min 200x200px. Kept off-screen but sized. -->
      <div
        ref="plyrContainerRef"
        class="absolute left-[-9999px] w-[320px] h-[180px] overflow-hidden"
        aria-hidden="true"
      />

      <!-- Compact layout: mini-player (mobile) -->
      <div v-if="props.compact" class="flex flex-col">
        <!-- Scrubber bar (full width, thin) -->
        <div
          class="w-full h-1 cursor-pointer bg-white/10"
          @click="onScrubberClick"
        >
          <div
            class="h-full bg-accent transition-all duration-150"
            :style="{ width: `${progress}%` }"
          />
        </div>
        <!-- Cover + info + controls -->
        <div class="flex items-center gap-3 px-3 py-2">
          <div class="w-10 h-10 rounded-lg overflow-hidden shrink-0 flex items-center justify-center path-glass-icon">
            <img
              v-if="coverUrl"
              :src="coverUrl"
              :alt="currentSong!.title"
              class="w-full h-full object-cover"
            />
            <span v-else class="text-base">🎵</span>
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-semibold truncate text-white/90">{{ cleanTitle(currentSong!.title) }}</p>
            <p class="text-xs truncate text-white/50">{{ currentSong!.artist }}</p>
          </div>
          <button
            class="w-11 h-11 rounded-full flex items-center justify-center shrink-0 active:scale-95"
            :class="hasPrevious ? 'text-white/70' : 'text-white/20'"
            :disabled="!hasPrevious"
            aria-label="Previous track"
            @click="playPrevious"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h2v12H6V6zm3.5 6l8.5 6V6l-8.5 6z" />
            </svg>
          </button>
          <button
            class="w-12 h-12 rounded-full flex items-center justify-center glass-button shrink-0 active:scale-95"
            aria-label="Play or pause"
            @click="toggle"
          >
            <svg v-if="isLoading" class="w-6 h-6 animate-spin text-white/90" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
            <svg v-else-if="isPlaying" class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
            </svg>
            <svg v-else class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7L8 5z" />
            </svg>
          </button>
          <button
            class="w-11 h-11 rounded-full flex items-center justify-center shrink-0 active:scale-95"
            :class="hasNext ? 'text-white/70' : 'text-white/20'"
            :disabled="!hasNext"
            aria-label="Next track"
            @click="playNext"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 18l8.5-6L6 6v12zm10-12v12h2V6h-2z" />
            </svg>
          </button>
          <button
            class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0 active:scale-95 text-white/40"
            aria-label="Close player"
            @click="clear"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <p v-if="error" class="text-xs text-red-400/60 px-3 pb-1 truncate">{{ error }}</p>
      </div>

      <!-- Desktop layout: full controls with scrubber -->
      <div v-else class="flex items-center gap-4 px-4 py-3">
        <div class="flex items-center gap-3 min-w-0 flex-1 max-w-[280px]">
          <div class="w-12 h-12 rounded-lg overflow-hidden shrink-0 flex items-center justify-center path-glass-icon">
            <img
              v-if="coverUrl"
              :src="coverUrl"
              :alt="currentSong!.title"
              class="w-full h-full object-cover"
            />
            <span v-else class="text-lg">🎵</span>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-semibold truncate text-white/90">{{ cleanTitle(currentSong!.title) }}</p>
            <p class="text-xs truncate text-white/50">{{ currentSong!.artist }}</p>
          </div>
        </div>

        <div class="flex flex-col gap-1 flex-1 max-w-xl mx-4">
          <div class="flex items-center gap-2">
            <button
              class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center shrink-0 transition-all hover:scale-105 active:scale-95"
              :class="hasPrevious ? 'text-white/70 hover:text-white/90' : 'text-white/20'"
              :disabled="!hasPrevious"
              aria-label="Previous track"
              @click="playPrevious"
            >
              <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 6h2v12H6V6zm3.5 6l8.5 6V6l-8.5 6z" />
              </svg>
            </button>
            <button
              class="w-12 h-12 rounded-full flex items-center justify-center glass-button shrink-0 transition-all hover:scale-105 active:scale-95"
              aria-label="Play or pause"
              @click="toggle"
            >
              <svg v-if="isLoading" class="w-6 h-6 animate-spin text-white/90" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              <svg v-else-if="isPlaying" class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
              </svg>
              <svg v-else class="w-6 h-6 text-white/90" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7L8 5z" />
              </svg>
            </button>
            <button
              class="min-w-[44px] min-h-[44px] rounded-full flex items-center justify-center shrink-0 transition-all hover:scale-105 active:scale-95"
              :class="hasNext ? 'text-white/70 hover:text-white/90' : 'text-white/20'"
              :disabled="!hasNext"
              aria-label="Next track"
              @click="playNext"
            >
              <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 18l8.5-6L6 6v12zm10-12v12h2V6h-2z" />
              </svg>
            </button>
            <span class="text-xs font-mono tabular-nums text-white/40">
              {{ formatTime(currentTime) }}
            </span>
            <div
              class="flex-1 h-1.5 rounded-full cursor-pointer group bg-white/15"
              @click="onScrubberClick"
            >
              <div
                class="h-full rounded-full transition-all duration-150 group-hover:h-2 bg-accent"
                :style="{ width: `${progress}%` }"
              />
            </div>
            <span class="text-xs font-mono tabular-nums text-white/40">
              {{ formatTime(duration) }}
            </span>
          </div>
          <p v-if="error" class="text-xs text-red-400/60 truncate">{{ error }}</p>
        </div>

        <span
          v-if="queue.length > 1"
          class="text-xs font-mono tabular-nums shrink-0 text-white/30"
        >{{ queue.length }} songs</span>
        <button
          class="touch-target rounded-xl path-glass-button path-glass-button-sm shrink-0 transition-all hover:scale-105"
          aria-label="Close player"
          @click="clear"
          title="Close player"
        >
          <svg class="w-4 h-4 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from 'vue'
import { usePlayer } from '@/composables/usePlayer'
import { fetchMusicCover } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  variant?: 'fixed' | 'inline'
  compact?: boolean
}>(), { variant: 'fixed', compact: false })

const {
  currentSong,
  hasTrack,
  isPlaying,
  isLoading,
  error,
  currentTime,
  duration,
  progress,
  queue,
  hasNext,
  hasPrevious,
  toggle,
  seek,
  clear,
  playNext,
  playPrevious,
  setContainer,
} = usePlayer()

const plyrContainerRef = ref<HTMLDivElement | null>(null)
const fetchedCover = ref<string | null>(null)

const coverUrl = computed(() => {
  const song = currentSong.value
  if (!song) return null
  return song.coverUrl || fetchedCover.value
})

function cleanTitle(raw: string): string {
  return raw.replace(/^(?:song|film|podcast|book|tv)_ext:/i, '').replace(/\|.*$/, '')
}

function formatTime(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = Math.floor(sec % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function onScrubberClick(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const rect = el.getBoundingClientRect()
  const percent = Math.max(0, Math.min(100, ((e.clientX - rect.left) / rect.width) * 100))
  seek(percent)
}

onMounted(async () => {
  await nextTick()
  setContainer(plyrContainerRef.value)
})

watch(
  () => plyrContainerRef.value,
  (el) => setContainer(el),
  { flush: 'post' },
)

watch(currentSong, (song) => {
  fetchedCover.value = null
  if (song && !song.coverUrl) {
    fetchMusicCover(song.title, song.artist).then((url) => {
      if (url) fetchedCover.value = url
    })
  }
}, { immediate: true })
</script>

<style scoped>
.player-slide-enter-active,
.player-slide-leave-active {
  transition: transform 0.25s ease, opacity 0.2s ease;
}
.player-slide-enter-from,
.player-slide-leave-to {
  transform: translateY(100%);
  opacity: 0;
}
</style>
