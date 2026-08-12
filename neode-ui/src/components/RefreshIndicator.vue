<template>
  <div class="refresh-indicator-slot" aria-hidden="false">
    <div
      v-if="state === 'refreshing'"
      class="refresh-indicator"
      role="status"
      aria-live="polite"
    >
      <span class="refresh-indicator-spinner" aria-hidden="true" />
      <span class="sr-only">{{ label ?? 'Refreshing…' }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ResourceLoadState } from '@/stores/resources'

// Subtle background-refresh indicator (D-05): visible only while a cached
// resource is revalidating behind already-rendered content. A first load
// is the view's own skeleton's job, not this component's — 'loading'
// intentionally renders nothing here, same as 'ready'/'idle'. No stale-age
// text or "last updated" badge (D-05 rules those out).
defineProps<{
  state: ResourceLoadState
  label?: string
}>()
</script>

<style scoped>
/* Fixed-size outer slot so the indicator appearing/disappearing never
   shifts surrounding layout — callers can drop this inline without also
   reserving space themselves. */
.refresh-indicator-slot {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
}

.refresh-indicator {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.refresh-indicator-spinner {
  width: 0.875rem;
  height: 0.875rem;
  border: 2px solid rgba(255, 255, 255, 0.15);
  border-top-color: rgba(255, 255, 255, 0.55);
  border-radius: 50%;
  animation: refresh-indicator-spin 0.8s linear infinite;
}

@keyframes refresh-indicator-spin {
  to { transform: rotate(360deg); }
}
</style>
