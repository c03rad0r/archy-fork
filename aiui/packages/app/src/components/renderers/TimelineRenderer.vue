<template>
  <div class="timeline-renderer my-4">
    <div class="relative">
      <!-- Vertical line -->
      <div class="absolute left-4 md:left-1/2 top-0 bottom-0 w-px bg-white/10 md:-translate-x-px" />

      <!-- Timeline entries -->
      <div
        v-for="(event, i) in events"
        :key="i"
        class="relative flex items-start gap-4 mb-6 animate-fade-up-fast"
        :class="i % 2 === 0 ? 'md:flex-row' : 'md:flex-row-reverse'"
        :style="{ animationDelay: `${i * 80}ms` }"
      >
        <!-- Dot -->
        <div
          class="absolute left-4 md:left-1/2 w-3 h-3 rounded-full bg-accent border-2 border-black z-10 md:-translate-x-1.5"
          :style="{ top: '6px' }"
        />

        <!-- Date (mobile: inline, desktop: left/right side) -->
        <div class="hidden md:block w-[calc(50%-2rem)] text-right shrink-0" :class="i % 2 === 0 ? '' : 'order-last text-left'">
          <p class="text-xs text-white/40 tabular-nums pt-1">{{ formatDate(event.date) }}</p>
        </div>

        <!-- Card -->
        <div class="ml-10 md:ml-0 md:w-[calc(50%-2rem)] shrink-0">
          <div class="rounded-lg bg-white/5 border border-white/10 px-3 py-2">
            <p class="text-xs text-white/30 md:hidden tabular-nums">{{ formatDate(event.date) }}</p>
            <h4 class="text-xs font-medium text-white/90">{{ event.title }}</h4>
            <p v-if="event.location" class="text-xs text-white/40 mt-0.5">{{ event.location }}</p>
            <p v-if="event.description" class="text-xs text-white/50 mt-1 line-clamp-2">{{ event.description }}</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { EventData } from '@/composables/contentExtraction'

defineProps<{
  events: EventData[]
}>()

function formatDate(dateStr: string): string {
  if (!dateStr) return ''
  const d = new Date(dateStr)
  if (isNaN(d.getTime())) return dateStr
  return d.toLocaleDateString('en', { year: 'numeric', month: 'short', day: 'numeric' })
}
</script>
