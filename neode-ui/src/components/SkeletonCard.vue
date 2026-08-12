<template>
  <div class="glass-card p-6 animate-pulse" :class="className">
    <!-- Header skeleton -->
    <div v-if="showHeader" class="flex items-start gap-4 mb-4">
      <div class="w-12 h-12 rounded-lg bg-white/10 shrink-0"></div>
      <div class="flex-1 space-y-2">
        <div class="h-4 bg-white/10 rounded w-2/3"></div>
        <div class="h-3 bg-white/5 rounded w-1/3"></div>
      </div>
    </div>

    <!-- Content skeleton lines -->
    <div class="space-y-3">
      <div v-for="i in lines" :key="i" class="h-3 bg-white/8 rounded" :style="{ width: lineWidth(i) }"></div>
    </div>

    <!-- Stats grid skeleton -->
    <div v-if="showStats" class="grid grid-cols-2 md:grid-cols-4 gap-3 mt-4">
      <div v-for="s in 4" :key="s" class="bg-white/5 rounded-lg p-3">
        <div class="h-2 bg-white/8 rounded w-1/2 mb-2"></div>
        <div class="h-5 bg-white/10 rounded w-3/4"></div>
      </div>
    </div>

    <!-- Action buttons skeleton -->
    <div v-if="showActions" class="flex gap-3 mt-4">
      <div class="h-9 bg-white/8 rounded-lg flex-1"></div>
      <div class="h-9 bg-white/8 rounded-lg flex-1"></div>
    </div>
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(defineProps<{
  lines?: number
  showHeader?: boolean
  showStats?: boolean
  showActions?: boolean
  className?: string
}>(), {
  lines: 3,
  showHeader: true,
  showStats: false,
  showActions: false,
  className: '',
})

function lineWidth(index: number): string {
  const widths = ['100%', '85%', '70%', '90%', '60%']
  return widths[(index - 1) % widths.length] ?? '100%'
}
</script>
