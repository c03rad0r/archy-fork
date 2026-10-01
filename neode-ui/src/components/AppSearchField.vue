<template>
  <div class="relative min-w-0 flex-1">
    <input
      ref="input"
      v-model="query"
      type="text"
      :placeholder="placeholder"
      :aria-label="label"
      data-controller-no-submit
      class="app-header-search w-full pr-10 text-white placeholder-white/50 focus:outline-none transition-colors"
      @keydown.esc.prevent="clear"
    />
    <button
      v-if="query.length"
      type="button"
      aria-label="Clear search"
      title="Clear search"
      class="absolute right-1 top-1/2 -translate-y-1/2 w-8 h-8 flex items-center justify-center rounded-lg text-white/50 hover:text-white hover:bg-white/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-orange-400/60 transition-colors"
      @click="clear"
    >
      <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m6 6 12 12M6 18 18 6" />
      </svg>
    </button>
  </div>
</template>
<script setup lang="ts">
import { nextTick, ref } from 'vue'
defineProps<{ placeholder: string; label: string }>()
const query = defineModel<string>({ default: '' })
const input = ref<HTMLInputElement | null>(null)
async function clear() {
  query.value = ''
  await nextTick()
  input.value?.focus()
}
</script>
