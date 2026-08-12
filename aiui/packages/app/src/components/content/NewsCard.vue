<template>
  <button
    class="flex gap-3 p-2 rounded-xl transition-all duration-200 text-left w-full group overflow-hidden"
    :class="isDark
      ? 'hover:bg-white/5 active:bg-white/10'
      : 'hover:bg-black/[0.03] active:bg-black/5'"
    @click="$emit('select-article', article)"
  >
    <div class="cover-card-sm shrink-0 w-12 h-12 rounded-lg overflow-hidden">
      <img
        v-if="imgSrc"
        :src="imgSrc"
        :alt="article.title"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="imgFailed = true"
      />
      <div
        v-else
        class="w-full h-full rounded-[6px] bg-cover bg-center"
        :style="{ backgroundImage: `url(${fallbackImg})` }"
      />
    </div>

    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">{{ article.title }}</p>
      <p v-if="article.content"
         class="text-xs mt-0.5 line-clamp-2"
         :class="isDark ? 'text-white/40' : 'text-gray-500'">
        {{ article.content }}
      </p>
      <p class="text-xs mt-1 truncate"
         :class="isDark ? 'text-white/30' : 'text-gray-400'">
        {{ formatDomain(article.url) }}
      </p>
    </div>
    <svg class="w-4 h-4 shrink-0 self-center opacity-50"
         :class="isDark ? 'text-white/50' : 'text-gray-400'"
         fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
    </svg>
  </button>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { WebSearchResult } from '@aiui/core/types/message'
import { useTheme } from '@/composables/useTheme'
import { isSafeUrl, formatDomain } from '@/utils/html'
import { generateNewsFallback } from '@/composables/useImageFallback'

const props = defineProps<{ article: WebSearchResult }>()
defineEmits<{ 'select-article': [article: WebSearchResult] }>()

const { isDark } = useTheme()
const imgFailed = ref(false)

const imgSrc = computed(() => {
  if (imgFailed.value) return null
  const u = props.article.imgSrc
  return isSafeUrl(u) ? u : null
})

const fallbackImg = computed(() =>
  generateNewsFallback(props.article.title, formatDomain(props.article.url))
)

</script>
