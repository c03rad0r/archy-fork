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
            {{ filteredSeries.length }} series
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <input
        v-model="search"
        type="text"
        placeholder="Search TV series..."
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
          : 'bg-black/3 text-gray-800 placeholder:text-gray-400 focus:bg-black/5'"
      />

      <div v-if="topGenres.length > 0" class="flex flex-wrap gap-1.5">
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
          v-for="s in filteredSeries"
          :key="s.id"
          class="group flex flex-col items-stretch text-left w-full path-glass-bubble rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :aria-label="s.title"
          @click="$emit('selectSeries', s)"
        >
          <div class="cover-card flex-1 min-h-0 relative">
            <div class="aspect-[2/3] relative w-full overflow-hidden rounded-[10px]" :class="!coverSrc(s) ? (isDark ? 'bg-white/[0.06]' : 'bg-black/[0.04]') : ''">
              <div v-if="isLoading(s)" class="absolute inset-0 animate-shimmer" />
              <img
                v-if="coverSrc(s)"
                :src="coverSrc(s)!"
                :alt="`${s.title} — TV Series`"
                class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
                loading="lazy"
                @error="onError(s)"
              />
              <img
                v-else-if="!isLoading(s)"
                :src="fallbackSrc(s)"
                :alt="s.title"
                class="w-full h-full object-cover"
              />
              <div v-if="coverSrc(s)" class="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent pointer-events-none" />

              <div class="absolute bottom-0 left-0 right-0 p-2">
                <p class="text-xs font-semibold text-white/90 leading-tight truncate">
                  {{ s.title }}
                </p>
                <p class="text-xs text-white/40 truncate mt-0.5">
                  {{ yearDisplay(s) }}<template v-if="s.seasons"> · {{ s.seasons }}S</template>
                </p>
              </div>

              <div v-if="s.rating" class="absolute top-1.5 left-1.5">
                <span class="text-xs px-1.5 py-0.5 rounded bg-black/60 text-amber-400 backdrop-blur-sm font-medium">
                  ★ {{ s.rating.toFixed(1) }}
                </span>
              </div>

              <div v-if="s.status === 'ongoing'" class="absolute top-1.5 right-1.5">
                <span class="text-xs px-1 py-0.5 rounded bg-emerald-500/80 text-white backdrop-blur-sm">
                  ongoing
                </span>
              </div>

              <div class="absolute top-1.5 right-1.5 flex gap-0.5 flex-wrap justify-end max-w-[60%]">
                <span
                  v-for="src in (s.sources ?? []).slice(0, 2)"
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

      <div v-if="filteredSeries.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No TV series match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef } from 'vue'
import type { TVSeries } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { useContentImages } from '@/composables/useContentImages'
import { generateTVSeriesFallback, fetchTVImage } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  series: TVSeries[]
  title?: string
}>(), {
  title: 'Recommended TV Series',
})

defineEmits<{ selectSeries: [series: TVSeries] }>()

const { isDark } = useTheme()
const search = ref('')
const activeGenre = ref<string | null>(null)

const { coverSrc, fallbackSrc, onError, isLoading } = useContentImages({
  items: toRef(props, 'series'),
  id: (s) => s.id,
  existingUrl: (s) => s.posterUrl || s.backdropUrl,
  fetch: (s) => fetchTVImage(s.title, s.year).then((r) => r.posterUrl),
  fallback: (s) => generateTVSeriesFallback(s.title, s.year),
})

function yearDisplay(s: TVSeries): string {
  if (!s.year) return ''
  if (s.endYear && s.endYear !== s.year) return `${s.year}–${s.endYear}`
  if (s.status === 'ongoing') return `${s.year}–`
  return String(s.year)
}

const topGenres = computed(() => {
  const counts = new Map<string, number>()
  for (const s of props.series) {
    for (const g of s.genres ?? []) {
      counts.set(g, (counts.get(g) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([g]) => g)
})

const filteredSeries = computed(() => {
  let result = props.series
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        (s.creator ?? '').toLowerCase().includes(q) ||
        (s.network ?? '').toLowerCase().includes(q) ||
        (s.genres ?? []).some((g) => g.toLowerCase().includes(q))
    )
  }
  if (activeGenre.value) {
    result = result.filter((s) => (s.genres ?? []).includes(activeGenre.value!))
  }
  return result
})
</script>
