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
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            {{ filteredArticles.length }} {{ variant === 'websites' ? 'websites' : 'articles' }}
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <input
        v-model="search"
        type="text"
        :placeholder="variant === 'websites' ? 'Search websites...' : 'Search articles...'"
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
          : 'bg-black/3 text-gray-800 placeholder:text-gray-400 focus:bg-black/5'"
      />
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="grid grid-cols-2 sm:grid-cols-3 gap-4">
        <button
          v-for="(article, i) in filteredArticles"
          :key="i"
          class="group flex flex-col items-stretch text-left w-full path-glass-bubble rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :aria-label="article.title"
          @click="openArticle(article)"
        >
          <div class="cover-card flex-1 min-h-0 relative">
            <div class="aspect-[4/3] flex flex-col w-full overflow-hidden rounded-[10px]">
              <!-- Top: image or icon area (edge-to-edge with title bar below) -->
              <div class="flex-1 min-h-0 relative">
                <img
                  v-if="isSafeImgUrl(article.imgSrc) && !failedImgs.has(article.url)"
                  :src="article.imgSrc"
                  :alt="article.title"
                  class="absolute inset-0 w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
                  loading="lazy"
                  @error="onImgError(article.url)"
                />
                <div
                  v-else
                  class="absolute inset-0 bg-cover bg-center"
                  :style="{ backgroundImage: `url(${newsFallback(article)})` }"
                />
                <div v-if="isSafeImgUrl(article.imgSrc) && !failedImgs.has(article.url)" class="absolute inset-0 bg-gradient-to-t from-black/60 via-black/20 to-transparent pointer-events-none" />
              </div>

              <div
                class="shrink-0 p-2 backdrop-blur-md rounded-b-[10px]"
                :class="[
                  isDark ? 'bg-black shadow-[inset_0_1px_0_rgba(255,255,255,0.12)]' : 'bg-white shadow-[inset_0_1px_0_rgba(0,0,0,0.06)]'
                ]"
              >
                <p class="text-xs font-semibold leading-tight line-clamp-2"
                   :class="isDark ? 'text-white/95' : 'text-gray-900'">
                  {{ article.title }}
                </p>
                <p v-if="article.content"
                   class="text-xs line-clamp-1 mt-0.5"
                   :class="isDark ? 'text-white/70' : 'text-gray-600'">
                  {{ article.content }}
                </p>
                <p class="text-xs truncate mt-0.5"
                   :class="isDark ? 'text-white/50' : 'text-gray-500'">
                  {{ formatDomain(article.url) }}
                </p>
              </div>
            </div>
          </div>
        </button>
      </div>

      <div v-if="filteredArticles.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ variant === 'websites' ? 'No websites match your search' : 'No articles match your search' }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { WebSearchResult } from '@aiui/core/types/message'
import { useTheme } from '@/composables/useTheme'
import { useContentPanel } from '@/composables/useContentPanel'
import { generateNewsFallback, generateWebsiteFallback } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  articles: WebSearchResult[]
  title?: string
  /** User query for contextual sorting (most relevant first) */
  query?: string
  /** 'news' = icon in upper area; 'websites' = icon in glassmorphic circle centered */
  variant?: 'news' | 'websites'
}>(), {
  title: 'News & Articles',
  query: '',
  variant: 'news',
})

const { isDark } = useTheme()
const { openArticleDetail, openWebsiteDetail } = useContentPanel()
const search = ref('')
const failedImgs = ref<Set<string>>(new Set())

function isSafeImgUrl(u: string | undefined): u is string {
  if (!u || typeof u !== 'string') return false
  return /^https?:\/\//i.test(u.trim())
}

function onImgError(url: string) {
  failedImgs.value = new Set([...failedImgs.value, url])
}

function formatDomain(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return url
  }
}

function newsFallback(article: WebSearchResult): string {
  if (props.variant === 'websites') {
    return generateWebsiteFallback(article.title, formatDomain(article.url))
  }
  return generateNewsFallback(article.title, formatDomain(article.url))
}

function openArticle(article: WebSearchResult) {
  if (props.variant === 'websites') {
    openWebsiteDetail(article)
  } else {
    openArticleDetail(article)
  }
}

function relevanceScore(article: WebSearchResult, query: string): number {
  if (!query.trim()) return 0
  const q = query.toLowerCase()
  const terms = q.split(/\s+/).filter((t) => t.length > 1)
  if (terms.length === 0) return 0
  const title = article.title.toLowerCase()
  const content = (article.content ?? '').toLowerCase()
  const url = article.url.toLowerCase()
  let score = 0
  for (const term of terms) {
    if (title.includes(term)) score += 3
    if (content.includes(term)) score += 2
    if (url.includes(term)) score += 1
  }
  return score
}

const filteredArticles = computed(() => {
  let list = props.articles
  if (search.value.trim()) {
    const q = search.value.toLowerCase()
    list = list.filter(
      (a) =>
        a.title.toLowerCase().includes(q) ||
        (a.content ?? '').toLowerCase().includes(q) ||
        a.url.toLowerCase().includes(q),
    )
  }
  if (props.query.trim()) {
    return [...list].sort((a, b) => relevanceScore(b, props.query) - relevanceScore(a, props.query))
  }
  return list
})
</script>
