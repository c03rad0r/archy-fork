<template>
  <div
    v-if="results.length > 0 || isSearching"
    class="absolute bottom-full left-0 right-0 mb-1 max-h-72 overflow-y-auto rounded-xl shadow-lg z-50"
    :class="isDark
      ? 'bg-gray-900/95 backdrop-blur-xl border border-white/10'
      : 'bg-white/95 backdrop-blur-xl border border-gray-200'"
  >
    <div v-if="isSearching" class="p-3 text-center">
      <span class="text-xs" :class="isDark ? 'text-white/40' : 'text-gray-400'">
        Searching...
      </span>
    </div>

    <template v-else>
      <div v-for="group in groupedResults" :key="group.type" class="py-1">
        <div
          class="px-3 py-1 text-xs font-bold uppercase tracking-wider"
          :class="isDark ? 'text-white/30' : 'text-gray-400'"
        >
          {{ group.label }}
        </div>
        <button
          v-for="(result, i) in group.items"
          :key="result.id"
          class="w-full text-left px-3 py-2 flex items-center gap-2 transition-colors"
          :class="[
            selectedIndex === group.startIndex + i
              ? isDark ? 'bg-white/10' : 'bg-gray-100'
              : isDark ? 'hover:bg-white/5' : 'hover:bg-gray-50'
          ]"
          @click="$emit('select', result)"
          @mouseenter="selectedIndex = group.startIndex + i"
        >
          <span
            class="text-xs w-5 h-5 rounded flex items-center justify-center shrink-0"
            :class="typeColor(result.type)"
          >
            {{ typeIcon(result.type) }}
          </span>
          <div class="min-w-0 flex-1">
            <div
              class="text-xs font-medium truncate"
              :class="isDark ? 'text-white/80' : 'text-gray-800'"
            >
              {{ result.title }}
            </div>
            <div
              class="text-xs truncate"
              :class="isDark ? 'text-white/40' : 'text-gray-500'"
            >
              {{ result.subtitle }}
            </div>
          </div>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import type { SearchResult } from '@/composables/useFederatedSearch'

const props = defineProps<{
  results: SearchResult[]
  isSearching: boolean
}>()

defineEmits<{
  select: [result: SearchResult]
}>()

const { isDark } = useTheme()
const selectedIndex = ref(0)

interface ResultGroup {
  type: string
  label: string
  items: SearchResult[]
  startIndex: number
}

const groupedResults = computed<ResultGroup[]>(() => {
  const groups: Record<string, SearchResult[]> = {}
  for (const r of props.results) {
    if (!groups[r.type]) groups[r.type] = []
    groups[r.type].push(r)
  }

  const labels: Record<string, string> = {
    film: 'Films',
    song: 'Songs',
    podcast: 'Podcasts',
  }

  let idx = 0
  return Object.entries(groups).map(([type, items]) => {
    const group = { type, label: labels[type] ?? type, items, startIndex: idx }
    idx += items.length
    return group
  })
})

function typeIcon(type: string): string {
  const icons: Record<string, string> = { film: '🎬', song: '🎵', podcast: '🎙' }
  return icons[type] ?? '📄'
}

function typeColor(type: string): string {
  if (isDark.value) {
    const colors: Record<string, string> = {
      film: 'bg-blue-500/20 text-blue-400',
      song: 'bg-green-500/20 text-green-400',
      podcast: 'bg-orange-500/20 text-orange-400',
    }
    return colors[type] ?? 'bg-white/10 text-white/40'
  }
  const colors: Record<string, string> = {
    film: 'bg-blue-50 text-blue-600',
    song: 'bg-green-50 text-green-600',
    podcast: 'bg-orange-50 text-orange-600',
  }
  return colors[type] ?? 'bg-gray-50 text-gray-600'
}

defineExpose({ selectedIndex })
</script>
