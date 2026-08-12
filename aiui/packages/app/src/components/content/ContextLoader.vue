<template>
  <div class="relative flex-1 flex flex-col min-h-0 overflow-hidden">
    <div
      v-if="['film','song','podcast','book','tvshow','image','news','websites','magazine'].includes(contextType)"
      class="flex-1 flex flex-col min-h-0"
    >
      <div
        class="p-4 shrink-0 flex items-center justify-between gap-2"
        :style="isDark
          ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
          : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
      >
        <p class="text-sm font-medium"
           :class="isDark ? 'text-white/70' : 'text-gray-600'">
          {{ contextLabel }}
        </p>
        <!-- gap-3 + mr-1 on the label: "Surfacing…" tracks at 0.2em, so its
             last glyph's letter-spacing sits flush against the close button
             and read as overlapping it. -->
        <div class="flex items-center gap-3 shrink-0">
          <p class="text-xs font-mono uppercase tracking-[0.2em] mr-1"
             :class="isDark ? 'text-white/25' : 'text-gray-400'">
            Surfacing…
          </p>
          <slot name="header-actions" />
        </div>
      </div>
      <LoadingContentGrid :variant="skeletonVariant" :count="skeletonCount" />
    </div>

    <div
      v-else
      class="relative flex-1 flex items-center justify-center min-h-0"
    >
      <div
        class="absolute inset-0 opacity-[0.04] pointer-events-none"
        :class="isDark ? 'bg-white' : 'bg-black'"
        style="background-image: url(&quot;data:image/svg+xml,%3Csvg viewBox='0 0 256 256' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.8' numOctaves='4'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E&quot;)"
      />
      <div class="relative flex flex-col items-center gap-8">
        <div class="flex items-center gap-2">
          <div
            v-for="(_, i) in 5"
            :key="i"
            class="w-12 h-16 rounded-lg overflow-hidden relative animate-cell-pulse"
            :class="isDark
              ? 'bg-white/8 border border-white/15 shadow-xl shadow-accent/5'
              : 'bg-black/6 border border-black/8 shadow-xl shadow-accent/10'"
            :style="{ animationDelay: `${i * 100}ms` }"
          >
            <div class="absolute inset-0 pointer-events-none overflow-hidden">
              <div
                class="absolute inset-0 w-1/2 animate-shimmer-sweep"
                :class="isDark
                  ? 'bg-gradient-to-r from-transparent via-accent/25 to-transparent'
                  : 'bg-gradient-to-r from-transparent via-accent/35 to-transparent'"
              />
            </div>
          </div>
        </div>
        <p class="text-sm font-medium"
           :class="isDark ? 'text-white/70' : 'text-gray-600'">
          {{ contextLabel }}
        </p>
        <div class="w-40 h-px rounded-full overflow-hidden"
             :class="isDark ? 'bg-white/8' : 'bg-black/8'">
          <div class="h-full bg-accent/90 rounded-full animate-progress-sweep" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import LoadingContentGrid from './LoadingContentGrid.vue'

const props = withDefaults(
  defineProps<{
    contextType?: 'film' | 'song' | 'podcast' | 'book' | 'tvshow' | 'image' | 'news' | 'websites' | 'magazine' | 'generic'
  }>(),
  { contextType: 'film' }
)

const { isDark } = useTheme()

const contextLabel = computed(() => {
  if (props.contextType === 'film') return 'Film recommendations'
  if (props.contextType === 'song') return 'Song recommendations'
  if (props.contextType === 'podcast') return 'Podcast recommendations'
  if (props.contextType === 'book') return 'Book recommendations'
  if (props.contextType === 'tvshow') return 'TV Series recommendations'
  if (props.contextType === 'image') return 'Images'
  if (props.contextType === 'news') return 'Articles'
  if (props.contextType === 'websites') return 'Websites'
  if (props.contextType === 'magazine') return 'Brief'
  return 'Content'
})

const skeletonVariant = computed<'poster' | 'square' | 'list' | 'magazine'>(() => {
  if (['song', 'podcast', 'image'].includes(props.contextType)) return 'square'
  if (['news', 'websites'].includes(props.contextType)) return 'list'
  if (props.contextType === 'magazine') return 'magazine'
  return 'poster'
})

const skeletonCount = computed(() => {
  if (props.contextType === 'magazine') return 6
  if (['news', 'websites'].includes(props.contextType)) return 6
  return 12
})
</script>

<style scoped>
.animate-cell-pulse {
  animation: cell-pulse 1.8s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}

.animate-shimmer-sweep {
  animation: shimmer-sweep 2.2s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}

.animate-progress-sweep {
  animation: progress-sweep 1.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}

@keyframes cell-pulse {
  0%, 100% { opacity: 0.6; transform: scale(0.96); }
  50% { opacity: 1; transform: scale(1.02); }
}

@keyframes shimmer-sweep {
  0% { transform: translateX(-100%); }
  60% { transform: translateX(200%); }
  100% { transform: translateX(200%); }
}

@keyframes progress-sweep {
  0% { width: 0; margin-left: 0; }
  45% { width: 60%; margin-left: 20%; }
  90% { width: 0; margin-left: 100%; }
  100% { width: 0; margin-left: 0; }
}
</style>
