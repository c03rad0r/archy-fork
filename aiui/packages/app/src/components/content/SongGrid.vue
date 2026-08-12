<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" :style="isDark
      ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
      : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
    >
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ title }}
        </h3>
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            {{ filteredSongs.length }} songs
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <input
        v-model="search"
        type="text"
        placeholder="Search songs..."
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
          : 'bg-black/3 text-gray-800 placeholder:text-gray-400 focus:bg-black/5'"
      />

      <div class="flex flex-wrap gap-1.5">
        <button
          v-for="genre in topGenres"
          :key="genre"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeGenre === genre
            ? 'nav-tab-active'
            : isDark
              ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
              : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
          @click="activeGenre = activeGenre === genre ? null : genre"
        >
          {{ genre }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="grid grid-cols-2 sm:grid-cols-3 gap-4">
        <button
          v-for="song in filteredSongs"
          :key="song.id"
          class="group flex flex-col items-stretch text-left w-full path-glass-bubble rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :aria-label="`${song.title} by ${song.artist}`"
          @click="emit('selectSong', song)"
        >
          <div class="cover-card flex-1 min-h-0 relative flex items-center justify-center">
            <div class="aspect-square relative w-full overflow-hidden rounded-[10px]" :class="!coverSrc(song) ? (isDark ? 'bg-white/[0.06]' : 'bg-black/[0.04]') : ''">
              <div v-if="isLoading(song)" class="absolute inset-0 animate-shimmer" />
              <button
                class="absolute inset-0 flex items-center justify-center z-10 backdrop-blur-sm bg-black/30 opacity-0 group-hover:opacity-100 transition-all duration-200"
                aria-label="Play"
                @click.stop="play(song); emit('selectSong', song)"
              >
                <span class="w-16 h-16 rounded-full flex items-center justify-center path-glass-icon">
                  <svg class="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M8 5v14l11-7L8 5z" />
                  </svg>
                </span>
              </button>
              <img
                v-if="coverSrc(song)"
                :src="coverSrc(song)!"
                :alt="`${song.title} by ${song.artist}`"
                class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
                loading="lazy"
                @error="onError(song)"
              />
              <img
                v-else-if="!isLoading(song)"
                :src="fallbackSrc(song)"
                :alt="song.title"
                class="w-full h-full object-cover"
              />
              <div v-if="coverSrc(song)" class="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent pointer-events-none" />

              <div class="absolute bottom-0 left-0 right-0 p-2">
                <p class="text-xs font-semibold text-white/90 leading-tight truncate">
                  {{ song.title }}
                </p>
                <!-- filename-derived songs have no artist — don't spend a blank line on it -->
                <p v-if="song.artist" class="text-xs text-white/40 truncate mt-0.5">{{ song.artist }}</p>
              </div>

              <div class="absolute top-1.5 right-1.5 flex gap-0.5 flex-wrap justify-end max-w-[60%]">
                <span
                  v-for="src in (song.sources ?? []).slice(0, 2)"
                  :key="src.type"
                  class="text-xs px-1 py-0.5 rounded bg-black/60 text-white/70 backdrop-blur-sm"
                >
                  {{ src.type }}
                </span>
              </div>
            </div>
          </div>
        </button>
      </div>

      <div v-if="filteredSongs.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No songs match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef } from 'vue'
import type { Song } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { usePlayer } from '@/composables/usePlayer'
import { useContentImages } from '@/composables/useContentImages'
import { generateSongCoverFallback, fetchMusicCover } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  songs: Song[]
  title?: string
}>(), {
  title: 'Recommended Songs',
})

const emit = defineEmits<{ selectSong: [song: Song] }>()

const { isDark } = useTheme()
const { play } = usePlayer()
const search = ref('')
const activeGenre = ref<string | null>(null)

const { coverSrc, fallbackSrc, onError, isLoading } = useContentImages({
  items: toRef(props, 'songs'),
  id: (s) => s.id,
  existingUrl: (s) => s.coverUrl,
  fetch: (s) => fetchMusicCover(s.title, s.artist, s.album),
  fallback: (s) => generateSongCoverFallback(s.title, s.artist),
})

const topGenres = computed(() => {
  const counts = new Map<string, number>()
  for (const s of props.songs) {
    for (const g of s.genres ?? []) {
      counts.set(g, (counts.get(g) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([g]) => g)
})

const filteredSongs = computed(() => {
  let result = props.songs
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.artist.toLowerCase().includes(q) ||
        (s.album ?? '').toLowerCase().includes(q)
    )
  }
  if (activeGenre.value) {
    result = result.filter((s) => (s.genres ?? []).includes(activeGenre.value!))
  }
  return result
})
</script>
