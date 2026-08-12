<template>
  <div class="website-detail h-full flex flex-col overflow-hidden">
    <div class="shrink-0 flex items-center gap-2 px-3 py-2.5"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="absolute top-3 left-3 p-2 rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="flex-1 min-w-0 pl-8">
        <p class="text-sm font-medium truncate"
           :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ website.title || 'Website' }}
        </p>
        <p v-if="domain" class="text-xs truncate"
           :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ domain }}
        </p>
      </div>
      <a
        :href="website.url"
        target="_blank"
        rel="noopener noreferrer"
        class="flex items-center justify-center w-8 h-8 rounded-lg transition-colors shrink-0"
        :class="isDark ? 'hover:bg-white/10 text-white/50' : 'hover:bg-black/5 text-gray-400'"
        aria-label="Open in new tab"
        title="Open in new tab"
        @click.stop
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
        </svg>
      </a>
    </div>
    <div class="flex-1 min-h-0 relative bg-black/20">
      <iframe
        :key="website.url"
        :src="website.url"
        class="absolute inset-0 w-full h-full border-0"
        style="-ms-overflow-style: none; scrollbar-width: none;"
        title="Website content"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { WebSearchResult } from '@aiui/core/types/message'
import { useTheme } from '@/composables/useTheme'

const props = defineProps<{ website: WebSearchResult }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()

const domain = computed(() => {
  if (!props.website.url) return ''
  try {
    return new URL(props.website.url).hostname.replace(/^www\./, '')
  } catch {
    return ''
  }
})
</script>

<style scoped>
iframe::-webkit-scrollbar {
  display: none;
}
</style>
