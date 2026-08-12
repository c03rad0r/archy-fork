<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-base font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ title || 'Places' }}
        </h3>
        <div class="shrink-0 flex items-center gap-2">
          <span class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            {{ filteredPlaces.length }} places
          </span>
          <slot name="header-actions" />
        </div>
      </div>
      <input
        v-model="search"
        type="text"
        :placeholder="`Search places...`"
        class="w-full text-base px-3 py-2 rounded-lg outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder-white/25 focus:bg-white/8'
          : 'bg-black/5 text-gray-800 placeholder-gray-400 focus:bg-black/8'"
      />
      <div v-if="topCategories.length > 1" class="flex flex-wrap gap-1.5">
        <button
          v-for="cat in topCategories"
          :key="cat"
          class="text-xs px-2 py-1 rounded-md font-medium transition-all duration-150"
          :class="activeCategory === cat
            ? 'nav-tab-active'
            : isDark
              ? 'bg-white/5 text-white/40 hover:text-white/70'
              : 'bg-black/5 text-gray-500 hover:text-gray-800'"
          @click="activeCategory = activeCategory === cat ? null : cat"
        >
          {{ cat }}
        </button>
      </div>
    </div>
    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pb-16">
      <div class="grid grid-cols-2 sm:grid-cols-3 gap-3">
        <button
          v-for="place in filteredPlaces"
          :key="place.id"
          class="group flex flex-col items-stretch text-left w-full path-glass-bubble rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :aria-label="place.name"
          @click="$emit('selectPlace', place)"
        >
          <div class="aspect-[4/3] relative w-full overflow-hidden rounded-t-[10px]">
            <div v-if="isLoading(place)" class="absolute inset-0 animate-shimmer" />
            <img
              v-if="coverSrc(place)"
              :src="coverSrc(place)!"
              :alt="place.name"
              class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
              loading="lazy"
              @error="onError(place)"
            />
            <img
              v-else-if="!isLoading(place)"
              :src="fallbackSrc(place)"
              :alt="place.name"
              class="w-full h-full object-cover"
            />
            <div v-if="coverSrc(place)" class="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent pointer-events-none" />
            <div class="absolute bottom-0 left-0 right-0 p-2">
              <p class="text-xs font-semibold text-white/90 truncate">{{ place.name }}</p>
              <p class="text-xs text-white/40 truncate mt-0.5">{{ place.cuisine || place.category }}</p>
            </div>
            <div v-if="place.rating" class="absolute top-1.5 right-1.5">
              <span class="text-xs px-1.5 py-0.5 rounded bg-black/60 text-amber-400 backdrop-blur-sm font-semibold">
                ★ {{ place.rating.toFixed(1) }}
              </span>
            </div>
            <div v-if="place.priceLevel" class="absolute top-1.5 left-1.5">
              <span class="text-xs px-1 py-0.5 rounded bg-black/60 text-white/70 backdrop-blur-sm">
                {{ '$'.repeat(place.priceLevel) }}
              </span>
            </div>
          </div>
        </button>
      </div>
      <div v-if="filteredPlaces.length === 0" class="flex items-center justify-center py-12">
        <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No places match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, toRef } from 'vue'
import type { Place } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { useContentImages } from '@/composables/useContentImages'
import { generatePlaceFallback, fetchPlaceImage } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  places: Place[]
  title?: string
}>(), {
  title: 'Places',
})

defineEmits<{ selectPlace: [place: Place] }>()

const { isDark } = useTheme()
const search = ref('')
const activeCategory = ref<string | null>(null)

const { coverSrc, fallbackSrc, onError, isLoading } = useContentImages({
  items: toRef(props, 'places'),
  id: (p) => p.id,
  existingUrl: (p) => p.photoUrl,
  fetch: (p) => fetchPlaceImage(p.name, p.city),
  fallback: (p) => generatePlaceFallback(p.name, p.cuisine || p.category),
})

const topCategories = computed(() => {
  const counts = new Map<string, number>()
  for (const p of props.places) {
    const cat = p.cuisine || p.category
    if (cat) counts.set(cat, (counts.get(cat) ?? 0) + 1)
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([g]) => g)
})

const filteredPlaces = computed(() => {
  let result = props.places
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(p =>
      p.name.toLowerCase().includes(q) ||
      (p.cuisine?.toLowerCase().includes(q)) ||
      (p.category?.toLowerCase().includes(q)) ||
      (p.city?.toLowerCase().includes(q)) ||
      (p.address?.toLowerCase().includes(q))
    )
  }
  if (activeCategory.value) {
    result = result.filter(p =>
      p.cuisine === activeCategory.value || p.category === activeCategory.value
    )
  }
  return result
})
</script>
