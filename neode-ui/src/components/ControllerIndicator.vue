<template>
  <div
    v-if="store.isActive"
    class="flex items-center gap-2 px-3 py-2 rounded-lg bg-white/5 border border-white/10"
    title="Controller connected - use arrows & Enter to navigate"
  >
    <svg class="w-5 h-5 text-amber-400/90 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="4" y="8" width="16" height="10" rx="2" stroke-width="2" />
      <circle cx="9" cy="13" r="1.5" fill="currentColor" />
      <circle cx="15" cy="13" r="1.5" fill="currentColor" />
      <path stroke-linecap="round" stroke-width="2" d="M12 10v2M11 11h2" />
    </svg>
    <span class="text-xs text-white/70 hidden sm:inline">Controller</span>
  </div>
</template>

<script setup lang="ts">
import { useControllerStore } from '@/stores/controller'

const store = useControllerStore()
</script>
