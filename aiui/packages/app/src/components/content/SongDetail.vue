<template>
  <div class="song-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden">
      <div class="w-full aspect-[16/7] flex items-center justify-center overflow-hidden bg-black/20">
        <img
          v-if="coverSrc"
          :src="coverSrc"
          :alt="song.title"
          class="w-full h-full object-cover object-center block"
          @error="coverFailed = true"
        />
        <div
          v-else
          class="w-full h-full bg-cover bg-center"
          :style="{ backgroundImage: `url(${fallbackCover})` }"
        />
      </div>
      <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pointer-events-none" />

      <button
        class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-20 h-20 rounded-full flex items-center justify-center path-glass-icon hover:scale-105 active:scale-95 transition-all duration-200 z-10"
        title="Play"
        @click="onPlay"
      >
        <svg class="w-10 h-10 text-white" fill="currentColor" viewBox="0 0 24 24">
          <path d="M8 5v14l11-7L8 5z" />
        </svg>
      </button>

      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <div class="absolute bottom-0 left-0 right-0 p-4">
        <h2 class="text-lg font-bold text-white">{{ song.title }}</h2>
        <div class="flex items-center gap-2 mt-1 text-xs text-white/60">
          <span>{{ song.artist }}</span>
          <span v-if="song.album">{{ song.album }}</span>
          <span v-if="song.year">{{ song.year }}</span>
          <span v-if="song.duration">{{ formatDuration(song.duration) }}</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <div v-if="song.genres?.length" class="flex flex-wrap gap-1.5">
        <span
          v-for="genre in song.genres"
          :key="genre"
          class="text-xs px-2 py-1 rounded-md font-medium"
          :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'"
        >
          {{ genre }}
        </span>
      </div>

      <div v-if="wavlakeChartUrl" class="rounded-xl overflow-hidden border"
           :class="isDark ? 'border-white/10' : 'border-black/8'">
        <h4 class="text-xs font-semibold mb-2 px-3 pt-3"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Discover on Wavlake</h4>
        <iframe
          :src="wavlakeChartUrl"
          class="w-full h-[380px] border-0"
          loading="lazy"
          title="Wavlake chart"
        />
      </div>

      <div>
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Listen on</h4>
        <div class="space-y-2">
          <a
            v-for="link in listenLinks"
            :key="link.url"
            :href="link.url"
            target="_blank"
            rel="noopener"
            class="flex items-center justify-between p-3 rounded-xl transition-colors"
            :class="isDark
              ? 'bg-white/5 hover:bg-white/10'
              : 'bg-black/3 hover:bg-black/5'"
          >
            <div class="flex items-center gap-2.5">
              <span class="text-sm">{{ link.icon }}</span>
              <div>
                <p class="text-xs font-medium"
                   :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ link.name }}</p>
                <p v-if="link.desc" class="text-xs"
                   :class="isDark ? 'text-white/30' : 'text-gray-400'">{{ link.desc }}</p>
              </div>
            </div>
            <svg class="w-4 h-4" :class="isDark ? 'text-white/30' : 'text-gray-400'"
                 fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </a>
          <a
            v-for="src in song.sources ?? []"
            :key="src.url"
            :href="src.url"
            target="_blank"
            rel="noopener"
            class="flex items-center justify-between p-3 rounded-xl transition-colors"
            :class="isDark
              ? 'bg-white/5 hover:bg-white/10'
              : 'bg-black/3 hover:bg-black/5'"
          >
            <div class="flex items-center gap-2.5">
              <span class="text-sm">{{ sourceIcon(src.type) }}</span>
              <p class="text-xs font-medium"
                 :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ src.name }}</p>
            </div>
            <svg class="w-4 h-4" :class="isDark ? 'text-white/30' : 'text-gray-400'"
                 fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Song } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generateSongCoverFallback, fetchMusicCover } from '@/composables/useImageFallback'
import { usePlayer } from '@/composables/usePlayer'
import { mapToWavlakeGenre } from '@/utils/wavlakeGenres'

const props = defineProps<{ song: Song }>()
const { play } = usePlayer()

function onPlay() {
  play(props.song)
}
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const coverFailed = ref(false)
const fetchedCover = ref<string | null>(null)

const q = computed(() =>
  `${props.song.title} ${props.song.artist}`.trim().replace(/\s+/g, '+'),
)

const listenLinks = computed(() => {
  const links: { name: string; url: string; icon: string; desc?: string }[] = [
    { name: 'Wavlake', url: `https://wavlake.com/search?q=${q.value}`, icon: '⚡', desc: 'Lightning-powered music' },
  ]
  return links
})

const wavlakeGenre = computed(() =>
  mapToWavlakeGenre(props.song.genres ?? []),
)

const wavlakeChartUrl = computed(() => {
  const genre = wavlakeGenre.value
  if (!genre) return null
  return `https://embed.wavlake.com/chart?days=21&limit=10&genre=${encodeURIComponent(genre)}`
})

const coverSrc = computed(() => {
  if (coverFailed.value) return null
  return props.song.coverUrl || fetchedCover.value
})

const fallbackCover = computed(() =>
  generateSongCoverFallback(props.song.title, props.song.artist)
)

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

function sourceIcon(type: string): string {
  const icons: Record<string, string> = {
    spotify: '🟢',
    youtube: '▶️',
    'apple-music': '🍎',
    bandcamp: '📦',
    soundcloud: '☁️',
    wavlake: '⚡',
    internet_archive: '🏛️',
    jamendo: '🎵',
    odysee: '🔗',
    funkwhale: '🐋',
  }
  return icons[type] ?? '🎵'
}

onMounted(() => {
  if (props.song.coverUrl) return
  fetchMusicCover(props.song.title, props.song.artist, props.song.album).then((url) => {
    if (url) fetchedCover.value = url
  })
})
</script>
