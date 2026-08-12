<template>
  <div class="article-detail h-full overflow-y-auto overflow-x-hidden scrollbar-hide">
    <div class="relative w-full overflow-hidden aspect-[16/7] shrink-0">
      <img
        v-if="article.imgSrc && isSafeImgSrc(article.imgSrc)"
        :src="article.imgSrc"
        :alt="article.title"
        class="absolute inset-0 w-full h-full object-cover object-center block"
      />
      <div
        v-else
        class="absolute inset-0"
        :style="{ background: fallbackGradient }"
      />
      <div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-transparent pointer-events-none" />

      <button
        class="absolute top-3 left-3 p-2 rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10 text-white/80"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <div class="absolute bottom-0 left-0 right-0 p-4">
        <h2 class="text-lg font-bold text-white">{{ article.title }}</h2>
        <p v-if="articleDomain" class="text-xs text-white/60 mt-1">{{ articleDomain }}</p>
      </div>
    </div>

    <div class="p-4 space-y-4">
      <article
        v-if="article.content"
        class="text-white/90 [&_p]:mb-3 [&_ul]:list-disc [&_ol]:list-decimal [&_li]:ml-4 [&_a]:underline [&_a]:underline-offset-2 [&_h1]:text-lg [&_h2]:text-base [&_h3]:text-sm [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_blockquote]:italic"
      >
        <div v-html="sanitizedContent" />
      </article>

      <div v-else class="py-4">
        <p class="text-sm text-white/50">
          Full article content is not available. Open the link below to read on the source site.
        </p>
      </div>

      <a
        v-if="article.url"
        :href="article.url"
        target="_blank"
        rel="noopener noreferrer"
        class="inline-flex items-center gap-2 p-3 rounded-xl transition-colors bg-white/10 hover:bg-white/15 text-white/90"
      >
        <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
        </svg>
        Read full article
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { WebSearchResult } from '@aiui/core/types/message'
import { isSafeImgSrc, sanitizeHtml, escapeHtml } from '@/utils/html'

const props = defineProps<{ article: WebSearchResult }>()
defineEmits<{ back: [] }>()

const articleDomain = computed(() => {
  const url = props.article?.url
  if (!url || typeof url !== 'string') return ''
  try {
    const u = new URL(url)
    if (!/^https?:$/i.test(u.protocol)) return ''
    return u.hostname.replace(/^www\./, '')
  } catch {
    return ''
  }
})

const fallbackGradient = computed(() => {
  const hue = [...props.article.title].reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
  return `linear-gradient(135deg, hsl(${hue}, 25%, 12%) 0%, hsl(${(hue + 40) % 360}, 20%, 8%) 100%)`
})

const sanitizedContent = computed(() => {
  const c = props.article.content
  if (!c) return ''
  if (/<[a-z][\s\S]*>/i.test(c)) return sanitizeHtml(c)
  return `<p class="whitespace-pre-wrap">${escapeHtml(c)}</p>`
})
</script>
