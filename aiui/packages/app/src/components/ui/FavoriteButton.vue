<template>
  <button
    class="p-1 rounded-full transition-all duration-200 active:scale-125"
    :class="favorited
      ? 'text-accent hover:text-accent/80'
      : isDark
        ? 'text-white/20 hover:text-white/50'
        : 'text-gray-300 hover:text-gray-500'"
    :aria-label="favorited ? 'Remove from favorites' : 'Add to favorites'"
    @click.stop="$emit('toggle')"
  >
    <svg
      class="w-4 h-4 transition-transform duration-200"
      :class="{ 'scale-110': favorited }"
      :fill="favorited ? 'currentColor' : 'none'"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
      />
    </svg>
  </button>
</template>

<script setup lang="ts">
import { useTheme } from '@/composables/useTheme'

defineProps<{
  favorited: boolean
}>()

defineEmits<{
  toggle: []
}>()

const { isDark } = useTheme()
</script>
