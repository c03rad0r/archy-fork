<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" :style="isDark
      ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
      : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
    >
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ title }}
        </h3>
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            {{ images.length }} images
          </span>
          <slot name="header-actions" />
        </div>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="columns-2 sm:columns-3 gap-3 space-y-3">
        <button
          v-for="img in images"
          :key="img.id"
          class="group w-full break-inside-avoid text-left rounded-xl overflow-hidden transition-all duration-200 hover:brightness-110 relative"
          :class="isDark ? 'bg-white/5' : 'bg-black/3'"
          :aria-label="img.alt || img.title || 'Image'"
          @click="$emit('selectImage', img)"
        >
          <img
            v-if="!failedIds.has(img.id)"
            :src="img.url"
            :alt="img.alt || img.title || 'Image'"
            class="w-full block transition-transform duration-300 group-hover:scale-[1.03]"
            loading="lazy"
            @error="onError(img)"
          />
          <div
            v-else
            class="w-full aspect-[4/3] flex items-center justify-center"
            :class="isDark ? 'bg-white/5' : 'bg-black/5'"
          >
            <svg class="w-8 h-8" :class="isDark ? 'text-white/15' : 'text-gray-300'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
          </div>
          <div v-if="img.title || img.source"
               class="absolute bottom-0 left-0 right-0 p-2 bg-gradient-to-t from-black/70 via-black/30 to-transparent">
            <p v-if="img.title" class="text-xs font-medium text-white/90 truncate">{{ img.title }}</p>
            <p v-if="img.source" class="text-xs text-white/50 truncate">{{ img.source }}</p>
          </div>
        </button>
      </div>

      <div v-if="images.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No images found
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { ImageItem } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'

withDefaults(defineProps<{
  images: ImageItem[]
  title?: string
}>(), {
  title: 'Images',
})

defineEmits<{ selectImage: [image: ImageItem] }>()

const { isDark } = useTheme()
const failedIds = ref<Set<string>>(new Set())

function onError(img: ImageItem) {
  failedIds.value.add(img.id)
  failedIds.value = new Set(failedIds.value)
}
</script>
