<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" :style="isDark
      ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
      : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
    >
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ title }}
        </h3>
        <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ filteredApps.length }} apps
        </span>
      </div>

      <input
        v-model="search"
        type="text"
        placeholder="Search apps..."
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
          : 'bg-black/3 text-gray-800 placeholder:text-gray-400 focus:bg-black/5'"
      />

      <div class="flex flex-wrap gap-1.5">
        <button
          v-for="cat in categories"
          :key="cat.value"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeCategory === cat.value
            ? 'nav-tab-active'
            : isDark
              ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
              : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
          @click="activeCategory = activeCategory === cat.value ? null : cat.value"
        >
          {{ cat.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="space-y-2">
        <button
          v-for="app in filteredApps"
          :key="app.id"
          class="w-full text-left p-3 rounded-xl transition-all duration-200 flex items-start gap-3"
          :class="isDark
            ? 'bg-white/5 hover:bg-white/10'
            : 'bg-black/3 hover:bg-black/5'"
          :aria-label="app.name"
          @click="$emit('selectApp', app)"
        >
          <div
            class="w-10 h-10 rounded-xl flex items-center justify-center text-lg font-bold shrink-0"
            :style="{ background: appGradient(app.id) }"
          >
            <span class="text-white/90">{{ app.name.charAt(0) }}</span>
          </div>

          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-xs font-semibold truncate"
                 :class="isDark ? 'text-white/90' : 'text-gray-900'">
                {{ app.name }}
              </p>
              <span
                class="text-xs px-1.5 py-0.5 rounded font-medium shrink-0"
                :class="isDark ? 'bg-white/10 text-white/50' : 'bg-black/5 text-gray-500'"
              >
                {{ categoryLabel(app.category) }}
              </span>
            </div>
            <p class="text-xs mt-0.5 line-clamp-2"
               :class="isDark ? 'text-white/50' : 'text-gray-500'">
              {{ app.description }}
            </p>
            <div class="flex gap-1 mt-1.5">
              <span
                v-for="p in app.platforms"
                :key="p"
                class="text-xs px-1 py-0.5 rounded"
                :class="isDark ? 'bg-white/5 text-white/30' : 'bg-black/3 text-gray-400'"
              >
                {{ platformLabel(p) }}
              </span>
            </div>
          </div>
        </button>
      </div>

      <div v-if="filteredApps.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No apps match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { AppEntry } from '@/data/apps'
import { useTheme } from '@/composables/useTheme'

const props = withDefaults(defineProps<{
  apps: AppEntry[]
  title?: string
}>(), {
  title: 'Recommended Apps',
})

defineEmits<{ selectApp: [app: AppEntry] }>()

const { isDark } = useTheme()
const search = ref('')
const activeCategory = ref<string | null>(null)

const categories = [
  { value: 'nostr-client', label: 'Nostr' },
  { value: 'lightning-wallet', label: 'Lightning' },
  { value: 'bitcoin-wallet', label: 'Bitcoin' },
  { value: 'privacy', label: 'Privacy' },
  { value: 'node', label: 'Nodes' },
  { value: 'dev-tool', label: 'Dev' },
]

function categoryLabel(cat: string): string {
  return categories.find(c => c.value === cat)?.label ?? cat
}

function platformLabel(p: string): string {
  const labels: Record<string, string> = {
    ios: 'iOS',
    android: 'Android',
    web: 'Web',
    desktop: 'Desktop',
    cli: 'CLI',
    nodeos: 'Node',
  }
  return labels[p] ?? p
}

function appGradient(id: string): string {
  let hash = 0
  for (let i = 0; i < id.length; i++) hash = id.charCodeAt(i) + ((hash << 5) - hash)
  const hue = Math.abs(hash % 360)
  return `linear-gradient(135deg, hsl(${hue}, 60%, 35%), hsl(${(hue + 40) % 360}, 50%, 25%))`
}

const filteredApps = computed(() => {
  let result = props.apps
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(
      a => a.name.toLowerCase().includes(q) ||
           a.description.toLowerCase().includes(q) ||
           a.keywords.some(k => k.toLowerCase().includes(q))
    )
  }
  if (activeCategory.value) {
    result = result.filter(a => a.category === activeCategory.value)
  }
  return result
})
</script>
