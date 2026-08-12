<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select', place)"
  >
    <div class="shrink-0 w-12 h-12 rounded-lg overflow-hidden">
      <img
        v-if="photoSrc"
        :src="photoSrc"
        :alt="place.name"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="photoFailed = true"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px] bg-cover bg-center"
        :style="{ backgroundImage: `url(${fallbackPhoto})` }"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ place.name }}</p>
      <p class="text-xs mt-0.5 truncate"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ place.cuisine || place.category }}<template v-if="place.city"> · {{ place.city }}</template>
      </p>
      <div class="flex items-center gap-1.5 mt-1.5">
        <span v-if="place.rating && place.rating > 0"
              class="text-xs font-semibold px-1.5 py-0.5 rounded"
              :class="ratingClass">
          ★ {{ place.rating.toFixed(1) }}
        </span>
        <span v-if="place.priceLevel"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-white/8 text-white/50' : 'bg-black/5 text-gray-500'">
          {{ '$'.repeat(place.priceLevel) }}
        </span>
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Place } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generatePlaceFallback, fetchPlaceImage } from '@/composables/useImageFallback'

const props = defineProps<{ place: Place }>()
defineEmits<{ select: [place: Place] }>()

const { isDark } = useTheme()
const photoFailed = ref(false)
const fetchedPhoto = ref<string | null>(null)

const photoSrc = computed(() => {
  if (photoFailed.value) return null
  return props.place.photoUrl || fetchedPhoto.value || null
})

onMounted(() => {
  if (props.place.photoUrl) return
  fetchPlaceImage(props.place.name, props.place.city).then((url) => {
    if (url) fetchedPhoto.value = url
  })
})

const fallbackPhoto = computed(() =>
  generatePlaceFallback(props.place.name, props.place.cuisine || props.place.category)
)

const ratingClass = computed(() => {
  const r = props.place.rating ?? 0
  if (r >= 4.5) return isDark.value ? 'bg-success/20 text-success' : 'bg-success/10 text-green-700'
  if (r >= 4.0) return isDark.value ? 'bg-accent/20 text-accent' : 'bg-accent/10 text-amber-700'
  return isDark.value ? 'bg-white/10 text-white/50' : 'bg-black/5 text-gray-500'
})
</script>
