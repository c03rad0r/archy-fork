<template>
  <button
    class="flex items-start gap-3 w-full text-left p-2.5 rounded-xl transition-all duration-150"
    :class="isDark
      ? 'hover:bg-white/5'
      : 'hover:bg-black/3'"
    @click="$emit('select', image)"
  >
    <div class="w-16 shrink-0 rounded-lg overflow-hidden">
      <div class="aspect-[4/3] relative bg-black/10">
        <img
          v-if="!imgFailed"
          :src="image.url"
          :alt="image.alt || image.title || 'Image'"
          class="w-full h-full object-cover"
          loading="lazy"
          @error="imgFailed = true"
        />
        <div
          v-else
          class="w-full h-full bg-cover bg-center"
          :style="{ backgroundImage: `url(${fallbackImg})` }"
        />
      </div>
    </div>
    <div class="flex-1 min-w-0 py-0.5">
      <p v-if="image.title" class="text-sm font-medium leading-snug line-clamp-1"
         :class="isDark ? 'text-white/90' : 'text-gray-900'">
        {{ image.title }}
      </p>
      <p v-if="image.source" class="text-xs mt-0.5 truncate"
         :class="isDark ? 'text-white/50' : 'text-gray-500'">
        {{ image.source }}
      </p>
      <p v-if="image.description" class="text-xs mt-1 line-clamp-2 leading-relaxed"
         :class="isDark ? 'text-white/40' : 'text-gray-400'">
        {{ image.description }}
      </p>
    </div>
  </button>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ImageItem } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generateImageFallback } from '@/composables/useImageFallback'

const props = defineProps<{ image: ImageItem }>()
defineEmits<{ select: [image: ImageItem] }>()

const { isDark } = useTheme()
const imgFailed = ref(false)

const fallbackImg = computed(() =>
  generateImageFallback(props.image.title || props.image.alt || 'Image')
)
</script>
