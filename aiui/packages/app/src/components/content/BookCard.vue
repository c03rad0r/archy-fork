<template>
  <button
    class="flex items-start gap-3 w-full text-left p-2.5 rounded-xl transition-all duration-150"
    :class="isDark
      ? 'hover:bg-white/5'
      : 'hover:bg-black/3'"
    @click="$emit('select', book)"
  >
    <div class="w-12 h-auto shrink-0 rounded-md overflow-hidden shadow-md">
      <div class="aspect-[2/3] relative">
        <img
          v-if="coverSrc"
          :src="coverSrc"
          :alt="book.title"
          class="w-full h-full object-cover"
          loading="lazy"
          @error="coverFailed = true"
        />
        <div
          v-else
          class="w-full h-full bg-cover bg-center"
          :style="{ backgroundImage: `url(${fallbackCover})` }"
        />
      </div>
    </div>
    <div class="flex-1 min-w-0 py-0.5">
      <p class="text-sm font-medium leading-snug line-clamp-2"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">
        {{ book.title }}
      </p>
      <p class="text-xs mt-0.5 truncate"
         :class="isDark ? 'text-white/50' : 'text-gray-500'">
        {{ book.author }}<span v-if="book.year"> · {{ book.year }}</span>
      </p>
      <p v-if="book.description" class="text-xs mt-1 line-clamp-2 leading-relaxed"
         :class="isDark ? 'text-white/40' : 'text-gray-400'">
        {{ book.description }}
      </p>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Book } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generateBookCoverFallback, fetchBookImage } from '@/composables/useImageFallback'

const props = defineProps<{ book: Book }>()
defineEmits<{ select: [book: Book] }>()

const { isDark } = useTheme()
const coverFailed = ref(false)
const fetchedCover = ref<string | null>(null)

const coverSrc = computed(() => {
  if (coverFailed.value) return null
  return props.book.coverUrl || fetchedCover.value
})

const fallbackCover = computed(() =>
  generateBookCoverFallback(props.book.title, props.book.author)
)

onMounted(() => {
  if (props.book.coverUrl) return
  fetchBookImage(props.book.title, props.book.author).then((url) => {
    if (url) fetchedCover.value = url
  })
})
</script>
