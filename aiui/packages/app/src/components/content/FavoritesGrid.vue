<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" :style="isDark
      ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
      : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
    >
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          Favorites
        </h3>
        <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ filteredItems.length }} saved
        </span>
      </div>

      <div class="flex gap-1.5 flex-wrap">
        <button
          v-for="filter in typeFilters"
          :key="filter.id"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeType === filter.id
            ? 'nav-tab-active'
            : isDark
              ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
              : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
          @click="activeType = activeType === filter.id ? null : filter.id"
        >
          {{ filter.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <div
        v-if="filteredItems.length === 0"
        class="flex flex-col items-center justify-center py-12 gap-2"
      >
        <svg
          class="w-8 h-8"
          :class="isDark ? 'text-white/10' : 'text-gray-200'"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
          />
        </svg>
        <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No favorites yet
        </p>
      </div>

      <div
        v-for="item in filteredItems"
        :key="item.id"
        class="flex items-center gap-3 p-3 rounded-xl transition-all duration-150"
        :class="isDark
          ? 'bg-white/[0.03] hover:bg-white/[0.07] border border-white/5'
          : 'bg-black/[0.02] hover:bg-black/[0.05] border border-black/5'"
      >
        <span
          class="text-xs w-6 h-6 rounded flex items-center justify-center shrink-0"
          :class="typeStyle(item.type)"
        >
          {{ typeIcon(item.type) }}
        </span>
        <div class="flex-1 min-w-0">
          <div
            class="text-xs font-semibold truncate"
            :class="isDark ? 'text-white/80' : 'text-gray-800'"
          >
            {{ item.title }}
          </div>
          <div
            v-if="item.subtitle"
            class="text-xs truncate"
            :class="isDark ? 'text-white/40' : 'text-gray-500'"
          >
            {{ item.subtitle }}
          </div>
        </div>
        <button
          class="shrink-0 text-accent/60 hover:text-accent transition-colors p-1"
          aria-label="Remove from favorites"
          @click="store.removeFavorite(item.id)"
        >
          <svg class="w-3.5 h-3.5" fill="currentColor" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
            />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useFavoritesStore, type FavoriteType } from '@/stores/favorites'

const { isDark } = useTheme()
const store = useFavoritesStore()

const activeType = ref<FavoriteType | null>(null)

const typeFilters: { id: FavoriteType; label: string }[] = [
  { id: 'film', label: 'Films' },
  { id: 'song', label: 'Songs' },
  { id: 'podcast', label: 'Podcasts' },
  { id: 'book', label: 'Books' },
  { id: 'tv', label: 'TV' },
  { id: 'place', label: 'Places' },
]

const filteredItems = computed(() => {
  if (activeType.value) {
    return store.getFavoritesByType(activeType.value)
  }
  return store.sortedItems
})

function typeIcon(type: string): string {
  const icons: Record<string, string> = {
    film: 'F', song: 'S', podcast: 'P', book: 'B', tv: 'T', place: 'L', article: 'A',
  }
  return icons[type] ?? '?'
}

function typeStyle(type: string): string {
  if (isDark.value) {
    const colors: Record<string, string> = {
      film: 'bg-blue-500/20 text-blue-400',
      song: 'bg-green-500/20 text-green-400',
      podcast: 'bg-orange-500/20 text-orange-400',
      book: 'bg-yellow-500/20 text-yellow-400',
      tv: 'bg-indigo-500/20 text-indigo-400',
      place: 'bg-red-500/20 text-red-400',
      article: 'bg-cyan-500/20 text-cyan-400',
    }
    return colors[type] ?? 'bg-white/10 text-white/40'
  }
  const colors: Record<string, string> = {
    film: 'bg-blue-50 text-blue-600',
    song: 'bg-green-50 text-green-600',
    podcast: 'bg-orange-50 text-orange-600',
    book: 'bg-yellow-50 text-yellow-600',
    tv: 'bg-indigo-50 text-indigo-600',
    place: 'bg-red-50 text-red-600',
    article: 'bg-cyan-50 text-cyan-600',
  }
  return colors[type] ?? 'bg-gray-50 text-gray-600'
}
</script>
