<template>
  <div class="h-full flex flex-col">
    <!-- Article detail view -->
    <template v-if="selectedArticle">
      <div class="flex items-center gap-2 px-4 py-3 border-b border-white/[0.08]">
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          @click="selectedArticle = null"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <span class="text-xs text-white/40 truncate">{{ articleTitle }}</span>
      </div>
      <div class="flex-1 overflow-y-auto custom-scrollbar">
        <ArticleReader
          :content="selectedArticle.content"
          :title="articleTitle"
        />
      </div>
    </template>

    <!-- Article list -->
    <template v-else>
      <div class="p-4 border-b border-white/[0.08]">
        <h3 class="text-sm font-bold text-white/90 mb-2">Long-Form Articles</h3>
        <p class="text-xs text-white/30">NIP-23 kind:30023 articles from your network</p>
      </div>

      <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
        <div v-if="isLoading" class="flex items-center justify-center py-12">
          <p class="text-xs text-white/30">Loading articles...</p>
        </div>

        <button
          v-for="article in articles"
          :key="article.id"
          class="w-full text-left p-3 rounded-xl transition-all duration-150 bg-white/[0.03] hover:bg-white/[0.07] border border-white/5"
          @click="selectedArticle = article"
        >
          <div class="space-y-1">
            <h4 class="text-xs font-semibold text-white/80 line-clamp-2">
              {{ getArticleTitle(article) }}
            </h4>
            <p class="text-xs text-white/40 line-clamp-2">
              {{ getArticleSummary(article) }}
            </p>
            <div class="flex items-center gap-2">
              <span class="text-xs text-white/25 font-mono">{{ truncate(article.pubkey) }}</span>
              <span class="text-xs text-white/20">{{ formatDate(article.created_at) }}</span>
              <span v-if="getArticleImage(article)" class="text-xs text-accent/40 ml-auto">has image</span>
            </div>
          </div>
        </button>

        <div v-if="!isLoading && articles.length === 0" class="flex items-center justify-center py-12">
          <p class="text-xs text-white/30">No articles found</p>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import ArticleReader from '@/components/renderers/ArticleReader.vue'
import type { NostrEvent, NostrNote } from '@/composables/useNostr'

const articles = ref<NostrNote[]>([])
const isLoading = ref(true)
const selectedArticle = ref<NostrNote | null>(null)

const articleTitle = computed(() => {
  if (!selectedArticle.value) return ''
  return getArticleTitle(selectedArticle.value)
})

function truncate(hex: string): string {
  if (hex.length <= 16) return hex
  return hex.slice(0, 8) + '...' + hex.slice(-4)
}

function formatDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString('en', { month: 'short', day: 'numeric', year: 'numeric' })
}

function getArticleTitle(note: NostrNote): string {
  const titleTag = note.tags.find(t => t[0] === 'title')
  if (titleTag?.[1]) return titleTag[1]
  // Fallback: first line or first 60 chars
  const firstLine = note.content.split('\n')[0]
  return firstLine.replace(/^#+ /, '').slice(0, 60) || 'Untitled'
}

function getArticleSummary(note: NostrNote): string {
  const summaryTag = note.tags.find(t => t[0] === 'summary')
  if (summaryTag?.[1]) return summaryTag[1]
  return note.content.slice(0, 120).replace(/[#*_]/g, '')
}

function getArticleImage(note: NostrNote): string | null {
  const imageTag = note.tags.find(t => t[0] === 'image')
  return imageTag?.[1] ?? null
}

async function loadArticles() {
  isLoading.value = true
  articles.value = []

  const relayUrl = 'wss://relay.nostr.band'
  const subId = 'articles-' + Math.random().toString(36).slice(2, 8)

  try {
    const ws = new WebSocket(relayUrl)
    const results: NostrNote[] = []

    const timer = setTimeout(() => {
      ws.close()
      articles.value = results
      isLoading.value = false
    }, 10000)

    ws.onopen = () => {
      ws.send(JSON.stringify([
        'REQ', subId,
        { kinds: [30023], limit: 30 },
      ]))
    }

    ws.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data)
        if (Array.isArray(data) && data[0] === 'EVENT' && data[1] === subId && data[2]) {
          const evt = data[2] as NostrEvent
          if (!results.find(r => r.id === evt.id)) {
            results.push({
              id: evt.id,
              pubkey: evt.pubkey,
              authorName: truncate(evt.pubkey),
              kind: evt.kind,
              content: evt.content,
              created_at: evt.created_at,
              tags: evt.tags ?? [],
            })
          }
        }
        if (Array.isArray(data) && data[0] === 'EOSE') {
          clearTimeout(timer)
          ws.close()
          results.sort((a, b) => b.created_at - a.created_at)
          articles.value = results
          isLoading.value = false
        }
      } catch { /* skip */ }
    }

    ws.onerror = () => {
      clearTimeout(timer)
      isLoading.value = false
    }
  } catch {
    isLoading.value = false
  }
}

onMounted(() => {
  loadArticles()
})
</script>
