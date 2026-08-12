<template>
  <button
    class="absolute top-3 right-3 z-10 p-2 rounded-lg path-glass-icon transition-colors"
    :class="isDark
      ? 'text-white/70 hover:bg-white/10'
      : 'text-gray-500 hover:bg-black/5 hover:text-gray-800'"
    title="Close"
    aria-label="Close"
    @click="$emit('click')"
  >
    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
    </svg>
  </button>
</template>

<script setup lang="ts">
import { useTheme } from '@/composables/useTheme'

const { isDark } = useTheme()
defineEmits<{ click: [] }>()
</script>
