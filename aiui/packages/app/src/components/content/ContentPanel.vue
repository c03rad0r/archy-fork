<template>
  <Transition name="panel">
    <aside
      v-if="panelOpen"
      class="path-glass-card overflow-hidden flex flex-col"
      :class="isMobile
        ? 'fixed inset-0 z-30'
        : 'w-80 xl:w-96 shrink-0'"
    >
      <!-- Mobile header -->
      <div
        v-if="isMobile"
        class="p-3 flex items-center justify-between shrink-0"
        :style="isDark
          ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
          : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
      >
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ panelTitle }}
        </h3>
        <button
          class="touch-target rounded-lg transition-colors"
          :class="isDark
            ? 'text-white/70 hover:bg-white/10'
            : 'text-gray-500 hover:bg-black/5'"
          @click="closePanel"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Tab bar (when multiple tabs available) -->
      <div
        v-if="displayTabs.length > 1 && !hasDetailOpen"
        class="flex items-center gap-2 px-3 pt-3 pb-1 shrink-0 overflow-x-auto scrollbar-hide"
      >
        <button
          v-for="tab in displayTabs"
          :key="tab"
          class="text-xs px-2.5 min-h-[44px] flex items-center justify-center rounded-lg font-medium whitespace-nowrap transition-all duration-150"
          :class="activeTab === tab
            ? 'nav-tab-active'
            : isDark
              ? 'text-white/40 hover:text-white/70 hover:bg-white/5'
              : 'text-gray-500 hover:text-gray-800 hover:bg-black/5'"
          @click="tab === 'favorites' || tab === 'discover' ? (activeTab = tab) : setActiveTab(tab)"
        >
          {{ tabLabel(tab) }}
        </button>
      </div>

      <!-- Content area -->
      <div class="flex-1 min-h-0">
        <ErrorBoundary title="Content failed to load">
          <!-- Detail views (override tab content) -->
          <component
            :is="filmRenderer?.panelPlay"
            v-if="selectedFilm && filmRenderer?.panelPlay"
            :key="selectedFilm.id"
            :film="selectedFilm"
            @back="closeFilmDetail"
          />
          <BookDetail
            v-else-if="selectedBook"
            :key="selectedBook.id"
            :book="selectedBook"
            @back="closeBookDetail"
          />
          <TVSeriesDetail
            v-else-if="selectedTVSeries"
            :key="selectedTVSeries.id"
            :series="selectedTVSeries"
            @back="closeTVSeriesDetail"
          />
          <component
            :is="songRenderer?.panelPlay"
            v-else-if="selectedSong && songRenderer?.panelPlay"
            :key="selectedSong.id"
            :song="selectedSong"
            @back="closeSongDetail"
          />
          <PodcastDetail
            v-else-if="selectedPodcast"
            :key="selectedPodcast.id"
            :podcast="selectedPodcast"
            @back="closePodcastDetail"
          />
          <ImageDetail
            v-else-if="selectedImage"
            :key="selectedImage.url"
            :image="selectedImage"
            @back="closeImageDetail"
          />
          <PlaceDetail
            v-else-if="selectedPlace"
            :key="selectedPlace.id"
            :place="selectedPlace"
            @back="closePlaceDetail"
          />
          <RecipeDetail
            v-else-if="selectedRecipe"
            :key="selectedRecipe.title"
            :recipe="selectedRecipe"
            @back="closeRecipeDetail"
          />
          <AppDetail
            v-else-if="selectedApp"
            :key="selectedApp.id"
            :app="selectedApp"
            @back="closeAppDetail"
            @select-app="openAppDetail"
          />
          <ArticleDetail
            v-else-if="selectedArticle"
            :key="selectedArticle.url"
            :article="selectedArticle"
            @back="closeArticleDetail"
          />
          <ArticleReader
            v-else-if="longFormArticle"
            :content="longFormArticle.content"
            :title="longFormArticle.title"
            @back="closeLongFormArticle"
          />
          <PdfViewer
            v-else-if="pdfUrl"
            :url="pdfUrl.url"
            :title="pdfUrl.title"
            @back="closePdfViewer"
          />
          <MapRenderer
            v-else-if="mapPlaces.length > 0"
            :places="mapPlaces"
            @back="closeMapView"
          />

          <!-- Grid views by active tab -->
          <component
            :is="filmRenderer?.panelPreview"
            v-else-if="activeTab === 'film' && filmRenderer?.panelPreview"
            :films="panelFilms"
            :title="panelTitle"
            @select-film="openFilmDetail"
          />
          <BookGrid
            v-else-if="activeTab === 'book'"
            :books="panelBooks"
            :title="panelTitle"
            @select-book="openBookDetail"
          />
          <TVSeriesGrid
            v-else-if="activeTab === 'tvshow'"
            :series="panelTVSeries"
            :title="panelTitle"
            @select-series="openTVSeriesDetail"
          />
          <component
            :is="songRenderer?.panelPreview"
            v-else-if="activeTab === 'song' && songRenderer?.panelPreview"
            :songs="panelSongs"
            :title="panelTitle"
            @select-song="openSongDetail"
          />
          <PodcastGrid
            v-else-if="activeTab === 'podcast'"
            :podcasts="panelPodcasts"
            :title="panelTitle"
            @select-podcast="openPodcastDetail"
          />
          <ImageGrid
            v-else-if="activeTab === 'image'"
            :images="panelImages"
            :title="panelTitle"
            @select-image="openImageDetail"
          />
          <PlaceGrid
            v-else-if="activeTab === 'place'"
            :places="panelPlaces"
            :title="panelTitle"
            @select-place="openPlaceDetail"
          />
          <RecipeGrid
            v-else-if="activeTab === 'recipe'"
            :recipes="panelRecipes"
            :title="panelTitle"
            @select-recipe="openRecipeDetail"
          />
          <NewsGrid
            v-else-if="activeTab === 'news'"
            :articles="panelWebResults"
            :title="panelTitle"
            :query="panelQuery"
            variant="news"
          />
          <NewsGrid
            v-else-if="activeTab === 'websites'"
            :articles="panelWebsites"
            :title="panelTitle"
            :query="panelQuery"
            variant="websites"
          />
          <MagazineGrid
            v-else-if="activeTab === 'magazine'"
            :sections="panelMagazineSections"
            :title="panelTitle"
            :query="panelQuery"
            :hero-image="panelMagazineHeroImage ?? undefined"
          />
          <ArchyAppsGrid
            v-else-if="activeTab === 'app' && isArchyEmbedded"
          />
          <AppsGrid
            v-else-if="activeTab === 'app'"
            :apps="panelApps"
            :title="panelTitle"
            @select-app="openAppDetail"
          />
          <ProjectGrid
            v-else-if="activeTab === 'code'"
          />
          <NostrGrid
            v-else-if="activeTab === 'nostr'"
          />
          <FavoritesGrid
            v-else-if="activeTab === 'favorites'"
          />
          <DiscoverPanel
            v-else-if="activeTab === 'discover'"
          />
        </ErrorBoundary>
      </div>
    </aside>
  </Transition>
</template>

<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, onUnmounted, ref } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useContentPanel, type ContentTab } from '@/composables/useContentPanel'
import { getRendererForContentType } from '@aiui/core'
import BookGrid from './BookGrid.vue'
import BookDetail from './BookDetail.vue'
import TVSeriesGrid from './TVSeriesGrid.vue'
import TVSeriesDetail from './TVSeriesDetail.vue'
import ImageGrid from './ImageGrid.vue'
import ImageDetail from './ImageDetail.vue'
import PlaceGrid from './PlaceGrid.vue'
import PlaceDetail from './PlaceDetail.vue'
import RecipeGrid from './RecipeGrid.vue'
import RecipeDetail from './RecipeDetail.vue'
import PodcastGrid from './PodcastGrid.vue'
import PodcastDetail from './PodcastDetail.vue'
import NewsGrid from './NewsGrid.vue'
import ArticleDetail from './ArticleDetail.vue'
import ArticleReader from '@/components/renderers/ArticleReader.vue'
const PdfViewer = defineAsyncComponent({
  loader: () => import('@/components/renderers/PdfViewer.vue'),
  loadingComponent: { template: '<div class="flex items-center justify-center h-32"><span class="text-sm text-white/50">Loading PDF viewer...</span></div>' },
})
const MapRenderer = defineAsyncComponent({
  loader: () => import('@/components/renderers/MapRenderer.vue'),
  loadingComponent: { template: '<div class="flex items-center justify-center h-32"><span class="text-sm text-white/50">Loading map...</span></div>' },
})
import MagazineGrid from './MagazineGrid.vue'
import AppsGrid from './AppsGrid.vue'
import ArchyAppsGrid from './ArchyAppsGrid.vue'
import AppDetail from './AppDetail.vue'
import ProjectGrid from './ProjectGrid.vue'
import NostrGrid from './NostrGrid.vue'
import FavoritesGrid from './FavoritesGrid.vue'
import DiscoverPanel from './DiscoverPanel.vue'
import ErrorBoundary from '@/components/ui/ErrorBoundary.vue'
import { useFavoritesStore } from '@/stores/favorites'
import { useArchy } from '@/composables/useArchy'

// Film and song renderers loaded from plugin registry
const filmRenderer = computed(() => getRendererForContentType('film'))
const songRenderer = computed(() => getRendererForContentType('song'))

const { isDark } = useTheme()
const favoritesStore = useFavoritesStore()
const { isEmbedded: isArchyEmbedded } = useArchy()

const {
  panelOpen,
  panelFilms,
  panelBooks,
  panelTVSeries,
  panelSongs,
  panelPodcasts,
  panelWebResults,
  panelWebsites,
  panelImages,
  panelPlaces,
  panelRecipes,
  panelApps,
  panelMagazineSections,
  panelMagazineHeroImage,
  panelTitle,
  panelQuery,
  activeTab,
  availableTabs,
  selectedFilm,
  selectedBook,
  selectedTVSeries,
  selectedSong,
  selectedPodcast,
  selectedImage,
  selectedPlace,
  selectedArticle,
  selectedRecipe,
  selectedApp,
  selectedDesignSystemItem,
  setActiveTab,
  openFilmDetail,
  closeFilmDetail,
  openBookDetail,
  closeBookDetail,
  openTVSeriesDetail,
  closeTVSeriesDetail,
  openSongDetail,
  closeSongDetail,
  openPodcastDetail,
  closePodcastDetail,
  openImageDetail,
  closeImageDetail,
  openPlaceDetail,
  closePlaceDetail,
  openRecipeDetail,
  closeRecipeDetail,
  openAppDetail,
  closeAppDetail,
  closeArticleDetail,
  longFormArticle,
  closeLongFormArticle,
  pdfUrl,
  closePdfViewer,
  mapPlaces,
  closeMapView,
  closePanel,
} = useContentPanel()

const hasDetailOpen = computed(() =>
  !!(selectedFilm.value || selectedBook.value || selectedTVSeries.value || selectedSong.value || selectedPodcast.value || selectedImage.value || selectedPlace.value || selectedRecipe.value || selectedArticle.value || selectedApp.value || selectedDesignSystemItem.value || longFormArticle.value || pdfUrl.value || mapPlaces.value.length > 0)
)

const windowWidth = ref(window.innerWidth)
const isMobile = computed(() => windowWidth.value < 1024)

const displayTabs = computed(() => {
  const tabs = [...availableTabs.value]
  if (favoritesStore.items.length > 0 && !tabs.includes('favorites')) {
    tabs.push('favorites')
  }
  if (!tabs.includes('discover')) {
    tabs.push('discover')
  }
  return tabs
})

function onResize() {
  windowWidth.value = window.innerWidth
}

const TAB_LABELS: Record<ContentTab, string> = {
  film: 'Films',
  book: 'Books',
  tvshow: 'TV',
  image: 'Images',
  place: 'Places',
  recipe: 'Recipes',
  song: 'Music',
  podcast: 'Podcasts',
  news: 'News',
  websites: 'Web',
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

onMounted(() => window.addEventListener('resize', onResize))
onUnmounted(() => window.removeEventListener('resize', onResize))
</script>

<style scoped>
.panel-enter-active {
  transition: all 0.3s cubic-bezier(0.22, 1, 0.36, 1);
}
.panel-leave-active {
  transition: all 0.2s ease-in;
}
.panel-enter-from {
  opacity: 0;
  transform: translateX(20px);
}
.panel-leave-to {
  opacity: 0;
  transform: translateX(20px);
}
</style>
