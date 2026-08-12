<template>
  <div class="app-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden">
      <div
        class="w-full aspect-[16/7] flex items-center justify-center"
        :style="{ background: appGradient }"
      >
        <span class="text-5xl font-bold text-white/20">{{ app.name.charAt(0) }}</span>
      </div>
      <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pointer-events-none" />

      <button
        class="absolute top-3 left-3 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <div class="absolute bottom-0 left-0 right-0 p-4">
        <h2 class="text-lg font-bold text-white">{{ app.name }}</h2>
        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 mt-1 text-xs text-white/60">
          <span
            class="px-1.5 py-0.5 rounded text-xs font-medium bg-white/15"
          >{{ categoryLabel }}</span>
          <span
            v-for="p in app.platforms"
            :key="p"
            class="text-xs"
          >{{ platformLabel(p) }}</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-5">
      <p class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/70' : 'text-gray-600'">
        {{ app.longDescription }}
      </p>

      <div v-if="app.howTo?.length">
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Getting Started</h4>
        <ol class="space-y-2">
          <li
            v-for="(step, i) in app.howTo"
            :key="i"
            class="flex gap-2.5 text-xs"
          >
            <span
              class="w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold shrink-0 mt-0.5"
              :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'"
            >{{ i + 1 }}</span>
            <span :class="isDark ? 'text-white/70' : 'text-gray-600'">{{ step }}</span>
          </li>
        </ol>
      </div>

      <div>
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Open</h4>
        <a
          :href="app.url"
          target="_blank"
          rel="noopener"
          class="flex items-center justify-between p-3 rounded-xl transition-colors"
          :class="isDark
            ? 'bg-white/5 hover:bg-white/10'
            : 'bg-black/3 hover:bg-black/5'"
        >
          <div class="flex items-center gap-2.5">
            <div
              class="w-8 h-8 rounded-lg flex items-center justify-center text-sm font-bold"
              :style="{ background: appGradient }"
            >
              <span class="text-white/90">{{ app.name.charAt(0) }}</span>
            </div>
            <div>
              <p class="text-xs font-medium"
                 :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ app.url.replace(/^https?:\/\//, '') }}</p>
              <p class="text-xs"
                 :class="isDark ? 'text-white/30' : 'text-gray-400'">Official website</p>
            </div>
          </div>
          <svg class="w-4 h-4" :class="isDark ? 'text-white/30' : 'text-gray-400'"
               fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
        </a>
      </div>

      <div v-if="relatedApps.length > 0">
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Related Apps</h4>
        <div class="space-y-2">
          <button
            v-for="related in relatedApps"
            :key="related.id"
            class="w-full text-left flex items-center gap-2.5 p-2.5 rounded-xl transition-colors"
            :class="isDark
              ? 'bg-white/5 hover:bg-white/10'
              : 'bg-black/3 hover:bg-black/5'"
            @click="$emit('selectApp', related)"
          >
            <div
              class="w-8 h-8 rounded-lg flex items-center justify-center text-sm font-bold shrink-0"
              :style="{ background: relatedGradient(related.id) }"
            >
              <span class="text-white/90">{{ related.name.charAt(0) }}</span>
            </div>
            <div class="min-w-0">
              <p class="text-xs font-medium truncate"
                 :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ related.name }}</p>
              <p class="text-xs truncate"
                 :class="isDark ? 'text-white/30' : 'text-gray-400'">{{ related.description }}</p>
            </div>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { APP_DATABASE, type AppEntry } from '@/data/apps'
import { useTheme } from '@/composables/useTheme'

const props = defineProps<{ app: AppEntry }>()
defineEmits<{ back: []; selectApp: [app: AppEntry] }>()

const { isDark } = useTheme()

const categoryLabels: Record<string, string> = {
  'nostr-client': 'Nostr Client',
  'lightning-wallet': 'Lightning Wallet',
  'bitcoin-wallet': 'Bitcoin Wallet',
  privacy: 'Privacy',
  node: 'Node Software',
  'dev-tool': 'Dev Tool',
  relay: 'Relay',
}

const categoryLabel = computed(() => categoryLabels[props.app.category] ?? props.app.category)

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

function hashToHue(id: string): number {
  let hash = 0
  for (let i = 0; i < id.length; i++) hash = id.charCodeAt(i) + ((hash << 5) - hash)
  return Math.abs(hash % 360)
}

const appGradient = computed(() => {
  const hue = hashToHue(props.app.id)
  return `linear-gradient(135deg, hsl(${hue}, 60%, 35%), hsl(${(hue + 40) % 360}, 50%, 25%))`
})

function relatedGradient(id: string): string {
  const hue = hashToHue(id)
  return `linear-gradient(135deg, hsl(${hue}, 60%, 35%), hsl(${(hue + 40) % 360}, 50%, 25%))`
}

const relatedApps = computed(() => {
  if (!props.app.relatedApps?.length) return []
  return props.app.relatedApps
    .map(id => APP_DATABASE.find(a => a.id === id))
    .filter((a): a is AppEntry => !!a)
})
</script>
