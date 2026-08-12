<template>
  <div class="image-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden bg-black/20">
      <img
        v-if="!imgFailed"
        :src="image.url"
        :alt="image.alt || image.title || 'Image'"
        class="w-full block max-h-[60vh] object-contain bg-black/40"
        @error="imgFailed = true"
      />
      <div
        v-else
        class="w-full aspect-video flex items-center justify-center"
        :class="isDark ? 'bg-white/5' : 'bg-black/5'"
      >
        <svg class="w-12 h-12" :class="isDark ? 'text-white/15' : 'text-gray-300'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </div>

      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
    </div>

    <div class="p-4 space-y-3">
      <h2 v-if="image.title" class="text-base font-bold"
          :class="isDark ? 'text-white/90' : 'text-gray-900'">
        {{ image.title }}
      </h2>

      <p v-if="image.description" class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/70' : 'text-gray-600'">
        {{ image.description }}
      </p>

      <div v-if="image.attribution" class="text-xs"
           :class="isDark ? 'text-white/50' : 'text-gray-500'">
        {{ image.attribution }}
      </div>

      <div v-if="image.source" class="text-xs"
           :class="isDark ? 'text-white/40' : 'text-gray-400'">
        Source: {{ image.source }}
      </div>

      <div v-if="image.width && image.height" class="text-xs"
           :class="isDark ? 'text-white/30' : 'text-gray-400'">
        {{ image.width }} &times; {{ image.height }}
      </div>

      <div class="pt-2">
        <a
          :href="image.url"
          target="_blank"
          rel="noopener"
          class="inline-flex items-center gap-2 px-4 min-h-[44px] rounded-xl text-xs font-medium transition-colors"
          :class="isDark
            ? 'bg-white/5 hover:bg-white/10 text-white/80'
            : 'bg-black/3 hover:bg-black/5 text-gray-800'"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
          Open original
        </a>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { ImageItem } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'

defineProps<{ image: ImageItem }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const imgFailed = ref(false)
</script>
