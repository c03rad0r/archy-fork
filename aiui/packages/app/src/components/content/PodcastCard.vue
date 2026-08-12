<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select', podcast)"
  >
    <div class="cover-card-sm shrink-0 w-12 h-12 rounded-lg overflow-hidden">
      <img
        v-if="coverSrc"
        :src="coverSrc"
        :alt="podcast.title"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="coverFailed = true"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px] bg-cover bg-center"
        :style="{ backgroundImage: `url(${fallbackCover})` }"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ podcast.title }}</p>
      <p class="text-xs mt-0.5"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ podcast.host || 'Podcast' }}<template v-if="podcast.year"> · {{ podcast.year }}</template>
      </p>
      <div class="flex items-center gap-1.5 mt-1.5">
        <span v-if="isExternal"
              class="text-xs px-1.5 py-0.5 rounded font-medium"
              :class="isDark ? 'bg-info/15 text-info/70' : 'bg-info/10 text-blue-600'">
          not in library
        </span>
        <span
          v-for="src in podcast.sources.slice(0, 3)"
          :key="src.type"
          class="text-xs px-1.5 py-0.5 rounded font-medium"
          :class="isDark ? 'bg-white/8 text-white/50' : 'bg-black/5 text-gray-500'"
        >
          {{ src.type }}
        </span>
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Podcast } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generatePodcastCoverFallback, fetchPodcastCover } from '@/composables/useImageFallback'

const props = defineProps<{ podcast: Podcast }>()
defineEmits<{ select: [podcast: Podcast] }>()

const { isDark } = useTheme()
const coverFailed = ref(false)
const fetchedCover = ref<string | null>(null)

const coverSrc = computed(() => {
  if (coverFailed.value) return null
  return props.podcast.coverUrl || fetchedCover.value || null
})

onMounted(() => {
  if (props.podcast.coverUrl) return
  fetchPodcastCover(props.podcast.title, props.podcast.host).then((url) => {
    if (url) fetchedCover.value = url
  })
})

const fallbackCover = computed(() =>
  generatePodcastCoverFallback(props.podcast.title, props.podcast.host)
)

const isExternal = computed(() => props.podcast.id.startsWith('ext-'))
</script>
