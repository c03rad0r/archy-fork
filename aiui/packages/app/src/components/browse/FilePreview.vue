<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3 border-b border-white/5 shrink-0">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-white/80 truncate">{{ file.name }}</p>
        <p class="text-xs text-white/30 truncate mt-0.5">{{ file.path }}</p>
      </div>
      <div class="flex items-center gap-2 shrink-0 ml-3">
        <span class="text-xs text-white/25 font-mono">{{ formatSize(file.size) }}</span>
        <button
          class="min-w-[32px] min-h-[32px] flex items-center justify-center rounded-md text-white/40 hover:text-white/70 hover:bg-white/10 transition-colors"
          aria-label="Close preview"
          @click="$emit('close')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-auto">
      <table class="text-xs font-mono leading-relaxed w-full">
        <tbody>
          <tr v-for="(line, i) in lines" :key="i" class="hover:bg-white/3">
            <td class="text-white/20 text-right pr-4 pl-4 py-0 select-none align-top whitespace-nowrap sticky left-0 bg-[#0a0a0a]">{{ i + 1 }}</td>
            <td class="text-white/70 pr-4 py-0 whitespace-pre">{{ line }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  file: {
    name: string
    path: string
    content: string
    size: number
  }
}>()

defineEmits<{ close: [] }>()

const lines = computed(() => props.file.content.split('\n'))

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
</script>
