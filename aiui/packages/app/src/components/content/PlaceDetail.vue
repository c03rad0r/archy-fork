<template>
  <div class="place-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden">
      <div class="w-full aspect-[16/9] flex items-center justify-center overflow-hidden bg-black/20">
        <img
          v-if="place.photoUrl || fetchedPhoto"
          :src="(place.photoUrl || fetchedPhoto)!"
          :alt="place.name"
          class="w-full h-full object-cover object-center block"
        />
        <img
          v-else
          :src="fallbackCover"
          :alt="place.name"
          class="w-full h-full object-cover"
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
        <h2 class="text-lg font-bold text-white">{{ place.name }}</h2>
        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 mt-1 text-xs text-white/60">
          <span v-if="place.cuisine || place.category">{{ place.cuisine || place.category }}</span>
          <span v-if="place.city">{{ place.city }}</span>
          <span v-if="place.rating" class="text-amber-400">★ {{ place.rating.toFixed(1) }}</span>
          <span v-if="place.priceLevel" class="text-white/50">{{ '$'.repeat(place.priceLevel) }}</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <p v-if="place.description" class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/70' : 'text-gray-600'">
        {{ place.description }}
      </p>

      <div v-if="place.address" class="flex items-start gap-2.5">
        <svg class="w-4 h-4 shrink-0 mt-0.5" :class="isDark ? 'text-white/30' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 11a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        <span class="text-xs" :class="isDark ? 'text-white/60' : 'text-gray-600'">
          {{ place.address }}
        </span>
      </div>

      <div v-if="place.phone" class="flex items-center gap-2.5">
        <svg class="w-4 h-4 shrink-0" :class="isDark ? 'text-white/30' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 5a2 2 0 012-2h3.28a1 1 0 01.948.684l1.498 4.493a1 1 0 01-.502 1.21l-2.257 1.13a11.042 11.042 0 005.516 5.516l1.13-2.257a1 1 0 011.21-.502l4.493 1.498a1 1 0 01.684.949V19a2 2 0 01-2 2h-1C9.716 21 3 14.284 3 6V5z" />
        </svg>
        <span class="text-xs" :class="isDark ? 'text-white/60' : 'text-gray-600'">
          {{ place.phone }}
        </span>
      </div>

      <div v-if="place.hours" class="flex items-start gap-2.5">
        <svg class="w-4 h-4 shrink-0 mt-0.5" :class="isDark ? 'text-white/30' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span class="text-xs" :class="isDark ? 'text-white/60' : 'text-gray-600'">
          {{ place.hours }}
        </span>
      </div>

      <div v-if="place.website" class="flex items-center gap-2.5">
        <svg class="w-4 h-4 shrink-0" :class="isDark ? 'text-white/30' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
        </svg>
        <a :href="place.website" target="_blank" rel="noopener"
           class="text-xs underline underline-offset-2"
           :class="isDark ? 'text-white/60 hover:text-white/80' : 'text-gray-600 hover:text-gray-800'">
          {{ websiteDomain }}
        </a>
      </div>

      <div>
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Find on</h4>
        <div class="space-y-2">
          <a
            v-for="src in (place.sources ?? [])"
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
            v-for="link in mapLinks"
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
import { ref, computed, onMounted } from 'vue'
import type { Place } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generatePlaceFallback, fetchPlaceImage } from '@/composables/useImageFallback'

const props = defineProps<{ place: Place }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const fetchedPhoto = ref<string | null>(null)

const fallbackCover = computed(() =>
  generatePlaceFallback(props.place.name, props.place.cuisine || props.place.category)
)

onMounted(() => {
  if (props.place.photoUrl) return
  fetchPlaceImage(props.place.name, props.place.city).then((url) => {
    if (url) fetchedPhoto.value = url
  })
})

const websiteDomain = computed(() => {
  if (!props.place.website) return ''
  try {
    return new URL(props.place.website).hostname.replace(/^www\./, '')
  } catch {
    return props.place.website
  }
})

const q = computed(() =>
  `${props.place.name} ${props.place.city ?? ''}`.trim().replace(/\s+/g, '+'),
)

const mapLinks = computed(() => {
  if ((props.place.sources ?? []).length > 0) return []
  const links = [
    { name: 'OpenStreetMap', url: `https://www.openstreetmap.org/search?query=${q.value}`, icon: '🗺️', desc: 'Open source maps' },
    { name: 'Google Maps', url: `https://www.google.com/maps/search/${q.value}`, icon: '📍', desc: 'Directions & reviews' },
  ]
  if (props.place.lat && props.place.lng) {
    links.unshift({
      name: 'OpenStreetMap',
      url: `https://www.openstreetmap.org/?mlat=${props.place.lat}&mlon=${props.place.lng}#map=17/${props.place.lat}/${props.place.lng}`,
      icon: '🗺️',
      desc: 'Open source maps',
    })
    links.splice(2) // Remove the search-based OSM link
  }
  return links
})

function sourceIcon(type: string): string {
  const icons: Record<string, string> = {
    gmaps: '📍',
    osm: '🗺️',
    yelp: '⭐',
    tripadvisor: '🦉',
    foursquare: '📌',
    local: '💾',
  }
  return icons[type] ?? '📍'
}
</script>
