<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select', series)"
  >
    <div class="poster-card-sm shrink-0 w-12 aspect-[2/3] rounded-lg overflow-hidden">
      <img
        v-if="posterSrc"
        :src="posterSrc"
        :alt="series.title"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="posterFailed = true"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px] bg-cover bg-center"
        :style="{ backgroundImage: `url(${fallbackPoster})` }"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ series.title }}</p>
      <p class="text-xs mt-0.5"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ yearDisplay }}<template v-if="series.network"> · {{ series.network }}</template>
      </p>
      <p v-if="series.synopsis"
         class="text-xs mt-0.5 line-clamp-2"
         :class="isDark ? 'text-white/35' : 'text-gray-400'">
        {{ series.synopsis }}
      </p>
      <div class="flex items-center gap-1.5 mt-1.5">
        <span v-if="series.rating && series.rating > 0"
              class="text-xs font-semibold px-1.5 py-0.5 rounded"
              :class="ratingClass">
          ★ {{ series.rating.toFixed(1) }}
        </span>
        <span v-if="series.seasons"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-white/8 text-white/50' : 'bg-black/5 text-gray-500'">
          {{ series.seasons }}S
        </span>
        <span v-if="series.status === 'ongoing'"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-success/15 text-success/70' : 'bg-green-50 text-green-600'">
          ongoing
        </span>
        <span v-else-if="series.status === 'ended'"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-white/8 text-white/40' : 'bg-black/5 text-gray-400'">
          ended
        </span>
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { TVSeries } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generateTVSeriesFallback, fetchTVImage } from '@/composables/useImageFallback'

const props = defineProps<{ series: TVSeries }>()
defineEmits<{ select: [series: TVSeries] }>()

const { isDark } = useTheme()
const posterFailed = ref(false)
const fetchedPoster = ref<string | null>(null)

const posterSrc = computed(() => {
  if (posterFailed.value) return null
  return props.series.posterUrl || fetchedPoster.value
})

const fallbackPoster = computed(() =>
  generateTVSeriesFallback(props.series.title, props.series.year)
)

const yearDisplay = computed(() => {
  if (!props.series.year) return ''
  if (props.series.endYear && props.series.endYear !== props.series.year) {
    return `${props.series.year}–${props.series.endYear}`
  }
  if (props.series.status === 'ongoing') return `${props.series.year}–`
  return String(props.series.year)
})

const ratingClass = computed(() => {
  const r = props.series.rating ?? 0
  if (r >= 8.5) return isDark.value ? 'bg-success/20 text-success' : 'bg-success/10 text-green-700'
  if (r >= 7.5) return isDark.value ? 'bg-accent/20 text-accent' : 'bg-accent/10 text-amber-700'
  return isDark.value ? 'bg-white/10 text-white/50' : 'bg-black/5 text-gray-500'
})

onMounted(() => {
  if (props.series.posterUrl) return
  fetchTVImage(props.series.title, props.series.year).then((result) => {
    if (result.posterUrl) fetchedPoster.value = result.posterUrl
  })
})
</script>
