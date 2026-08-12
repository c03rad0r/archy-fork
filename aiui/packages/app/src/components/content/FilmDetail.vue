<template>
  <div class="film-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden aspect-[16/7] shrink-0">
      <img
        v-if="bannerSrc"
        :src="bannerSrc"
        :alt="film.title"
        class="absolute inset-0 w-full h-full object-cover object-center block"
        @error="onBannerError"
      />
      <div
        v-else
        class="absolute inset-0"
        :style="{ background: fallbackGradient }"
      />
      <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pointer-events-none" />

      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10 text-white/80"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <button
        v-if="playableSource"
        class="absolute inset-0 flex items-center justify-center z-[5] group/play"
        aria-label="Watch film"
        @click="openVideo"
      >
        <span class="w-20 h-20 rounded-full flex items-center justify-center path-glass-icon group-hover/play:scale-110 transition-transform">
          <svg class="w-10 h-10 text-white" fill="currentColor" viewBox="0 0 24 24">
            <path d="M8 5v14l11-7L8 5z" />
          </svg>
        </span>
      </button>

      <div class="absolute bottom-0 left-0 right-0 p-4">
        <h2 class="text-lg font-bold text-white">{{ film.title }}</h2>
        <div class="flex items-center gap-2 mt-1 text-xs text-white/60">
          <!-- honest rendering: zero/empty metadata is hidden, not "★ 0 · 0m" -->
          <span v-if="film.rating > 0" class="text-accent font-bold">★ {{ film.rating }}</span>
          <span v-if="film.year > 0">{{ film.year }}</span>
          <span v-if="film.runtime > 0">{{ film.runtime }}m</span>
          <span v-if="film.director">{{ film.director }}</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <div v-if="film.genres.length" class="flex flex-wrap gap-1.5">
        <span
          v-for="genre in film.genres"
          :key="genre"
          class="text-xs px-2 py-1 rounded-md font-medium"
          :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'"
        >
          {{ genre }}
        </span>
      </div>

      <div v-if="film.synopsis">
        <h4 v-if="isExternal"
            class="text-xs uppercase tracking-[0.2em] font-semibold mb-1.5"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">
          Why watch
        </h4>
        <p class="text-sm leading-relaxed"
           :class="isDark ? 'text-white/70' : 'text-gray-600'">
          {{ film.synopsis }}
        </p>
      </div>

      <div v-if="film.cast.length">
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Cast</h4>
        <p class="text-sm" :class="isDark ? 'text-white/70' : 'text-gray-700'">
          {{ film.cast.join(', ') }}
        </p>
      </div>

      <div v-if="film.sources.length">
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Watch on</h4>
        <div class="space-y-2">
          <a
            v-for="src in film.sources"
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
              <div>
                <p class="text-xs font-medium"
                   :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ src.name }}</p>
                <p class="text-xs"
                   :class="isDark ? 'text-white/30' : 'text-gray-400'">{{ src.quality }}</p>
              </div>
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
import { computed } from 'vue'
import type { Film } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { useBannerFallback } from '@/composables/useBannerFallback'
import { fetchFilmImage } from '@/composables/useImageFallback'
import { useVideoPlayerStore } from '@/stores/videoPlayer'

const props = defineProps<{ film: Film }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const videoStore = useVideoPlayerStore()

const isExternal = computed(() => props.film.id.startsWith('ext-'))

// Node files first: own/peer/IndeeHub sources are same-origin `/content/<id>`
// streams the node serves with Range support (media-src 'self' allows them).
// YouTube is the free-films fallback, not the default — a real library file
// must never route out to YouTube when its bytes are on the operator's node.
const NODE_SOURCE_TYPES = new Set(['nextcloud', 'plex', 'indeehub'])
const playableSource = computed(() =>
  props.film.sources.find(
    s => NODE_SOURCE_TYPES.has(s.type) && (s.url.startsWith('/') || s.url.startsWith(window.location.origin)),
  ) ?? props.film.sources.find(s => s.type === 'youtube' || s.url.includes('youtube.com'))
)

function openVideo() {
  if (!playableSource.value) return
  videoStore.open(playableSource.value.url, props.film.title, props.film.posterUrl || props.film.backdropUrl)
}

const { bannerSrc, fallbackGradient, onBannerError } = useBannerFallback({
  primaryUrls: () => [props.film.backdropUrl, props.film.posterUrl],
  apiFetch: () => fetchFilmImage(props.film.title, props.film.year),
  title: () => props.film.title,
})

function sourceIcon(type: string): string {
  const icons: Record<string, string> = {
    plex: '🟧',
    nextcloud: '☁️',
    youtube: '▶️',
    'free-web': '🌐',
  }
  return icons[type] ?? '📺'
}
</script>
