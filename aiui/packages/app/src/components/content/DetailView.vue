<template>
  <FilmDetail
    v-if="selectedFilm"
    :film="selectedFilm"
    @back="closeFilmDetail"
  />
  <SongDetail
    v-else-if="selectedSong"
    :song="selectedSong"
    @back="closeSongDetail"
  />
  <PodcastDetail
    v-else-if="selectedPodcast"
    :podcast="selectedPodcast"
    @back="closePodcastDetail"
  />
  <BookDetail
    v-else-if="selectedBook"
    :book="selectedBook"
    @back="closeBookDetail"
  />
  <TVSeriesDetail
    v-else-if="selectedTVSeries"
    :series="selectedTVSeries"
    @back="closeTVSeriesDetail"
  />
  <ImageDetail
    v-else-if="selectedImage"
    :image="selectedImage"
    @back="closeImageDetail"
  />
  <PlaceDetail
    v-else-if="selectedPlace"
    :place="selectedPlace"
    @back="closePlaceDetail"
  />
  <ArticleDetail
    v-else-if="selectedArticle"
    :article="selectedArticle"
    @back="closeArticleDetail"
  />
  <WebsiteDetail
    v-else-if="selectedWebsite"
    :website="selectedWebsite"
    @back="closeWebsiteDetail"
  />
  <MagazineSectionDetail
    v-else-if="selectedMagazineSection"
    :section="selectedMagazineSection"
    :current-index="magazineSectionIndex"
    :total-sections="panelMagazineSections.length"
    @back="closeMagazineSectionDetail"
    @navigate="navigateMagazineSection"
  />
  <CodeDetail
    v-else-if="isCodeMode && activeCodeFile"
    @back="closeCodeFile"
  />
  <DesignSystemDetail
    v-else-if="selectedDesignSystemItem"
    :item="selectedDesignSystemItem"
    @back="closeDesignSystemItem"
  />
</template>

<script setup lang="ts">
import { useContentPanel } from '@/composables/useContentPanel'
import FilmDetail from './FilmDetail.vue'
import BookDetail from './BookDetail.vue'
import TVSeriesDetail from './TVSeriesDetail.vue'
import SongDetail from './SongDetail.vue'
import PodcastDetail from './PodcastDetail.vue'
import ImageDetail from './ImageDetail.vue'
import PlaceDetail from './PlaceDetail.vue'
import ArticleDetail from './ArticleDetail.vue'
import WebsiteDetail from './WebsiteDetail.vue'
import MagazineSectionDetail from './MagazineSectionDetail.vue'
import CodeDetail from './CodeDetail.vue'
import DesignSystemDetail from './DesignSystemDetail.vue'
import { useCodeContext } from '@/composables/useCodeContext'

const { isCodeMode, activeFile: activeCodeFile } = useCodeContext()

function closeCodeFile() {
  const { activeFile, activeFileContent } = useCodeContext()
  activeFile.value = null
  activeFileContent.value = ''
}

const {
  selectedFilm,
  selectedBook,
  selectedTVSeries,
  selectedImage,
  selectedPlace,
  selectedSong,
  selectedPodcast,
  selectedArticle,
  closeFilmDetail,
  closeBookDetail,
  closeTVSeriesDetail,
  closeImageDetail,
  closePlaceDetail,
  closeSongDetail,
  closePodcastDetail,
  closeArticleDetail,
  selectedWebsite,
  closeWebsiteDetail,
  selectedMagazineSection,
  magazineSectionIndex,
  panelMagazineSections,
  closeMagazineSectionDetail,
  navigateMagazineSection,
  selectedDesignSystemItem,
  closeDesignSystemItem,
} = useContentPanel()
</script>
