<template>
  <div class="podcast-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden">
      <div class="w-full aspect-[16/7] flex items-center justify-center overflow-hidden bg-black/20">
        <img
          v-if="coverSrc"
          :src="coverSrc"
          :alt="podcast.title"
          class="w-full h-full object-cover object-center block"
          @error="coverFailed = true"
        />
        <div
          v-else
          class="w-full h-full bg-cover bg-center"
          :style="{ backgroundImage: `url(${fallbackCover})` }"
        />
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
        <h2 class="text-lg font-bold text-white">{{ podcast.title }}</h2>
        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 mt-1 text-xs text-white/60">
          <span v-if="podcast.host">{{ podcast.host }}</span>
          <span v-if="podcast.year">{{ podcast.year }}</span>
          <span v-if="podcast.episodeCount">{{ podcast.episodeCount }} episodes</span>
        </div>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <p v-if="podcast.description" class="text-sm leading-relaxed"
         :class="isDark ? 'text-white/70' : 'text-gray-600'">
        {{ podcast.description }}
      </p>

      <div v-if="podcast.genres?.length" class="flex flex-wrap gap-1.5">
        <span
          v-for="genre in podcast.genres"
          :key="genre"
          class="text-xs px-2 py-1 rounded-md font-medium"
          :class="isDark ? 'bg-white/10 text-white/60' : 'bg-black/5 text-gray-600'"
        >
          {{ genre }}
        </span>
      </div>

      <div>
        <h4 class="text-xs font-semibold mb-2"
            :class="isDark ? 'text-white/50' : 'text-gray-500'">Listen on</h4>
        <div class="space-y-2">
          <a
            v-for="src in podcast.sources"
            :key="src.url"
            :href="src.url"
            target="_blank"
            rel="noopener"
            class="flex items-center justify-between p-3 rounded-xl transition-colors"
            :class="isDark
              ? 'bg-white/5 hover:bg-white/10'
              : 'bg-black/3 hover:bg-black/5'"
          >
            <div class="flex items-center gap-2.5">
              <span class="text-sm">{{ sourceIcon(src.type) }}</span>
              <p class="text-xs font-medium"
                 :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ src.name }}</p>
            </div>
            <svg class="w-4 h-4" :class="isDark ? 'text-white/30' : 'text-gray-400'"
                 fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </a>
          <template v-if="extListenLinks.length">
            <a
              v-for="link in extListenLinks"
              :key="link.url"
              :href="link.url"
              target="_blank"
              rel="noopener"
              class="flex items-center justify-between p-3 rounded-xl transition-colors"
              :class="isDark
                ? 'bg-white/5 hover:bg-white/10'
                : 'bg-black/3 hover:bg-black/5'"
            >
              <div class="flex items-center gap-2.5">
                <span class="text-sm">{{ link.icon }}</span>
                <div>
                  <p class="text-xs font-medium"
                     :class="isDark ? 'text-white/80' : 'text-gray-800'">{{ link.name }}</p>
                  <p v-if="link.desc" class="text-xs"
                     :class="isDark ? 'text-white/30' : 'text-gray-400'">{{ link.desc }}</p>
                </div>
              </div>
              <svg class="w-4 h-4" :class="isDark ? 'text-white/30' : 'text-gray-400'"
                   fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </a>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import type { Podcast } from '@aiui/core/types/content'
import { useTheme } from '@/composables/useTheme'
import { generatePodcastCoverFallback, fetchPodcastCover } from '@/composables/useImageFallback'

const props = defineProps<{ podcast: Podcast }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const coverFailed = ref(false)
const fetchedCover = ref<string | null>(null)

const coverSrc = computed(() => {
  if (coverFailed.value) return null
  return props.podcast.coverUrl || fetchedCover.value || null
})

onMounted(() => {
  if (props.podcast.coverUrl) return
  fetchPodcastCover(props.podcast.title, props.podcast.host).then((url) => {
    if (url) fetchedCover.value = url
  })
})

const fallbackCover = computed(() =>
  generatePodcastCoverFallback(props.podcast.title, props.podcast.host)
)

const q = computed(() =>
  `${props.podcast.title} ${props.podcast.host ?? ''}`.trim().replace(/\s+/g, '+'),
)

const extListenLinks = computed(() => {
  if (props.podcast.sources.length > 0) return []
  return [
    { name: 'Fountain', url: `https://fountain.fm/search?q=${q.value}`, icon: '⚡', desc: 'Podcasting 2.0, Lightning' },
    { name: 'Podcast Index', url: `https://podcastindex.org/search?q=${q.value}`, icon: '📻', desc: 'Open podcast directory' },
    { name: 'YouTube', url: `https://youtube.com/results?search_query=${q.value}`, icon: '▶️', desc: 'Video podcasts' },
    { name: 'Rumble', url: `https://rumble.com/search/video?q=${q.value}`, icon: '📺', desc: 'Video & podcasts' },
    { name: 'Odysee', url: `https://odysee.com/$/search?q=${q.value}`, icon: '🔗', desc: 'Decentralized' },
  ]
})

function sourceIcon(type: string): string {
  const icons: Record<string, string> = {
    fountain: '⚡',
    rumble: '📺',
    youtube: '▶️',
    podcastindex: '📻',
    castopod: '🦣',
    odysee: '🔗',
    podverse: '🎧',
    ipfs: '🌐',
    rss: '📡',
  }
  return icons[type] ?? '🎙️'
}
</script>
