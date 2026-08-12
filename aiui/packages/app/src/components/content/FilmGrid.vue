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
            {{ filteredFilms.length }} films
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <input
        v-model="search"
        type="text"
        placeholder="Search films..."
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
          v-for="film in filteredFilms"
          :key="film.id"
          class="group flex flex-col items-stretch text-left w-full path-glass-bubble rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :aria-label="`${film.title} (${film.year})`"
          @click="$emit('selectFilm', film)"
        >
          <div class="poster-card flex-1 min-h-0">
            <div class="aspect-[2/3] relative w-full overflow-hidden rounded-[10px]" :class="!coverSrc(film) ? (isDark ? 'bg-white/[0.06]' : 'bg-black/[0.04]') : ''">
              <div v-if="isLoading(film)" class="absolute inset-0 animate-shimmer" />
              <img
                v-if="coverSrc(film)"
                :src="coverSrc(film)!"
                :alt="`${film.title} (${film.year}) directed by ${film.director}`"
                class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
                loading="lazy"
                @error="onError(film)"
              />
              <img
                v-else-if="!isLoading(film)"
                :src="fallbackSrc(film)"
                :alt="film.title"
                class="w-full h-full object-cover"
              />
              <div v-if="coverSrc(film)" class="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent pointer-events-none" />

              <div class="absolute bottom-0 left-0 right-0 p-2">
                <p class="text-xs font-semibold text-white/90 leading-tight truncate">
                  {{ film.title }}
                </p>
                <div class="flex items-center gap-1 mt-0.5">
                  <!-- hide zero-value metadata: "★ 0" tells the operator nothing -->
                  <span v-if="film.rating > 0" class="text-xs text-accent font-bold">★ {{ film.rating }}</span>
                  <span v-if="film.year > 0" class="text-xs text-white/40">{{ film.year }}</span>
                </div>
              </div>

              <div class="absolute top-1.5 right-1.5 flex gap-0.5">
                <span
                  v-for="src in film.sources.slice(0, 2)"
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

      <div v-if="filteredFilms.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No films match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef } from 'vue'
import type { Film } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { useContentImages } from '@/composables/useContentImages'
import { fetchFilmImage, generatePosterFallback } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  films: Film[]
  title?: string
}>(), {
  title: 'Recommended Films',
})

defineEmits<{ selectFilm: [film: Film] }>()

const { isDark } = useTheme()
const search = ref('')
const activeGenre = ref<string | null>(null)

const { coverSrc, fallbackSrc, onError, isLoading } = useContentImages({
  items: toRef(props, 'films'),
  id: (f) => f.id,
  existingUrl: (f) => f.posterUrl || f.backdropUrl,
  fetch: (f) => fetchFilmImage(f.title, f.year).then((r) => r.posterUrl),
  fallback: (f) => generatePosterFallback(f.title, f.year),
})

const topGenres = computed(() => {
  const counts = new Map<string, number>()
  for (const f of props.films) {
    for (const g of f.genres) {
      counts.set(g, (counts.get(g) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([g]) => g)
})

const filteredFilms = computed(() => {
  let result = props.films
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(
      (f) =>
        f.title.toLowerCase().includes(q) ||
        f.director.toLowerCase().includes(q) ||
        f.cast.some((c) => c.toLowerCase().includes(q))
    )
  }
  if (activeGenre.value) {
    result = result.filter((f) => f.genres.includes(activeGenre.value!))
  }
  return result
})
</script>
