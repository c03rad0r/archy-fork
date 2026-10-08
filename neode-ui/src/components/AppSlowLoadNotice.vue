<template>
    <div role="status"
      class="absolute top-3 left-3 right-3 z-10 flex items-center gap-2 rounded-lg border border-white/10 bg-black/80 px-3 py-2 text-xs text-white/75 backdrop-blur-md">
      <span class="min-w-0 flex-1">{{ title }} is taking longer to load. You can keep waiting.</span>
      <button type="button" class="shrink-0 rounded px-2 py-1 text-white hover:bg-white/10" @click="$emit('refresh')">Retry</button>
      <button type="button" class="shrink-0 rounded p-1 text-white/60 hover:bg-white/10" aria-label="Dismiss loading notice" @click="$emit('dismissSlowLoad')">
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true"><path stroke-linecap="round" stroke-width="2" d="m6 6 12 12M18 6 6 18" /></svg>
      </button>
    </div>
</template>

<script setup lang="ts">
defineProps<{ title: string }>()
defineEmits<{ refresh: []; dismissSlowLoad: [] }>()
</script>
