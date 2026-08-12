<template>
  <div class="flex-1 min-h-0 flex flex-col">
    <FilmGrid
      v-if="activeTab === 'film'"
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
    <SongGrid
      v-else-if="activeTab === 'song'"
      :songs="panelSongs"
      :title="panelTitle"
      @select-song="openSongDetail"
    />
    <MagazineGrid
      v-else-if="activeTab === 'magazine'"
      :sections="panelMagazineSections"
      :hero-image-url="panelMagazineHeroImage"
      :title="panelTitle"
      :query="panelQuery"
    />
    <NewsGrid
      v-else-if="activeTab === 'news'"
      :articles="panelWebResults"
      :title="panelTitle"
      :query="panelQuery"
    />
    <NewsGrid
      v-else-if="activeTab === 'websites'"
      :articles="panelWebsites"
      :title="panelTitle"
      variant="websites"
    />
    <PodcastGrid
      v-else-if="activeTab === 'podcast'"
      :podcasts="panelPodcasts"
      :title="panelTitle"
      @select-podcast="openPodcastDetail"
    />
    <RecipeGrid
      v-else-if="activeTab === 'recipe'"
      :recipes="panelRecipes"
      :title="panelTitle"
      @select-recipe="openRecipeDetail"
    />
    <AppsGrid
      v-else-if="activeTab === 'app'"
      :apps="panelApps"
      :title="panelTitle"
      @select-app="openAppDetail"
    />
    <ProjectGrid
      v-else-if="activeTab === 'code'"
      :is-wide-desktop="isWideDesktop"
      :is-mobile="isMobile"
    />
    <DesignSystemGrid
      v-else-if="activeTab === 'design-system'"
    />
    <NostrGrid
      v-else-if="activeTab === 'nostr'"
    />
    <MagazineGrid
      v-else-if="activeTab === 'prompt'"
      :sections="promptSections"
      :hero-image-url="null"
      title="Prompt"
      :query="panelQuery"
    />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Film, Song, Podcast, Book, TVSeries, ImageItem, Place } from '@aiui/core/types/content'
import type { WebSearchResult } from '@aiui/core/types/message'
import type { ContentTab, MagazineSection } from '@/composables/useContentPanel'
import type { RecipeData, AppEntry } from '@/composables/contentExtraction'
import { useContentPanel } from '@/composables/useContentPanel'
import { extractMagazineSections, stripContentTags } from '@/composables/contentExtraction'
import FilmGrid from './FilmGrid.vue'
import BookGrid from './BookGrid.vue'
import TVSeriesGrid from './TVSeriesGrid.vue'
import ImageGrid from './ImageGrid.vue'
import PlaceGrid from './PlaceGrid.vue'
import SongGrid from './SongGrid.vue'
import PodcastGrid from './PodcastGrid.vue'
import MagazineGrid from './MagazineGrid.vue'
import NewsGrid from './NewsGrid.vue'
import RecipeGrid from './RecipeGrid.vue'
import AppsGrid from './AppsGrid.vue'
import ProjectGrid from './ProjectGrid.vue'
import DesignSystemGrid from './DesignSystemGrid.vue'
import NostrGrid from './NostrGrid.vue'

const props = defineProps<{
  activeTab: ContentTab
  isWideDesktop?: boolean
  isMobile?: boolean
  panelFilms: Film[]
  panelBooks: Book[]
  panelTVSeries: TVSeries[]
  panelImages: ImageItem[]
  panelPlaces: Place[]
  panelSongs: Song[]
  panelPodcasts: Podcast[]
  panelWebResults: WebSearchResult[]
  panelWebsites: WebSearchResult[]
  panelMagazineSections: MagazineSection[]
  panelMagazineHeroImage: string | null
  panelRecipes: RecipeData[]
  panelApps: AppEntry[]
  panelTitle: string
  panelQuery: string
  panelResponseText?: string
}>()

const promptSections = computed<MagazineSection[]>(() => {
  const text = props.panelResponseText ?? ''
  if (!text) return [{ title: props.panelQuery || 'Prompt', content: '' }]
  // Use magazine extraction to format the response beautifully
  const sections = extractMagazineSections(text)
  if (sections.length > 0) return sections
  // Fallback: single section with cleaned response
  return [{ title: props.panelQuery || 'Response', content: stripContentTags(text) }]
})

const {
  openFilmDetail,
  openBookDetail,
  openTVSeriesDetail,
  openImageDetail,
  openPlaceDetail,
  openSongDetail,
  openPodcastDetail,
  openRecipeDetail,
  openAppDetail,
} = useContentPanel()
</script>
