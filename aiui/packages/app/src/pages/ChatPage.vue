<template>
  <div
    class="h-full flex flex-col relative overflow-hidden transition-colors duration-300"
    :class="[]"
    :style="isDark
      ? { background: '#000 url(' + bgImageUrl + ') center center / cover no-repeat fixed' }
      : isEmbedded
        ? { background: 'transparent' }
        : { backgroundColor: '#f5f4f1' }"
  >
    <div v-if="isDark" class="absolute inset-0 pointer-events-none bg-black/20" />

    <!-- Desktop layout -->
    <div
      class="flex-1 flex h-full p-3 md:p-4 gap-3 md:gap-4 transition-[padding] duration-200"
      :class="[isMobile ? 'hidden' : '']"
      :style="playerActive && !isMobile ? { paddingBottom: '76px' } : {}"
    >

      <!-- Content surface (main area) -->
      <main
        class="flex-1 min-w-0 path-glass-card overflow-hidden flex relative panel-slide-in"
        :class="[
          isWideDesktop
            ? (panelSide === 'left' ? 'order-2' : 'order-1')
            : (panelSide === 'left' ? 'order-last' : 'order-first'),
          !isWideDesktop && hasDetailOpen && !hasGridContent && 'detail-active'
        ]"
        style="animation-delay: 0.1s"
      >
        <!-- Grid/list panel (on wide desktop: always show grid; on regular: show when no detail) -->
        <div
          v-if="panelOpen && hasGridContent && (isWideDesktop || !hasDetailOpen)"
          class="flex-1 min-w-0 flex flex-col relative"
        >
          <CloseButton @click="closePanel" />
          <div
            class="shrink-0 flex items-center gap-2 px-4 pr-12 py-3"
            :style="isDark
              ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
              : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
          >
            <div class="flex flex-wrap gap-1.5 flex-1 min-w-0 justify-center">
              <button
                v-for="tab in availableTabs"
                :key="tab"
                class="text-xs px-2 py-1 rounded-md transition-all duration-150"
                :class="activeTab === tab
                  ? 'nav-tab-active'
                  : isDark
                    ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
                    : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
                @click="setActiveTab(tab)"
              >
                {{ tabLabel(tab) }}
              </button>
            </div>
          </div>
          <ErrorBoundary title="Content failed to load">
            <ContentGridView
              :active-tab="activeTab"
              :is-wide-desktop="isWideDesktop"
              :is-mobile="isMobile"
              :panel-films="panelFilms"
              :panel-books="panelBooks"
              :panelTVSeries="panelTVSeries"
              :panel-images="panelImages"
              :panel-places="panelPlaces"
              :panel-songs="panelSongs"
              :panel-podcasts="panelPodcasts"
              :panel-web-results="panelWebResults"
              :panel-websites="panelWebsites"
              :panel-magazine-sections="panelMagazineSections"
              :panel-magazine-hero-image="panelMagazineHeroImage"
              :panel-recipes="panelRecipes"
              :panel-apps="panelApps"
              :panel-title="panelTitle"
              :panel-query="panelQuery"
              :panel-response-text="panelResponseText"
              @close="closePanel"
            />
          </ErrorBoundary>
        </div>

        <!-- Detail replaces grid on regular desktops -->
        <div v-else-if="!isWideDesktop && panelOpen && hasDetailOpen" class="flex-1 min-w-0 flex flex-col">
          <CloseButton @click="hasGridContent ? closeAllDetails() : closePanel()" />
          <ErrorBoundary title="Detail view error">
            <DetailView />
          </ErrorBoundary>
        </div>

        <!-- Loading state -->
        <ContextLoader
          v-else-if="chatStore.isStreaming"
          :context-type="loaderContextType"
        >
          <template #header-actions>
            <CloseButton @click="closePanel" />
          </template>
        </ContextLoader>

        <!-- Empty state -->
        <div v-else class="flex-1 flex items-center justify-center">
          <div class="text-center space-y-4 animate-fade-up max-w-sm px-6">
            <div class="empty-state-icon w-20 h-20 rounded-2xl path-glass-icon flex items-center justify-center mx-auto overflow-hidden">
              <span class="text-3xl" :class="isDark ? 'text-[#fafafa]' : 'text-gray-800'">✦</span>
            </div>
            <div>
              <h2 class="text-lg font-bold mb-1"
                  :class="isDark ? 'text-white/80' : 'text-gray-800'">
                Content Surface
              </h2>
              <p class="text-sm leading-relaxed"
                 :class="isDark ? 'text-white/30' : 'text-gray-400'">
                Ask about films, songs, or podcasts in the chat to see rich content here.
              </p>
            </div>
          </div>
        </div>
      </main>

      <!-- Detail panel (wide desktop only — persistent, no close/back) -->
      <div
        v-if="isWideDesktop"
        class="flex-1 min-w-0 path-glass-card overflow-hidden flex flex-col order-3 detail-persistent panel-slide-in"
        style="animation-delay: 0.2s"
      >
        <template v-if="hasDetailOpen">
          <ErrorBoundary title="Detail view error">
            <DetailView />
          </ErrorBoundary>
        </template>
        <div v-else class="flex-1 flex items-center justify-center">
          <div class="text-center space-y-4 max-w-[200px] px-4">
            <div class="empty-state-icon w-16 h-16 rounded-2xl path-glass-icon flex items-center justify-center mx-auto overflow-hidden">
              <span class="text-2xl" :class="isDark ? 'text-[#fafafa]' : 'text-gray-800'">✦</span>
            </div>
            <div>
              <h2 class="text-sm font-semibold mb-1"
                  :class="isDark ? 'text-white/60' : 'text-gray-600'">
                Awaiting Context
              </h2>
              <p class="text-xs leading-relaxed"
                 :class="isDark ? 'text-white/25' : 'text-gray-400'">
                Select an item to see details here.
              </p>
            </div>
          </div>
        </div>
      </div>

      <!-- Chat panel -->
      <aside
        class="relative z-[100] w-80 xl:w-96 shrink-0 flex flex-col path-glass-card overflow-visible panel-slide-in"
        style="animation-delay: 0s"
        :class="isWideDesktop
          ? (panelSide === 'left' ? 'order-1' : 'order-2')
          : (panelSide === 'left' ? 'order-first' : 'order-last')"
      >
        <ErrorBoundary title="Chat error">
          <ChatWindow
            :side="panelSide"
            @switch-side="chatStore.switchSide()"
          />
        </ErrorBoundary>
      </aside>
    </div>

    <!-- Mobile layout -->
    <div v-if="isMobile" class="flex-1 flex flex-col min-h-0">
      <!-- Chat view -->
      <div v-show="mobileTab === 'chat'" class="flex-1 min-h-0 flex flex-col p-2 pb-0">
        <div class="flex-1 min-h-0 path-glass-card flex flex-col rounded-2xl overflow-hidden">
          <ErrorBoundary title="Chat error">
            <ChatWindow
              variant="standalone"
              :show-close="false"
            />
          </ErrorBoundary>
        </div>
      </div>

      <!-- Content view -->
      <div v-show="mobileTab === 'content'" class="flex-1 min-h-0 flex flex-col p-2 pb-0">
        <div class="flex-1 min-h-0 path-glass-card flex flex-col rounded-2xl overflow-hidden">
          <template v-if="panelOpen">
            <div class="shrink-0 flex items-center gap-2 px-3 pt-2 pb-1 overflow-x-auto scrollbar-hide">
              <button
                class="p-1.5 rounded-lg transition-colors shrink-0"
                :class="isDark
                  ? 'text-white/50 hover:text-white/80 hover:bg-white/5'
                  : 'text-gray-400 hover:text-gray-700 hover:bg-black/5'"
                @click="mobileTab = 'chat'"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
              </button>
              <button
                v-for="tab in availableTabs"
                :key="tab"
                class="text-xs px-2.5 py-1.5 rounded-lg font-medium whitespace-nowrap transition-all duration-150"
                :class="activeTab === tab
                  ? 'nav-tab-active'
                  : isDark
                    ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
                    : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
                @click="setActiveTab(tab)"
              >
                {{ tabLabel(tab) }}
              </button>
            </div>
            <ErrorBoundary title="Content failed to load">
              <ContentGridView
                :active-tab="activeTab"
                :is-wide-desktop="isWideDesktop"
                :is-mobile="isMobile"
                :panel-films="panelFilms"
                :panel-books="panelBooks"
                :panelTVSeries="panelTVSeries"
                :panel-images="panelImages"
                :panel-places="panelPlaces"
                :panel-songs="panelSongs"
                :panel-podcasts="panelPodcasts"
                :panel-web-results="panelWebResults"
                :panel-websites="panelWebsites"
                :panel-magazine-sections="panelMagazineSections"
                :panel-magazine-hero-image="panelMagazineHeroImage"
                :panel-recipes="panelRecipes"
                :panel-apps="panelApps"
                :panel-title="panelTitle"
                :panel-query="panelQuery"
                @close="closePanel"
              />
            </ErrorBoundary>
          </template>
          <template v-else-if="chatStore.isStreaming">
            <ContextLoader :context-type="loaderContextType" />
          </template>
          <div v-else class="flex-1 flex items-center justify-center">
            <div class="text-center space-y-3 px-6">
              <div class="empty-state-icon w-16 h-16 rounded-2xl path-glass-icon flex items-center justify-center mx-auto overflow-hidden">
                <span class="text-2xl" :class="isDark ? 'text-[#fafafa]' : 'text-gray-800'">✦</span>
              </div>
              <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
                Ask about something in the chat to see content here.
              </p>
            </div>
          </div>
        </div>
      </div>

      <!-- Context view (detail panel) -->
      <div v-show="mobileTab === 'context'" class="flex-1 min-h-0 flex flex-col p-2 pb-0">
        <div class="flex-1 min-h-0 path-glass-card flex flex-col rounded-2xl overflow-hidden">
          <template v-if="hasDetailOpen">
            <div class="flex-1 min-h-0 flex flex-col">
              <ErrorBoundary title="Detail view error">
                <DetailView />
              </ErrorBoundary>
            </div>
            <!-- Navigation bar (skip for magazine sections which have their own) -->
            <div
              v-if="!selectedMagazineSection && contextItems.length > 1"
              class="shrink-0 flex items-center justify-between px-4 py-2"
              :style="isDark
                ? 'border-top: 1px solid rgba(255, 255, 255, 0.08)'
                : 'border-top: 1px solid rgba(0, 0, 0, 0.06)'"
            >
              <button
                class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-colors"
                :class="isDark
                  ? 'text-white/50 hover:text-white/80 hover:bg-white/5'
                  : 'text-black/40 hover:text-black/70 hover:bg-black/5'"
                @click="navigateContext('prev')"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
                Prev
              </button>
              <span class="text-xs font-mono tabular-nums"
                    :class="isDark ? 'text-white/25' : 'text-black/25'">
                {{ currentContextIndex + 1 }}/{{ contextItems.length }}
              </span>
              <button
                class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-colors"
                :class="isDark
                  ? 'text-white/50 hover:text-white/80 hover:bg-white/5'
                  : 'text-black/40 hover:text-black/70 hover:bg-black/5'"
                @click="navigateContext('next')"
              >
                Next
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </button>
            </div>
          </template>
          <div v-else class="flex-1 flex items-center justify-center">
            <div class="text-center space-y-3 px-6">
              <div class="empty-state-icon w-16 h-16 rounded-2xl path-glass-icon flex items-center justify-center mx-auto overflow-hidden">
                <svg class="w-7 h-7" :class="isDark ? 'text-white/30' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              </div>
              <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
                Tap an item in Content to view details here.
              </p>
            </div>
          </div>
        </div>
      </div>

      <!-- Inline player bar on mobile (above tab bar) -->
      <PlayerBar variant="inline" compact />

      <!-- Bottom tab bar (iOS HIG: 49pt + safe area + 24px margin) -->
      <div
        v-show="!isKeyboardOpen"
        class="shrink-0 pt-3 pb-3"
        :style="{ paddingBottom: 'calc(12px + env(safe-area-inset-bottom, 0px))' }"
      >
        <div class="flex items-center h-[49px] px-2 gap-1">
          <button
            class="flex-1 flex flex-col items-center justify-center h-[49px] min-h-[44px] rounded-xl text-[10px] font-medium tracking-wide transition-all duration-150 gap-0.5"
            :class="mobileTab === 'chat'
              ? isDark ? 'bg-white/10 text-white/90' : 'bg-black/8 text-gray-900'
              : isDark ? 'text-white/40 hover:text-white/60' : 'text-gray-400 hover:text-gray-600'"
            @click="mobileTab = 'chat'"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
            </svg>
            <span>Chat</span>
          </button>
          <button
            class="flex-1 flex flex-col items-center justify-center h-[49px] min-h-[44px] rounded-xl text-[10px] font-medium tracking-wide transition-all duration-150 relative gap-0.5"
            :class="mobileTab === 'content'
              ? isDark ? 'bg-white/10 text-white/90' : 'bg-black/8 text-gray-900'
              : isDark ? 'text-white/40 hover:text-white/60' : 'text-gray-400 hover:text-gray-600'"
            @click="mobileTab = 'content'"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
            </svg>
            <span>Content</span>
            <span
              v-if="panelOpen && mobileTab === 'chat'"
              class="absolute top-1.5 right-[28%] w-1.5 h-1.5 rounded-full bg-accent"
            />
          </button>
          <button
            class="flex-1 flex flex-col items-center justify-center h-[49px] min-h-[44px] rounded-xl text-[10px] font-medium tracking-wide transition-all duration-150 relative gap-0.5"
            :class="mobileTab === 'context'
              ? isDark ? 'bg-white/10 text-white/90' : 'bg-black/8 text-gray-900'
              : isDark ? 'text-white/40 hover:text-white/60' : 'text-gray-400 hover:text-gray-600'"
            @click="mobileTab = 'context'"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
            </svg>
            <span>Context</span>
            <span
              v-if="hasDetailOpen && mobileTab !== 'context'"
              class="absolute top-1.5 right-[28%] w-1.5 h-1.5 rounded-full bg-accent"
            />
          </button>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch } from 'vue'
import { useChatStore } from '@/stores/chat'
import { useTheme } from '@/composables/useTheme'
import { useAI } from '@/composables/useAI'
import { useContentPanel, type ContentTab } from '@/composables/useContentPanel'
import { useVisualViewport } from '@/composables/useVisualViewport'
import ChatWindow from '@/components/chat/ChatWindow.vue'
import ContentGridView from '@/components/content/ContentGridView.vue'
import DetailView from '@/components/content/DetailView.vue'
import CloseButton from '@/components/content/CloseButton.vue'
import ContextLoader from '@/components/content/ContextLoader.vue'
import ErrorBoundary from '@/components/ui/ErrorBoundary.vue'
import PlayerBar from '@/components/player/PlayerBar.vue'
import { usePlayer } from '@/composables/usePlayer'
import { useCodeContext } from '@/composables/useCodeContext'

const { hasTrack: playerActive } = usePlayer()
const chatStore = useChatStore()
const { activeFile: codeActiveFile, isCodeMode, exitCodeMode, clearActiveFile: clearCodeFile } = useCodeContext()
const { isDark } = useTheme()

// Detect if running inside Archy's iframe — transparent bg, hide player bar
const isEmbedded = !!(window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__

// Background image URL — use BASE_URL for correct path in all deployments
// 2912×1632 jpg was 1052K — visible as a slow paint over Tailscale/Tor.
// The webp (1920w, q82) is 478K with no visible difference behind glass.
const bgImageUrl = `${import.meta.env.BASE_URL}assets/img/bg-intro-3.webp`

useAI()

const {
  panelOpen,
  panelFilms,
  panelBooks,
  panelTVSeries,
  panelImages,
  panelPlaces,
  panelSongs,
  panelPodcasts,
  panelWebResults,
  panelWebsites,
  panelMagazineSections,
  panelMagazineHeroImage,
  panelRecipes,
  panelApps,
  panelTitle,
  panelQuery,
  panelResponseText,
  contentType,
  activeTab,
  availableTabs,
  setActiveTab,
  selectedFilm,
  selectedBook,
  selectedTVSeries,
  selectedImage,
  selectedPlace,
  selectedSong,
  selectedPodcast,
  selectedArticle,
  selectedWebsite,
  selectedMagazineSection,
  selectedDesignSystemItem,
  closeFilmDetail,
  closeBookDetail,
  closeTVSeriesDetail,
  closeImageDetail,
  closePlaceDetail,
  closeSongDetail,
  closePodcastDetail,
  closeArticleDetail,
  closeWebsiteDetail,
  closeMagazineSectionDetail,
  closePanel,
  openFilmDetail,
  openBookDetail,
  openTVSeriesDetail,
  openImageDetail,
  openPlaceDetail,
  openSongDetail,
  openPodcastDetail,
  openMagazineSectionDetail,
} = useContentPanel()

const panelSide = computed(() => chatStore.panelSide)

// Mobile detection
const windowWidth = ref(window.innerWidth)
const isMobile = computed(() => windowWidth.value < 1024)
const isWideDesktop = computed(() => windowWidth.value >= 1440)
const mobileTab = ref<'chat' | 'content' | 'context'>('chat')
const { isKeyboardOpen } = useVisualViewport()

function onResize() { windowWidth.value = window.innerWidth }

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isCodeMode.value && !isMobile.value) {
    exitCodeMode()
  }
}

// D-14b (Archipelago phase 02-ui-performance): when the host explicitly asks
// for a chat-first mobile start via ?mobileChat, force the initial mobile tab
// to 'chat' on mount. mobileTab already defaults to 'chat', but ChatPage can
// remount while content/detail selection state (module-singleton refs in
// useContentPanel) survives from a previous mount within the same AIUI
// session — this guarantees the documented default regardless of that carry-
// over. It only runs once, at mount, so it never interferes with the
// hasDetailOpen/panelOpen watchers below responding to real user taps after
// the initial paint.
const startOnMobileChat = new URLSearchParams(window.location.search).has('mobileChat')

onMounted(() => {
  window.addEventListener('resize', onResize)
  window.addEventListener('keydown', onKeydown)
  if (startOnMobileChat && isMobile.value) {
    mobileTab.value = 'chat'
  }
})
onUnmounted(() => {
  window.removeEventListener('resize', onResize)
  window.removeEventListener('keydown', onKeydown)
})

// Auto-switch to content tab on mobile when panel opens or content changes
watch(panelOpen, (open) => {
  if (open && isMobile.value && !isEmbedded) mobileTab.value = 'content'
})
watch(panelTitle, () => {
  if (panelOpen.value && isMobile.value && mobileTab.value === 'chat' && !isEmbedded) mobileTab.value = 'content'
})
watch(activeTab, () => {
  if (panelOpen.value && isMobile.value && mobileTab.value === 'chat' && !isEmbedded) mobileTab.value = 'content'
})

const hasDetailOpen = computed(() =>
  !!(selectedFilm.value || selectedBook.value || selectedTVSeries.value ||
     selectedImage.value || selectedPlace.value || selectedSong.value || selectedPodcast.value || selectedArticle.value || selectedWebsite.value || selectedMagazineSection.value ||
     selectedDesignSystemItem.value ||
     (isCodeMode.value && codeActiveFile.value))
)

watch(hasDetailOpen, (open) => {
  if (open && isMobile.value) mobileTab.value = 'context'
  if (!open && isMobile.value && mobileTab.value === 'context') mobileTab.value = 'content'
})

const hasGridContent = computed(() =>
  panelOpen.value && availableTabs.value.length > 0
)

function closeAllDetails() {
  closeFilmDetail()
  closeBookDetail()
  closeTVSeriesDetail()
  closeImageDetail()
  closePlaceDetail()
  closeSongDetail()
  closePodcastDetail()
  closeArticleDetail()
  closeWebsiteDetail()
  closeMagazineSectionDetail()
  clearCodeFile()
}

// Context navigation — flat list of all detail-openable items
interface ContextNavItem {
  open: () => void
}

const contextItems = computed<ContextNavItem[]>(() => {
  const items: ContextNavItem[] = []
  panelFilms.value.forEach(f => items.push({ open: () => openFilmDetail(f) }))
  panelBooks.value.forEach(b => items.push({ open: () => openBookDetail(b) }))
  panelTVSeries.value.forEach(s => items.push({ open: () => openTVSeriesDetail(s) }))
  panelSongs.value.forEach(s => items.push({ open: () => openSongDetail(s) }))
  panelPodcasts.value.forEach(p => items.push({ open: () => openPodcastDetail(p) }))
  panelImages.value.forEach(img => items.push({ open: () => openImageDetail(img) }))
  panelPlaces.value.forEach(p => items.push({ open: () => openPlaceDetail(p) }))
  panelMagazineSections.value.forEach((s, i) => items.push({ open: () => openMagazineSectionDetail(s, i) }))
  return items
})

const currentContextIndex = computed(() => {
  // Determine which item is currently selected
  if (selectedFilm.value) {
    const idx = panelFilms.value.indexOf(selectedFilm.value)
    return idx >= 0 ? idx : 0
  }
  if (selectedBook.value) {
    const idx = panelBooks.value.indexOf(selectedBook.value)
    return idx >= 0 ? panelFilms.value.length + idx : 0
  }
  if (selectedTVSeries.value) {
    const idx = panelTVSeries.value.indexOf(selectedTVSeries.value)
    return idx >= 0 ? panelFilms.value.length + panelBooks.value.length + idx : 0
  }
  if (selectedSong.value) {
    const base = panelFilms.value.length + panelBooks.value.length + panelTVSeries.value.length
    const idx = panelSongs.value.indexOf(selectedSong.value)
    return idx >= 0 ? base + idx : 0
  }
  if (selectedPodcast.value) {
    const base = panelFilms.value.length + panelBooks.value.length + panelTVSeries.value.length + panelSongs.value.length
    const idx = panelPodcasts.value.indexOf(selectedPodcast.value)
    return idx >= 0 ? base + idx : 0
  }
  if (selectedImage.value) {
    const base = panelFilms.value.length + panelBooks.value.length + panelTVSeries.value.length + panelSongs.value.length + panelPodcasts.value.length
    const idx = panelImages.value.indexOf(selectedImage.value)
    return idx >= 0 ? base + idx : 0
  }
  if (selectedPlace.value) {
    const base = panelFilms.value.length + panelBooks.value.length + panelTVSeries.value.length + panelSongs.value.length + panelPodcasts.value.length + panelImages.value.length
    const idx = panelPlaces.value.indexOf(selectedPlace.value)
    return idx >= 0 ? base + idx : 0
  }
  if (selectedMagazineSection.value) {
    const base = panelFilms.value.length + panelBooks.value.length + panelTVSeries.value.length + panelSongs.value.length + panelPodcasts.value.length + panelImages.value.length + panelPlaces.value.length
    const idx = panelMagazineSections.value.indexOf(selectedMagazineSection.value)
    return idx >= 0 ? base + idx : 0
  }
  return 0
})

function navigateContext(direction: 'prev' | 'next') {
  const items = contextItems.value
  if (!items.length) return
  let idx = currentContextIndex.value
  idx += direction === 'next' ? 1 : -1
  if (idx < 0) idx = items.length - 1
  if (idx >= items.length) idx = 0
  closeAllDetails()
  items[idx].open()
}

const TAB_LABELS: Record<ContentTab, string> = {
  film: 'Films',
  book: 'Books',
  tvshow: 'TV',
  image: 'Images',
  place: 'Places',
  recipe: 'Recipes',
  song: 'Songs',
  podcast: 'Podcasts',
  news: 'News',
  websites: 'Websites',
  magazine: 'Brief',
  code: 'Code',
  'design-system': 'Design',
  app: 'Apps',
  nostr: 'Nostr',
  favorites: 'Favorites',
  discover: 'Discover',
  prompt: 'Prompt',
}

function tabLabel(tab: ContentTab): string {
  return TAB_LABELS[tab] ?? tab
}

const loaderContextType = computed(() => {
  if (panelOpen.value) return contentType.value
  const lastUser = [...chatStore.messages].reverse().find((m) => m.role === 'user')
  const q = (lastUser?.content ?? '').toLowerCase()
  // Podcast before song: its words are the specific ones, so "listen to a
  // podcast" must not be swallowed by the song rule's bare `listen`.
  // Mirrors contentFiltering.ts's preferredFirstTab ordering — these two
  // classifiers label the same panel and must not disagree.
  if (/\b(podcast|episode)\b/.test(q)) return 'podcast'
  if (/\b(song|music|track|album|band|artist|listen)\b/.test(q)) return 'song'
  if (/\b(book|novel|read|author|fiction|nonfiction|memoir)\b/.test(q)) return 'book'
  if (/\b(tv show|tv series|series|television|binge|season)\b/.test(q)) return 'tvshow'
  if (/\b(image|images|photo|photos|picture|pictures|screenshot|gallery|artwork|illustration)\b/.test(q)) return 'image'
  if (/\b(restaurant|restaurants|place|places|food|eat|dining|cafe|bar|pub|brunch|lunch|dinner)\b/.test(q)) return 'film'
  if (/\b(news|latest|recent|current|what'?s happening|what are people saying)\b/.test(q)) return 'news'
  if (/\b(bip|protocol|debate|sentiment|bearish|bull case|macro)\b/.test(q)) return 'magazine'
  if (/\b(website|websites|where to check|best places|check online|resources?|sources?)\b/.test(q)) return 'websites'
  return contentType.value
})
</script>

<style scoped>
.content-fade-enter-active,
.content-fade-leave-active {
  transition: opacity 0.2s ease;
}
.content-fade-enter-from,
.content-fade-leave-to {
  opacity: 0;
}
.detail-active {
  border-color: transparent !important;
}
/* Hide back buttons in persistent detail panel on wide desktops */
.detail-persistent :deep(button[class*="absolute"][class*="top-3"][class*="left-3"]) {
  display: none !important;
}
</style>
