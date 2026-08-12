<template>
  <div class="h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <!-- Header -->
    <div class="shrink-0 px-4 py-3 flex items-center gap-3"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="min-w-[44px] min-h-[44px] rounded-lg path-glass-icon flex items-center justify-center transition-colors shrink-0"
        :class="isDark ? 'hover:bg-white/10' : 'hover:bg-black/5'"
        @click="$emit('back')"
      >
        <svg class="w-3.5 h-3.5" :class="isDark ? 'text-white/70' : 'text-gray-500'"
             fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <div class="min-w-0 flex-1">
        <h2 class="text-sm font-semibold truncate"
            :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ item.name }}
        </h2>
        <p class="text-xs"
           :class="isDark ? 'text-white/40' : 'text-gray-400'">
          {{ categoryLabel }}
        </p>
      </div>
      <button
        class="text-xs px-2 py-1 rounded-md transition-colors"
        :class="copied
          ? 'bg-emerald-500/20 text-emerald-400'
          : isDark
            ? 'bg-white/5 text-white/50 hover:bg-white/10'
            : 'bg-black/5 text-gray-500 hover:bg-black/10'"
        @click="copyCode"
      >
        {{ copied ? 'Copied' : 'Copy' }}
      </button>
    </div>

    <div class="p-4 space-y-4">
      <!-- Description -->
      <p class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/60' : 'text-gray-600'">
        {{ item.description }}
      </p>

      <!-- Live preview -->
      <div>
        <h4 class="text-xs uppercase tracking-[0.2em] font-semibold mb-2"
            :class="isDark ? 'text-white/30' : 'text-gray-400'">
          Preview
        </h4>
        <div class="rounded-xl p-4 overflow-hidden"
             :class="isDark ? 'bg-white/[0.03] border border-white/10' : 'bg-black/[0.02] border border-black/10'">
          <!-- Color preview -->
          <div v-if="item.category === 'colors'" class="space-y-2">
            <div class="h-12 rounded-lg border"
                 :class="isDark ? 'border-white/10' : 'border-black/10'"
                 :style="{ background: extractColorValue(item.code) }" />
            <p class="text-xs font-mono text-center"
               :class="isDark ? 'text-white/40' : 'text-gray-400'">
              {{ extractColorValue(item.code) }}
            </p>
          </div>

          <!-- Typography preview -->
          <div v-else-if="item.category === 'typography'" class="space-y-2">
            <p class="text-2xl font-bold"
               :class="isDark ? 'text-white/90' : 'text-gray-900'"
               :style="fontStyle">
              Aa Bb Cc 123
            </p>
            <p class="text-sm"
               :class="isDark ? 'text-white/60' : 'text-gray-600'"
               :style="fontStyle">
              The quick brown fox jumps over the lazy dog.
            </p>
          </div>

          <!-- Spacing preview -->
          <div v-else-if="item.category === 'spacing'" class="flex items-end gap-2">
            <div v-for="(size, i) in [4, 8, 12, 16, 20, 24, 32]" :key="i"
                 class="bg-accent/30 rounded-sm flex items-center justify-center"
                 :style="{ width: `${size}px`, height: `${size}px` }">
              <span v-if="size >= 16" class="text-[7px] text-accent font-mono">{{ size }}</span>
            </div>
          </div>

          <!-- Component preview (rendered as styled blocks) -->
          <div v-else class="space-y-2">
            <!-- Glass button preview -->
            <div v-if="item.id === 'atom-glass-btn'" class="flex gap-3">
              <button class="glass-button text-sm">Action</button>
              <button class="glass-button text-sm opacity-50 cursor-not-allowed">Disabled</button>
            </div>
            <div v-else-if="item.id === 'atom-glass-btn-sm'" class="flex gap-3">
              <button class="glass-button glass-button-sm text-xs">Small</button>
              <button class="glass-button glass-button-sm text-xs opacity-50 cursor-not-allowed">Disabled</button>
            </div>
            <div v-else-if="item.id === 'atom-icon-btn'" class="flex gap-3">
              <button class="w-9 h-9 rounded-xl path-glass-icon flex items-center justify-center"
                      :class="isDark ? 'text-white/70' : 'text-gray-500'">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                </svg>
              </button>
              <button class="w-9 h-9 rounded-xl path-glass-icon flex items-center justify-center"
                      :class="isDark ? 'text-white/70' : 'text-gray-500'">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
              </button>
              <button class="w-9 h-9 rounded-xl path-glass-icon flex items-center justify-center"
                      :class="isDark ? 'text-white/70' : 'text-gray-500'">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <div v-else-if="item.id === 'atom-badge'" class="flex flex-wrap gap-1.5">
              <span class="text-xs px-2 py-1 rounded-md font-medium"
                    :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'">Science Fiction</span>
              <span class="text-xs px-2 py-1 rounded-md font-medium"
                    :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'">Drama</span>
              <span class="text-xs px-2 py-1 rounded-md font-medium"
                    :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'">Thriller</span>
            </div>
            <div v-else-if="item.id === 'mol-glass-card'">
              <div class="glass-card p-4">
                <h3 class="text-sm font-semibold mb-1" :class="isDark ? 'text-white/90' : 'text-gray-900'">Glass Card</h3>
                <p class="text-xs" :class="isDark ? 'text-white/60' : 'text-gray-500'">Content with frosted glass background and subtle border.</p>
              </div>
            </div>

            <!-- Nav tab preview -->
            <div v-else-if="item.id === 'atom-nav-tab'" class="flex gap-1.5">
              <button class="text-xs px-2.5 py-1 rounded-md font-medium bg-accent/20 text-accent">Films</button>
              <button class="text-xs px-2.5 py-1 rounded-md font-medium"
                      :class="isDark ? 'bg-white/5 text-white/50' : 'bg-black/5 text-gray-500'">Songs</button>
              <button class="text-xs px-2.5 py-1 rounded-md font-medium"
                      :class="isDark ? 'bg-white/5 text-white/50' : 'bg-black/5 text-gray-500'">Podcasts</button>
            </div>

            <!-- Text input preview -->
            <div v-else-if="item.id === 'atom-input'">
              <input
                class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
                :class="isDark
                  ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
                  : 'bg-black/5 text-gray-800 placeholder:text-gray-400 focus:bg-black/10'"
                placeholder="Search..."
                readonly
              />
            </div>

            <!-- Scrollbar preview -->
            <div v-else-if="item.id === 'atom-scrollbar'" class="space-y-2">
              <div class="h-16 overflow-y-auto rounded-lg px-3 py-2"
                   :class="isDark ? 'bg-white/5' : 'bg-black/5'"
                   style="scrollbar-width: thin;">
                <p v-for="n in 8" :key="n" class="text-xs py-0.5"
                   :class="isDark ? 'text-white/40' : 'text-gray-400'">
                  Scrollable content line {{ n }}
                </p>
              </div>
              <p class="text-xs text-center" :class="isDark ? 'text-white/30' : 'text-gray-400'">
                4px wide, translucent thumb
              </p>
            </div>

            <!-- Gradient card preview -->
            <div v-else-if="item.id === 'mol-gradient-card'">
              <div class="gradient-card p-4 rounded-2xl">
                <h3 class="text-sm font-semibold mb-1 text-white">Featured</h3>
                <p class="text-xs text-white/70">Gradient background card for highlights.</p>
              </div>
            </div>

            <!-- Source link row preview -->
            <div v-else-if="item.id === 'mol-source-link'" class="space-y-1.5">
              <div class="flex items-center justify-between p-3 rounded-xl transition-colors"
                   :class="isDark ? 'bg-white/5' : 'bg-black/5'">
                <div class="flex items-center gap-2.5">
                  <span class="text-sm">🎬</span>
                  <div>
                    <p class="text-xs font-medium" :class="isDark ? 'text-white/80' : 'text-gray-800'">Netflix</p>
                    <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">Stream now</p>
                  </div>
                </div>
                <svg class="w-3.5 h-3.5" :class="isDark ? 'text-white/30' : 'text-gray-400'"
                     fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
              </div>
            </div>

            <!-- Banner hero preview -->
            <div v-else-if="item.id === 'mol-banner-hero'">
              <div class="relative w-full aspect-[16/7] rounded-lg overflow-hidden">
                <div class="absolute inset-0"
                     :style="{ background: 'linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%)' }" />
                <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent" />
                <div class="absolute bottom-0 left-0 p-3">
                  <h3 class="text-sm font-bold text-white/90">Banner Title</h3>
                  <p class="text-xs text-white/50">Subtitle text</p>
                </div>
              </div>
            </div>

            <!-- Cover card preview -->
            <div v-else-if="item.id === 'mol-cover-card'" class="flex gap-2">
              <div v-for="n in 3" :key="n"
                   class="flex-1 rounded-xl overflow-hidden">
                <div class="aspect-[2/3] relative"
                     :style="{ background: `linear-gradient(${120 * n}deg, ${['#2d1b69','#1b3a4b','#3b1b2b'][n-1]}, ${['#1a0a3e','#0a2030','#200a1a'][n-1]})` }">
                  <div class="absolute inset-0 bg-gradient-to-t from-black/60 to-transparent" />
                  <div class="absolute bottom-0 left-0 right-0 p-1.5">
                    <p class="text-xs text-white/80 font-medium truncate">{{ ['Film', 'Album', 'Series'][n-1] }}</p>
                  </div>
                </div>
              </div>
            </div>

            <!-- Chat bubble preview -->
            <div v-else-if="item.id === 'org-chat-bubble'" class="space-y-2">
              <div class="flex justify-end">
                <div class="max-w-[80%] px-3 py-2 rounded-2xl text-xs"
                     :class="isDark ? 'bg-white/10 text-white/90' : 'bg-black/10 text-gray-800'">
                  What films should I watch?
                </div>
              </div>
              <div class="flex justify-start">
                <div class="max-w-[80%] px-3 py-2 text-xs"
                     :class="isDark ? 'text-white/70' : 'text-gray-600'">
                  Here are some great picks from your library...
                </div>
              </div>
            </div>

            <!-- Content panel preview -->
            <div v-else-if="item.id === 'org-content-panel'" class="space-y-2">
              <div class="flex gap-1 pb-1.5"
                   :style="isDark ? 'border-bottom: 1px solid rgba(255,255,255,0.08)' : 'border-bottom: 1px solid rgba(0,0,0,0.06)'">
                <span class="text-xs px-2 py-0.5 rounded font-medium bg-accent/20 text-accent">Films</span>
                <span class="text-xs px-2 py-0.5 rounded font-medium"
                      :class="isDark ? 'text-white/40' : 'text-gray-400'">Songs</span>
                <span class="text-xs px-2 py-0.5 rounded font-medium"
                      :class="isDark ? 'text-white/40' : 'text-gray-400'">Books</span>
              </div>
              <div class="grid grid-cols-3 gap-1">
                <div v-for="n in 6" :key="n" class="aspect-[2/3] rounded-md"
                     :class="isDark ? 'bg-white/5' : 'bg-black/5'" />
              </div>
            </div>

            <!-- Detail view preview -->
            <div v-else-if="item.id === 'org-detail-view'" class="space-y-2">
              <div class="relative aspect-[16/7] rounded-lg overflow-hidden"
                   :class="isDark ? 'bg-white/5' : 'bg-black/5'">
                <div class="absolute inset-0 bg-gradient-to-t from-black/60 to-transparent" />
                <div class="absolute top-1.5 left-1.5 w-4 h-4 rounded-md flex items-center justify-center"
                     :class="isDark ? 'bg-white/10' : 'bg-black/10'">
                  <svg class="w-2.5 h-2.5" :class="isDark ? 'text-white/60' : 'text-gray-500'"
                       fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                  </svg>
                </div>
                <div class="absolute bottom-1 left-2">
                  <p class="text-xs font-bold text-white/90">Title</p>
                  <p class="text-xs text-white/50">Meta</p>
                </div>
              </div>
              <div class="space-y-1 px-1">
                <div class="h-1.5 rounded-full w-full" :class="isDark ? 'bg-white/5' : 'bg-black/5'" />
                <div class="h-1.5 rounded-full w-3/4" :class="isDark ? 'bg-white/5' : 'bg-black/5'" />
              </div>
            </div>

            <!-- Magazine grid preview -->
            <div v-else-if="item.id === 'org-magazine'">
              <div class="grid grid-cols-2 gap-px rounded-lg overflow-hidden"
                   :class="isDark ? 'bg-white/[0.12]' : 'bg-black/[0.08]'">
                <div class="col-span-2 px-3 py-3"
                     :class="isDark ? 'bg-[#0a0a0a]' : 'bg-white'">
                  <p class="text-[7px] uppercase tracking-[0.3em] mb-0.5"
                     :class="isDark ? 'text-white/30' : 'text-gray-400'">Editorial</p>
                  <p class="text-xs font-serif font-bold"
                     :class="isDark ? 'text-white/90' : 'text-gray-900'">Hero Headline</p>
                </div>
                <div class="px-2 py-2" :class="isDark ? 'bg-[#0a0a0a]' : 'bg-white'">
                  <p class="text-xs font-serif font-bold"
                     :class="isDark ? 'text-white/80' : 'text-gray-800'">Half Tile</p>
                </div>
                <div class="px-2 py-2" :class="isDark ? 'bg-[#0a0a0a]' : 'bg-white'">
                  <p class="text-xs font-serif font-bold"
                     :class="isDark ? 'text-white/80' : 'text-gray-800'">Half Tile</p>
                </div>
              </div>
            </div>

            <!-- Nostr note preview -->
            <div v-else-if="item.id === 'org-nostr-note'">
              <div class="p-3 rounded-xl"
                   :class="isDark ? 'bg-white/[0.03] border border-white/5' : 'bg-black/[0.02] border border-black/5'">
                <div class="flex items-start gap-2.5">
                  <div class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-bold shrink-0"
                       style="background: rgba(168, 85, 247, 0.2); color: rgba(168, 85, 247, 0.8);">
                    F
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-1.5">
                      <span class="text-xs font-semibold" :class="isDark ? 'text-white/80' : 'text-gray-800'">fiatjaf</span>
                      <span class="text-xs" :class="isDark ? 'text-white/25' : 'text-gray-300'">2h</span>
                    </div>
                    <p class="text-xs mt-0.5 leading-relaxed"
                       :class="isDark ? 'text-white/50' : 'text-gray-500'">
                      Nostr is the simplest open protocol...
                    </p>
                    <div class="flex gap-3 mt-1.5">
                      <span class="text-xs" :class="isDark ? 'text-white/25' : 'text-gray-300'">3 replies</span>
                      <span class="text-xs text-amber-500/70">21000 sats</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- Fade up animation preview -->
            <div v-else-if="item.id === 'anim-fade-up'" class="flex flex-col items-center gap-2">
              <div :key="fadeUpKey" class="animate-fade-up px-4 py-2 rounded-lg text-xs font-medium"
                   :class="isDark ? 'bg-white/10 text-white/70' : 'bg-black/10 text-gray-600'">
                Fade Up (900ms)
              </div>
              <button class="text-xs px-2 py-0.5 rounded transition-colors"
                      :class="isDark ? 'text-white/40 hover:text-white/60' : 'text-gray-400 hover:text-gray-600'"
                      @click="fadeUpKey++">
                Replay
              </button>
            </div>

            <!-- Scale in animation preview -->
            <div v-else-if="item.id === 'anim-scale-in'" class="flex flex-col items-center gap-2">
              <div :key="scaleInKey" class="animate-scale-in px-4 py-2 rounded-lg text-xs font-medium"
                   :class="isDark ? 'bg-white/10 text-white/70' : 'bg-black/10 text-gray-600'">
                Scale In (250ms)
              </div>
              <button class="text-xs px-2 py-0.5 rounded transition-colors"
                      :class="isDark ? 'text-white/40 hover:text-white/60' : 'text-gray-400 hover:text-gray-600'"
                      @click="scaleInKey++">
                Replay
              </button>
            </div>

            <!-- Generic preview fallback -->
            <div v-else class="text-center py-4">
              <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
                See code below for usage pattern
              </p>
            </div>
          </div>
        </div>
      </div>

      <!-- Used In -->
      <div v-if="item.usedIn">
        <h4 class="text-xs uppercase tracking-[0.2em] font-semibold mb-2"
            :class="isDark ? 'text-white/30' : 'text-gray-400'">
          Used In
        </h4>
        <div class="rounded-xl px-3 py-2.5"
             :class="isDark ? 'bg-white/[0.03] border border-white/10' : 'bg-black/[0.02] border border-black/10'">
          <p class="text-xs leading-relaxed"
             :class="isDark ? 'text-white/50' : 'text-gray-500'">
            {{ item.usedIn }}
          </p>
        </div>
      </div>

      <!-- Code block -->
      <div>
        <h4 class="text-xs uppercase tracking-[0.2em] font-semibold mb-2"
            :class="isDark ? 'text-white/30' : 'text-gray-400'">
          Code
        </h4>
        <pre class="rounded-xl p-4 text-xs leading-relaxed font-mono overflow-x-auto"
             :class="isDark
               ? 'bg-black/40 text-white/70 border border-white/10'
               : 'bg-gray-50 text-gray-700 border border-gray-200'">{{ item.code }}</pre>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import type { DesignSystemItem } from '@/composables/useContentPanel'

const props = defineProps<{ item: DesignSystemItem }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const copied = ref(false)
const fadeUpKey = ref(0)
const scaleInKey = ref(0)

const categoryLabels: Record<string, string> = {
  colors: 'Colors',
  typography: 'Typography',
  spacing: 'Spacing',
  atoms: 'Atoms',
  molecules: 'Molecules',
  organisms: 'Organisms',
}

const categoryLabel = computed(() => categoryLabels[props.item.category] ?? props.item.category)

const fontStyle = computed(() => {
  if (props.item.id === 'type-mono') return { fontFamily: 'Menlo, Monaco, "Courier New", monospace' }
  if (props.item.id === 'type-serif') return { fontFamily: 'Georgia, "Times New Roman", Times, serif' }
  return { fontFamily: 'Inter, system-ui, -apple-system, sans-serif' }
})

function extractColorValue(code: string): string {
  const match = /(?:background-color|color|background):\s*([^;]+)/i.exec(code)
  if (!match) return '#333'
  return match[1].trim()
}

async function copyCode() {
  try {
    await navigator.clipboard.writeText(props.item.code)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch { /* ignore */ }
}
</script>
