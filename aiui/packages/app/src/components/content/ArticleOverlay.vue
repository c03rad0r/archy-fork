<template>
  <Teleport to="body">
    <Transition name="app-launcher">
      <div
        v-if="store.isOpen"
        class="fixed inset-0 z-[2400] flex items-center justify-center p-6 md:p-10"
        @click.self="store.close()"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-md" />

        <div
          class="article-overlay-panel relative z-10 flex flex-col overflow-hidden rounded-2xl shadow-2xl path-glass-card"
          :class="panelClasses"
        >
          <div class="flex items-center gap-3 px-4 py-3 shrink-0"
               :style="isDark
                 ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
                 : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
            <button
              v-if="!store.content"
              type="button"
              class="flex items-center justify-center w-9 h-9 rounded-lg transition-colors transition-transform duration-300 disabled:opacity-70 disabled:cursor-not-allowed"
              :class="isDark ? 'hover:bg-white/10 text-white/70' : 'hover:bg-black/5 text-gray-600'"
              aria-label="Refresh page"
              title="Refresh"
              :disabled="isRefreshing"
              @click="refreshIframe"
            >
              <svg
                class="w-5 h-5"
                :class="{ 'animate-spin': isRefreshing }"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
            <span class="flex-1 truncate text-sm font-medium min-w-0"
                  :class="isDark ? 'text-white/90' : 'text-gray-900'">
              {{ store.title || 'Article' }}
            </span>
            <a
              v-if="store.url"
              :href="store.url"
              target="_blank"
              rel="noopener noreferrer"
              class="flex items-center justify-center w-9 h-9 rounded-lg transition-colors shrink-0"
              :class="isDark ? 'hover:bg-white/10 text-white/70' : 'hover:bg-black/5 text-gray-600'"
              aria-label="Open in new tab"
              title="Open in new tab"
              @click.stop
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </a>
            <button
              type="button"
              class="flex items-center justify-center w-9 h-9 rounded-lg transition-colors shrink-0"
              :class="isDark ? 'hover:bg-white/10 text-white/70' : 'hover:bg-black/5 text-gray-600'"
              aria-label="Close"
              @click="store.close()"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="relative flex-1 min-h-0 bg-black/20 overflow-hidden">
            <!-- When we have RSS/article content, render it; otherwise load URL in iframe -->
            <div
              v-if="store.content"
              class="absolute inset-0 overflow-y-auto p-4 md:p-6 text-sm leading-relaxed"
              :class="isDark ? 'text-white/90' : 'text-gray-900'"
            >
              <article
                class="[&_p]:mb-3 [&_ul]:list-disc [&_ol]:list-decimal [&_li]:ml-4 [&_a]:underline [&_a]:underline-offset-2 [&_h1]:text-lg [&_h2]:text-base [&_h3]:text-sm [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_blockquote]:italic"
              >
                <img
                  v-if="store.imgSrc && isSafeImgSrc(store.imgSrc)"
                  :src="store.imgSrc"
                  :alt="store.title"
                  class="w-full rounded-lg object-cover max-h-48 mb-4"
                />
                <div v-html="sanitizedContent" />
              </article>
              <a
                :href="store.url ?? undefined"
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex items-center gap-1.5 mt-4 text-sm"
                :class="isDark ? 'text-white/70 hover:text-white' : 'text-gray-500 hover:text-gray-800'"
              >
                Read full article
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
              </a>
            </div>
            <iframe
              v-else-if="store.url"
              ref="iframeRef"
              :key="iframeRefreshKey"
              :src="store.url"
              class="absolute inset-0 w-full h-full border-0"
              style="-ms-overflow-style: none; scrollbar-width: none;"
              title="Article content"
              @load="onIframeLoad"
            />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useArticleOverlayStore } from '@/stores/articleOverlay'
import { useTheme } from '@/composables/useTheme'
import { isSafeImgSrc, sanitizeHtml, escapeHtml } from '@/utils/html'

const store = useArticleOverlayStore()
const { isDark } = useTheme()

const sanitizedContent = computed(() => {
  const c = store.content
  if (!c) return ''
  if (/<[a-z][\s\S]*>/i.test(c)) return sanitizeHtml(c)
  return `<p class="whitespace-pre-wrap">${escapeHtml(c)}</p>`
})

const iframeRef = ref<HTMLIFrameElement | null>(null)
const iframeRefreshKey = ref(0)
const isRefreshing = ref(false)

function refreshIframe() {
  isRefreshing.value = true
  iframeRefreshKey.value++
}

function onIframeLoad() {
  isRefreshing.value = false
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape' && store.isOpen) {
    store.close()
    e.preventDefault()
    e.stopPropagation()
  }
}

watch(
  () => store.isOpen,
  (open) => {
    if (!open) isRefreshing.value = false
  }
)

onMounted(() => {
  window.addEventListener('keydown', onKeyDown, true)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown, true)
})

const panelClasses = [
  'w-full max-w-[calc(100vw-3rem)] h-[80vh] max-h-[calc(100vh-5rem)]',
  'md:max-w-[calc(100vw-5rem)]',
]
</script>

<style scoped>
iframe::-webkit-scrollbar {
  display: none;
}
.app-launcher-enter-active,
.app-launcher-leave-active {
  transition: opacity 0.25s ease;
}
.app-launcher-enter-active .article-overlay-panel,
.app-launcher-leave-active .article-overlay-panel {
  transition: transform 0.25s ease, opacity 0.25s ease;
}
.app-launcher-enter-from,
.app-launcher-leave-to {
  opacity: 0;
}
.app-launcher-enter-from .article-overlay-panel,
.app-launcher-leave-to .article-overlay-panel {
  transform: scale(0.96);
  opacity: 0;
}
</style>
