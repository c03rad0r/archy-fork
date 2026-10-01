<template>
  <div v-if="task" :class="floating ? 'fixed z-50 top-20 left-4 right-4 md:left-auto md:w-96' : 'mb-3 shrink-0'">
    <div class="glass-card relative overflow-hidden h-11 px-3 flex items-center gap-2" role="status" aria-live="polite">
      <div v-if="task.active" class="absolute inset-y-0 left-0 bg-emerald-400/10 transition-[width] duration-200 pointer-events-none" :style="{ width: `${percent}%` }" />
      <div v-if="task.active" class="absolute bottom-0 left-0 h-0.5 bg-emerald-400 transition-[width] duration-200" :style="{ width: `${percent}%` }" role="progressbar" :aria-valuenow="percent" aria-valuemin="0" aria-valuemax="100" :aria-label="`Uploading ${task.filename}`" />
      <span class="relative min-w-0 flex-1 text-sm truncate" :class="task.error ? 'text-red-300' : 'text-white/80'" :title="label">{{ label }}</span>
      <span v-if="task.active" class="relative shrink-0 text-xs tabular-nums text-white/60">{{ percent }}%</span>
      <button class="relative shrink-0 w-8 h-8 flex items-center justify-center rounded-lg text-white/60 hover:text-white hover:bg-white/10" :aria-label="task.active ? 'Cancel upload' : 'Dismiss upload'" @click="task.active ? store.cancelUpload() : store.dismissUpload()">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-width="2" d="m6 6 12 12M6 18 18 6" /></svg>
      </button>
    </div>
  </div>
</template>
<script setup lang="ts">
import { computed } from 'vue'
import { useCloudStore } from '@/stores/cloud'
defineProps<{ floating?: boolean }>()
const store = useCloudStore()
const task = computed(() => store.upload)
const percent = computed(() => !task.value ? 0 : task.value.total ? Math.min(100, Math.floor(task.value.sent * 100 / task.value.total)) : task.value.active ? 0 : 100)
const label = computed(() => {
  const t = task.value
  if (!t) return ''
  if (t.error) return t.error
  if (t.cancelled) return `Upload stopped · ${t.completed}/${t.count} saved`
  if (!t.active) return `${t.count === 1 ? t.filename : `${t.count} files`} uploaded`
  return `${percent.value === 100 ? 'Saving' : 'Uploading'} ${t.filename}${t.count > 1 ? ` · ${t.completed + 1}/${t.count}` : ''}`
})
</script>
