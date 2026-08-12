<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select', film)"
  >
    <div class="poster-card-sm shrink-0 w-12 aspect-[2/3] rounded-lg overflow-hidden">
      <img
        v-if="film.posterUrl"
        :src="film.posterUrl"
        :alt="film.title"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="(e) => handleImgError(e, film.title, film.year)"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px]"
        :class="isDark ? 'bg-white/10' : 'bg-black/5'"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ film.title }}</p>
      <p class="text-xs mt-0.5"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ film.year }}<template v-if="film.director"> · {{ film.director }}</template>
      </p>
      <p v-if="isExternal && film.synopsis"
         class="text-xs mt-0.5 line-clamp-2"
         :class="isDark ? 'text-white/35' : 'text-gray-400'">
        {{ film.synopsis }}
      </p>
      <div class="flex items-center gap-1.5 mt-1.5">
        <span v-if="film.rating > 0"
              class="text-xs font-semibold px-1.5 py-0.5 rounded"
              :class="ratingClass">
          ★ {{ film.rating }}
        </span>
        <span v-if="isExternal"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-info/15 text-info/70' : 'bg-info/10 text-blue-600'">
          not in library
        </span>
        <span
          v-for="src in film.sources.slice(0, 3)"
          :key="src.type"
          class="text-xs px-1.5 py-0.5 rounded font-medium"
          :class="isDark ? 'bg-white/8 text-white/50' : 'bg-black/5 text-gray-500'"
        >
          {{ src.type }}
        </span>
        <FavoriteButton
          class="ml-auto"
          :favorited="favoritesStore.isFavorited(film.id)"
          @toggle="favoritesStore.toggleFavorite({ id: film.id, type: 'film', title: film.title, subtitle: `${film.year} · ${film.director}`, data: film })"
        />
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Film } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { handleImgError } from '@/composables/useImageFallback'
import FavoriteButton from '@/components/ui/FavoriteButton.vue'
import { useFavoritesStore } from '@/stores/favorites'

const props = defineProps<{ film: Film }>()
defineEmits<{ select: [film: Film] }>()

const { isDark } = useTheme()
const favoritesStore = useFavoritesStore()

const isExternal = computed(() => props.film.id.startsWith('ext-'))

const ratingClass = computed(() => {
  const r = props.film.rating
  if (r >= 8.5) return isDark.value ? 'bg-success/20 text-success' : 'bg-success/10 text-green-700'
  if (r >= 7.5) return isDark.value ? 'bg-accent/20 text-accent' : 'bg-accent/10 text-amber-700'
  return isDark.value ? 'bg-white/10 text-white/50' : 'bg-black/5 text-gray-500'
})
</script>
