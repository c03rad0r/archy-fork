<template>
  <div class="tv-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden">
      <div class="w-full aspect-[16/7] flex items-center justify-center overflow-hidden bg-black/20">
        <img
          v-if="bannerSrc"
          :src="bannerSrc"
          :alt="series.title"
          class="w-full h-full object-cover object-center block"
          @error="onBannerError"
        />
        <div
          v-else
          class="w-full h-full"
          :style="{ background: fallbackGradient }"
        />
      </div>
      <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pointer-events-none" />

      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <div class="absolute bottom-0 left-0 right-0 p-4">
        <h2 class="text-lg font-bold text-white">{{ series.title }}</h2>
        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 mt-1 text-xs text-white/60">
          <span v-if="yearDisplay">{{ yearDisplay }}</span>
          <span v-if="series.seasons">{{ series.seasons }} seasons</span>
          <span v-if="series.episodes">{{ series.episodes }} episodes</span>
          <span v-if="series.network">{{ series.network }}</span>
          <span v-if="series.rating" class="text-amber-400">★ {{ series.rating.toFixed(1) }}</span>
          <span v-if="series.status === 'ongoing'" class="text-emerald-400">ongoing</span>
          <span v-else-if="series.status === 'ended'" class="text-white/40">ended</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <p v-if="series.synopsis" class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/70' : 'text-gray-600'">
        {{ series.synopsis }}
      </p>

      <div v-if="series.creator" class="text-xs"
           :class="isDark ? 'text-white/50' : 'text-gray-500'">
        Created by <span class="font-medium" :class="isDark ? 'text-white/70' : 'text-gray-700'">{{ series.creator }}</span>
      </div>

      <div v-if="series.cast?.length" class="text-xs"
           :class="isDark ? 'text-white/50' : 'text-gray-500'">
        Starring: {{ series.cast.slice(0, 5).join(', ') }}
      </div>

      <div v-if="series.genres?.length" class="flex flex-wrap gap-1.5">
        <span
          v-for="genre in series.genres"
          :key="genre"
          class="text-xs px-2 py-1 rounded-md font-medium"
          :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'"
        >
          {{ genre }}
        </span>
      </div>

      <div>
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Watch on</h4>
        <div class="space-y-2">
          <a
            v-for="src in (series.sources ?? [])"
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
          <a
            v-for="link in watchLinks"
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
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TVSeries } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { useBannerFallback } from '@/composables/useBannerFallback'
import { fetchTVImage } from '@/composables/useImageFallback'

const props = defineProps<{ series: TVSeries }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()

const { bannerSrc, fallbackGradient, onBannerError } = useBannerFallback({
  primaryUrls: () => [props.series.posterUrl, props.series.backdropUrl],
  apiFetch: () => fetchTVImage(props.series.title, props.series.year),
  title: () => props.series.title,
})

const yearDisplay = computed(() => {
  if (!props.series.year) return ''
  if (props.series.endYear && props.series.endYear !== props.series.year) {
    return `${props.series.year}–${props.series.endYear}`
  }
  if (props.series.status === 'ongoing') return `${props.series.year}–`
  return String(props.series.year)
})

const q = computed(() =>
  props.series.title.trim().replace(/\s+/g, '+'),
)

const watchLinks = computed(() => {
  if ((props.series.sources ?? []).length > 0) return []
  return [
    { name: 'Internet Archive', url: `https://archive.org/search?query=${q.value}`, icon: '🏛️', desc: 'Free, open archive' },
    { name: 'YouTube', url: `https://youtube.com/results?search_query=${q.value}+full+series`, icon: '▶️', desc: 'Free episodes' },
    { name: 'Odysee', url: `https://odysee.com/$/search?q=${q.value}`, icon: '🔗', desc: 'Decentralized' },
    { name: 'Tubi', url: `https://tubitv.com/search/${q.value}`, icon: '📺', desc: 'Free streaming' },
  ]
})

function sourceIcon(type: string): string {
  const icons: Record<string, string> = {
    plex: '🟠',
    nextcloud: '☁️',
    youtube: '▶️',
    netflix: '🔴',
    'free-web': '🌐',
    local: '💾',
  }
  return icons[type] ?? '📺'
}

</script>
