<template>
  <button
    class="fixed z-50 w-14 h-14 rounded-full path-glass-button flex items-center justify-center
           transition-all duration-300 hover:scale-105 active:scale-95"
    :class="positionClasses"
    :style="{ boxShadow: '0 8px 24px rgba(0, 0, 0, 0.45), 0 0 20px rgba(247, 147, 26, 0.15)' }"
    :aria-label="isOpen ? 'Close AIUI' : 'Open AIUI'"
    @click="$emit('toggle')"
  >
    <svg
      class="w-6 h-6 text-accent transition-transform duration-300"
      :class="isOpen ? 'rotate-45' : ''"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path
        v-if="!isOpen"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
      />
      <path
        v-else
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M12 4v16m8-8H4"
      />
    </svg>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    isOpen: boolean
    position?: 'bottom-right' | 'bottom-left'
  }>(),
  { position: 'bottom-right' }
)

defineEmits<{ toggle: [] }>()

const positionClasses = computed(() =>
  props.position === 'bottom-left'
    ? 'bottom-6 left-6'
    : 'bottom-6 right-6'
)
</script>
