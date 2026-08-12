<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3 border-b border-white/5 shrink-0">
      <h2 class="text-sm font-bold text-white/90">
        {{ title }}
      </h2>
      <button
        class="min-w-[32px] min-h-[32px] flex items-center justify-center rounded-md text-white/40 hover:text-white/70 hover:bg-white/10 transition-colors"
        aria-label="Close help"
        @click="$emit('close')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto px-4 py-4 space-y-4">
      <section v-for="section in sections" :key="section.title">
        <h3 class="text-xs font-semibold text-white/60 uppercase tracking-wider mb-2">
          {{ section.title }}
        </h3>
        <div class="space-y-1.5">
          <div
            v-for="item in section.items"
            :key="item.label"
            class="flex items-start gap-3 px-2 py-1.5 rounded-md"
          >
            <span class="text-xs text-white/30 font-mono shrink-0 mt-0.5 min-w-[80px]">{{ item.label }}</span>
            <span class="text-xs text-white/60">{{ item.description }}</span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  title: string
  sections: {
    title: string
    items: { label: string; description: string }[]
  }[]
}>()

defineEmits<{ close: [] }>()
</script>
