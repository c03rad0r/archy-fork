<template>
  <div class="flex flex-col items-center justify-center py-12 px-4 text-center">
    <div class="w-16 h-16 rounded-2xl bg-white/5 flex items-center justify-center mb-4">
      <span class="text-3xl">{{ icon }}</span>
    </div>
    <h3 class="text-lg font-semibold text-white/80 mb-2">{{ title }}</h3>
    <p class="text-sm text-white/50 max-w-sm mb-6">{{ description }}</p>
    <button
      v-if="actionLabel"
      @click="$emit('action')"
      class="glass-button px-5 py-2.5 rounded-lg text-sm font-medium"
    >
      {{ actionLabel }}
    </button>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  icon?: string
  title: string
  description: string
  actionLabel?: string
}>()

defineEmits<{
  action: []
}>()
</script>
