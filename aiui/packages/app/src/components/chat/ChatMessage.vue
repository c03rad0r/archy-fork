<template>
  <div
    class="group/msg flex animate-fade-up-fast"
    :class="isUser ? 'justify-end' : 'justify-start'"
    :style="{ animationDelay: `${index * 30}ms` }"
    @contextmenu.prevent="openContextMenu"
  >
    <div class="relative max-w-[85%] md:max-w-[75%]">
      <!-- Action buttons (hover on desktop, always visible on touch) -->
      <div
        v-if="!isEditing"
        class="absolute -top-4 opacity-0 group-hover/msg:opacity-100 transition-opacity duration-200 z-10"
        :class="isUser ? 'right-1' : 'left-1'"
      >
        <div class="flex items-center gap-2 px-1 py-0.5 rounded-lg bg-black/60 backdrop-blur-md border border-white/10 shadow-lg">
          <button
            v-if="isUser"
            class="touch-target rounded-md text-white/50 hover:text-white/90 hover:bg-white/10 transition-colors"
            title="Edit message"
            aria-label="Edit message"
            @click.stop="startEditing"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor"><path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z" /></svg>
          </button>
          <button
            v-if="!isUser"
            class="touch-target rounded-md text-white/50 hover:text-white/90 hover:bg-white/10 transition-colors"
            title="Regenerate response"
            aria-label="Regenerate response"
            @click.stop="$emit('regenerate')"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd" /></svg>
          </button>
          <button
            class="touch-target rounded-md text-white/50 hover:text-white/90 hover:bg-white/10 transition-colors"
            title="Reply"
            aria-label="Reply"
            @click.stop="$emit('reply', message.id, message.content)"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M7.707 3.293a1 1 0 010 1.414L5.414 7H11a7 7 0 017 7v2a1 1 0 11-2 0v-2a5 5 0 00-5-5H5.414l2.293 2.293a1 1 0 11-1.414 1.414l-4-4a1 1 0 010-1.414l4-4a1 1 0 011.414 0z" clip-rule="evenodd" /></svg>
          </button>
          <button
            v-if="!isUser"
            class="touch-target rounded-md text-white/50 hover:text-white/90 hover:bg-white/10 transition-colors"
            title="Branch from here"
            aria-label="Branch from here"
            @click.stop="$emit('branch', message.id)"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M5 3a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2V5a2 2 0 00-2-2H5zM5 11a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2v-2a2 2 0 00-2-2H5zM11 5a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V5z" clip-rule="evenodd" /></svg>
          </button>
          <div v-if="!isUser" class="w-px h-4 bg-white/10 mx-0.5" />
          <button
            v-if="!isUser"
            class="touch-target rounded-md transition-colors"
            :class="message.feedback === 'up' ? 'text-green-400' : 'text-white/50 hover:text-green-400/80 hover:bg-white/10'"
            title="Good response"
            aria-label="Good response"
            @click.stop="toggleFeedback('up')"
          >
            <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M2 10.5a1.5 1.5 0 113 0v6a1.5 1.5 0 01-3 0v-6zM6 10.333v5.43a2 2 0 001.106 1.79l.05.025A4 4 0 008.943 18h5.416a2 2 0 001.962-1.608l1.2-6A2 2 0 0015.56 8H12V4a2 2 0 00-2-2 1 1 0 00-1 1v.667a4 4 0 01-.8 2.4L6.8 7.933a4 4 0 00-.8 2.4z" /></svg>
          </button>
          <button
            v-if="!isUser"
            class="touch-target rounded-md transition-colors"
            :class="message.feedback === 'down' ? 'text-red-400' : 'text-white/50 hover:text-red-400/80 hover:bg-white/10'"
            title="Poor response"
            aria-label="Poor response"
            @click.stop="toggleFeedback('down')"
          >
            <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M18 9.5a1.5 1.5 0 11-3 0v-6a1.5 1.5 0 013 0v6zM14 9.667v-5.43a2 2 0 00-1.106-1.79l-.05-.025A4 4 0 0011.057 2H5.64a2 2 0 00-1.962 1.608l-1.2 6A2 2 0 004.44 12H8v4a2 2 0 002 2 1 1 0 001-1v-.667a4 4 0 01.8-2.4l1.4-1.866a4 4 0 00.8-2.4z" /></svg>
          </button>
        </div>
      </div>

      <div
        class="rounded-2xl px-4 py-3 transition-all duration-300"
        :class="[bubbleClasses, { 'cursor-pointer': hasContext }]"
        @click="handleBubbleClick"
      >
        <!-- Edit mode -->
        <div v-if="isEditing" class="space-y-2" @click.stop>
          <textarea
            ref="editTextareaRef"
            v-model="editContent"
            class="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-sm text-white/90 resize-none focus:outline-none focus:border-accent/50"
            rows="3"
            @keydown.enter.exact="submitEdit"
            @keydown.escape="cancelEdit"
          />
          <div class="flex gap-2 justify-end">
            <button
              class="glass-button-sm min-h-[44px] md:min-h-0 md:!h-7 text-xs text-white/60 hover:text-white/90 px-3"
              @click="cancelEdit"
            >Cancel</button>
            <button
              class="glass-button-sm min-h-[44px] md:min-h-0 md:!h-7 text-xs bg-accent/20 text-accent hover:bg-accent/30 px-3"
              @click="submitEdit"
            >Save & Resend</button>
          </div>
        </div>

        <!-- Normal display -->
        <template v-else>
          <!-- Attached images -->
          <div v-if="message.images && message.images.length > 0" class="flex gap-2 flex-wrap mb-2">
            <img
              v-for="(img, i) in message.images"
              :key="i"
              :src="`data:${img.mediaType};base64,${img.data}`"
              :alt="`Attached image ${i + 1}`"
              class="rounded-lg max-w-[200px] max-h-[200px] object-cover border border-white/10"
            />
          </div>
          <div
            v-if="!isUser"
            class="chat-markdown text-sm leading-relaxed break-words text-white/90"
            v-html="renderedMarkdown"
          />
          <p
            v-else-if="message.content"
            class="text-sm leading-relaxed whitespace-pre-wrap break-words text-white/90"
          >{{ displayText }}</p>
        </template>

      <div v-if="inlineFilms.length > 0" class="mt-3 space-y-1" @click.stop>
        <FilmCard
          v-for="film in inlineFilms"
          :key="film.id"
          :film="film"
          @select="handleFilmSelect"
        />
      </div>

      <div v-if="inlineBooks.length > 0" class="mt-3 space-y-1" @click.stop>
        <BookCard
          v-for="book in inlineBooks"
          :key="book.id"
          :book="book"
          @select="handleBookSelect"
        />
      </div>

      <div v-if="inlineTVSeries.length > 0" class="mt-3 space-y-1" @click.stop>
        <TVSeriesCard
          v-for="series in inlineTVSeries"
          :key="series.id"
          :series="series"
          @select="handleTVSeriesSelect"
        />
      </div>

      <div v-if="inlineSongs.length > 0" class="mt-3 space-y-1" @click.stop>
        <SongCard
          v-for="song in inlineSongs"
          :key="song.id"
          :song="song"
          @select="handleSongSelect"
        />
      </div>

      <div v-if="inlinePodcasts.length > 0" class="mt-3 space-y-1" @click.stop>
        <PodcastCard
          v-for="podcast in inlinePodcasts"
          :key="podcast.id"
          :podcast="podcast"
          @select="handlePodcastSelect"
        />
      </div>

      <div v-if="inlinePlaces.length > 0" class="mt-3 space-y-1" @click.stop>
        <PlaceCard
          v-for="place in inlinePlaces"
          :key="place.id"
          :place="place"
          @select="handlePlaceSelect"
        />
      </div>

      <div v-if="nostrUris.length > 0" class="mt-3 space-y-1.5" @click.stop>
        <NostrEmbed
          v-for="uri in nostrUris"
          :key="uri"
          :uri="uri"
        />
      </div>

      <div v-if="cashuTokens.length > 0" class="mt-3 space-y-1.5" @click.stop>
        <CashuToken
          v-for="token in cashuTokens"
          :key="token"
          :token="token"
        />
      </div>

      <div v-if="inlineRecipes.length > 0" class="mt-3 space-y-2" @click.stop>
        <RecipeCard
          v-for="(recipe, i) in inlineRecipes"
          :key="`recipe-${i}`"
          :recipe="recipe"
        />
      </div>

      <!-- Timeline view for 3+ events -->
      <div v-if="inlineEvents.length >= 3" class="mt-3" @click.stop>
        <TimelineRenderer :events="inlineEvents" />
      </div>
      <!-- Individual event cards for 1-2 events -->
      <div v-else-if="inlineEvents.length > 0" class="mt-3 space-y-2" @click.stop>
        <EventCard
          v-for="(event, i) in inlineEvents"
          :key="`event-${i}`"
          :event="event"
        />
      </div>

      <div v-if="inlineTables.length > 0" class="mt-3 space-y-2" @click.stop>
        <InteractiveTable
          v-for="(table, i) in inlineTables"
          :key="`table-${i}`"
          :headers="table.headers"
          :rows="table.rows"
        />
      </div>

      <div v-if="runnableCodeBlocks.length > 0" class="mt-3 space-y-2" @click.stop>
        <CodeRunner
          v-for="(block, i) in runnableCodeBlocks"
          :key="`code-${i}`"
          :code="block.code"
          :language="block.language"
        />
      </div>

      <div v-if="bitcoinAddresses.length > 0" class="mt-3 space-y-2" @click.stop>
        <BitcoinAddressCard
          v-for="(addr, i) in bitcoinAddresses"
          :key="`btc-${i}`"
          :address="addr.address"
        />
      </div>

      <div v-if="bolt11Invoices.length > 0" class="mt-3 space-y-2" @click.stop>
        <Bolt11InvoiceCard
          v-for="(inv, i) in bolt11Invoices"
          :key="`bolt11-${i}`"
          :invoice="inv.invoice"
        />
      </div>

      <div v-if="bolt12Offers.length > 0" class="mt-3 space-y-2" @click.stop>
        <Bolt12OfferCard
          v-for="(offer, i) in bolt12Offers"
          :key="`bolt12-${i}`"
          :offer="offer.offer"
        />
      </div>

      <div v-if="detectedTxIds.length > 0" class="mt-3 space-y-2" @click.stop>
        <MempoolTxCard
          v-for="(tx, i) in detectedTxIds"
          :key="`tx-${i}`"
          :txid="tx.txid"
        />
      </div>

      <div v-if="inlineNewsLinks.length > 0" class="mt-3 space-y-1" @click.stop>
        <NewsCard
          v-for="(link, i) in inlineNewsLinks"
          :key="i"
          :article="link"
          @select-article="handleArticleSelect"
        />
      </div>

      <div v-if="inlineWebsitesLinks.length > 0" class="mt-3 space-y-1" @click.stop>
        <NewsCard
          v-for="(link, i) in inlineWebsitesLinks"
          :key="`web-${i}`"
          :article="link"
          @select-article="handleWebsiteSelect"
        />
      </div>

      <div v-if="!isEditing" class="flex items-center gap-2 mt-1.5">
        <span class="text-xs select-none text-white/30">{{ formattedTime }}</span>
        <span v-if="message.editedAt" class="text-xs text-white/25 select-none">(edited)</span>
        <span v-if="showTokenCount" class="text-xs text-white/20 select-none" :title="`~${estimatedTokens} tokens`">{{ tokenLabel }}</span>
        <span v-if="message.feedback" class="text-xs select-none">{{ message.feedback === 'up' ? '👍' : '👎' }}</span>
        <button
          v-if="inlineFilms.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineFilms.length }} films →
        </button>
        <button
          v-else-if="inlineBooks.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineBooks.length }} books →
        </button>
        <button
          v-else-if="inlineTVSeries.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineTVSeries.length }} series →
        </button>
        <button
          v-else-if="inlineImages.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineImages.length }} images →
        </button>
        <button
          v-else-if="inlinePlaces.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlinePlaces.length }} places →
        </button>
        <button
          v-if="inlinePlaces.length > 0 && inlinePlaces.some(p => p.lat != null)"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openMapView(inlinePlaces)"
        >
          View on map →
        </button>
        <button
          v-else-if="inlineSongs.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineSongs.length }} songs →
        </button>
        <button
          v-else-if="inlinePodcasts.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlinePodcasts.length }} podcasts →
        </button>
        <button
          v-else-if="inlineNewsLinks.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineNewsLinks.length }} articles →
        </button>
        <button
          v-else-if="inlineWebsitesLinks.length > 1"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View all {{ inlineWebsitesLinks.length }} websites →
        </button>
        <button
          v-else-if="inlineMagazineSections.length > 0"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openPanel"
        >
          View brief →
        </button>
        <button
          v-if="isLongForm"
          class="text-xs text-accent/70 hover:text-accent transition-colors"
          @click.stop="openLongFormArticle(message.content)"
        >
          Read as article →
        </button>
      </div>
      </div>
    </div>

    <!-- Context menu -->
    <ContextMenu ref="contextMenuRef">
      <ContextMenuItem v-if="isUser" @click="startEditingFromMenu">Edit</ContextMenuItem>
      <ContextMenuItem v-if="!isUser" @click="emitRegenerate">Regenerate</ContextMenuItem>
      <ContextMenuItem @click="emitReply">Reply</ContextMenuItem>
      <ContextMenuItem v-if="!isUser" @click="emitBranch">Branch from here</ContextMenuItem>
      <ContextMenuItem @click="copyContent">Copy</ContextMenuItem>
    </ContextMenu>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, nextTick, watch } from 'vue'
import MarkdownIt from 'markdown-it'
import { hasMath, renderMathInHtml } from '@/composables/useMathRenderer'
import { hasMermaid, renderMermaidBlocks } from '@/composables/useMermaidRenderer'
import ContextMenu from '@/components/ui/ContextMenu.vue'
import ContextMenuItem from '@/components/ui/ContextMenuItem.vue'
import type { Message, WebSearchResult } from '@aiui/core/types/message'
import type { Film, Song, Podcast, Book, TVSeries, ImageItem, Place } from '@aiui/core/types/content'
import { useContentPanel, type MagazineSection } from '@/composables/useContentPanel'
import { useCodeContext } from '@/composables/useCodeContext'
import FilmCard from '@/components/content/FilmCard.vue'
import BookCard from '@/components/content/BookCard.vue'
import TVSeriesCard from '@/components/content/TVSeriesCard.vue'
import SongCard from '@/components/content/SongCard.vue'
import PodcastCard from '@/components/content/PodcastCard.vue'
import PlaceCard from '@/components/content/PlaceCard.vue'
import NewsCard from '@/components/content/NewsCard.vue'
import NostrEmbed from '@/components/chat/NostrEmbed.vue'
import CashuToken from '@/components/chat/CashuToken.vue'
import { extractCashuTokens } from '@/utils/cashu'
import { extractRecipes, extractEvents } from '@/composables/contentExtraction'
import RecipeCard from '@/components/renderers/RecipeCard.vue'
import EventCard from '@/components/renderers/EventCard.vue'
import InteractiveTable from '@/components/renderers/InteractiveTable.vue'
import TimelineRenderer from '@/components/renderers/TimelineRenderer.vue'
import CodeRunner from '@/components/renderers/CodeRunner.vue'
import BitcoinAddressCard from '@/components/renderers/BitcoinAddressCard.vue'
import Bolt11InvoiceCard from '@/components/renderers/Bolt11InvoiceCard.vue'
import Bolt12OfferCard from '@/components/renderers/Bolt12OfferCard.vue'
import MempoolTxCard from '@/components/renderers/MempoolTxCard.vue'
import { detectBitcoinAddresses, detectBolt11Invoices, detectBolt12Offers, detectTxIds } from '@/composables/useBitcoinDetector'
import { extractTables } from '@/composables/useTableExtractor'
import { extractRunnableCodeBlocks } from '@/composables/useCodeBlockExtractor'

const props = withDefaults(
  defineProps<{
    message: Message
    index: number
    triggeringQuery?: string
  }>(),
  { triggeringQuery: '' }
)

const emit = defineEmits<{
  edit: [messageId: string, newContent: string]
  regenerate: []
  branch: [messageId: string]
  reply: [messageId: string, content: string]
  feedback: [messageId: string, value: 'up' | 'down' | undefined]
}>()

function toggleFeedback(value: 'up' | 'down') {
  const newValue = props.message.feedback === value ? undefined : value
  emit('feedback', props.message.id, newValue)
}

// Editing state
const isEditing = ref(false)
const editContent = ref('')
const editTextareaRef = ref<HTMLTextAreaElement | null>(null)

function startEditing() {
  editContent.value = props.message.content
  isEditing.value = true
  nextTick(() => {
    editTextareaRef.value?.focus()
    // Auto-size textarea
    if (editTextareaRef.value) {
      editTextareaRef.value.style.height = 'auto'
      editTextareaRef.value.style.height = editTextareaRef.value.scrollHeight + 'px'
    }
  })
}

function cancelEdit() {
  isEditing.value = false
  editContent.value = ''
}

function submitEdit() {
  const trimmed = editContent.value.trim()
  if (!trimmed) return
  emit('edit', props.message.id, trimmed)
  isEditing.value = false
  editContent.value = ''
}
const { getContextualInlineContent, stripContentTags, stripMarkdownLinks, updatePanelFromText, panelOpen, availableTabs, setActiveTab, openFilmDetail, openBookDetail, openTVSeriesDetail, openImageDetail, openPlaceDetail, openSongDetail, openPodcastDetail, openArticleDetail, openWebsiteDetail, openLongFormArticle, openMapView, closeFilmDetail, closeBookDetail, closeTVSeriesDetail, closeImageDetail, closePlaceDetail, closeSongDetail, closePodcastDetail } = useContentPanel()
const codeContext = useCodeContext()

const isUser = computed(() => props.message.role === 'user')

const inlineContent = computed(() => {
  if (isUser.value) return { films: [] as Film[], books: [] as Book[], tvSeries: [] as TVSeries[], images: [] as ImageItem[], places: [] as Place[], songs: [] as Song[], podcasts: [] as Podcast[], newsLinks: [] as WebSearchResult[], websitesLinks: [] as WebSearchResult[], magazineSections: [] as MagazineSection[] }
  return getContextualInlineContent(props.message.content, props.triggeringQuery, props.message.webResults ?? [])
})

const bubbleClasses = computed(() =>
  isUser.value
    ? 'path-glass-bubble-user rounded-br-md rounded-2xl'
    : 'path-glass-bubble rounded-bl-md rounded-2xl'
)

const inlineFilms = computed(() => inlineContent.value.films)
const inlineBooks = computed(() => inlineContent.value.books ?? [])
const inlineTVSeries = computed(() => inlineContent.value.tvSeries ?? [])
const inlineImages = computed(() => inlineContent.value.images ?? [])
const inlinePlaces = computed(() => inlineContent.value.places ?? [])
const inlineSongs = computed(() => inlineContent.value.songs)
const inlinePodcasts = computed(() => inlineContent.value.podcasts)
const inlineNewsLinks = computed(() => inlineContent.value.newsLinks ?? [])
const inlineWebsitesLinks = computed(() => inlineContent.value.websitesLinks ?? [])
const inlineMagazineSections = computed(() => inlineContent.value.magazineSections ?? [])

const NOSTR_URI_RE = /nostr:(note1[a-z0-9]{58}|npub1[a-z0-9]{58}|nevent1[a-z0-9]+|nprofile1[a-z0-9]+)/g
const nostrUris = computed(() => {
  if (isUser.value) return []
  const matches = props.message.content.match(NOSTR_URI_RE)
  return matches ? [...new Set(matches)] : []
})

const cashuTokens = computed(() => {
  if (isUser.value) return []
  return extractCashuTokens(props.message.content)
})

const inlineRecipes = computed(() => {
  if (isUser.value) return []
  return extractRecipes(props.message.content)
})

const inlineTables = computed(() => {
  if (isUser.value) return []
  return extractTables(props.message.content)
})

const inlineEvents = computed(() => {
  if (isUser.value) return []
  return extractEvents(props.message.content)
})

const runnableCodeBlocks = computed(() => {
  if (isUser.value) return []
  return extractRunnableCodeBlocks(props.message.content)
})

const bitcoinAddresses = computed(() => {
  if (isUser.value) return []
  return detectBitcoinAddresses(props.message.content)
})

const bolt11Invoices = computed(() => {
  if (isUser.value) return []
  return detectBolt11Invoices(props.message.content)
})

const bolt12Offers = computed(() => {
  if (isUser.value) return []
  return detectBolt12Offers(props.message.content)
})

const detectedTxIds = computed(() => {
  if (isUser.value) return []
  return detectTxIds(props.message.content)
})

const isCodeResponse = computed(() =>
  !isUser.value && props.triggeringQuery.trim().toLowerCase() === '/code'
)

const hasContext = computed(() => !isUser.value && (
  isCodeResponse.value ||
  inlineFilms.value.length > 0 ||
  inlineBooks.value.length > 0 ||
  inlineTVSeries.value.length > 0 ||
  inlineImages.value.length > 0 ||
  inlineSongs.value.length > 0 ||
  inlinePodcasts.value.length > 0 ||
  inlineNewsLinks.value.length > 0 ||
  inlineWebsitesLinks.value.length > 0 ||
  inlineMagazineSections.value.length > 0
))

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
})

// Open links in new tab
const defaultRender = md.renderer.rules.link_open || function (tokens, idx, options, _env, self) {
  return self.renderToken(tokens, idx, options)
}
md.renderer.rules.link_open = function (tokens, idx, options, env, self) {
  tokens[idx].attrSet('target', '_blank')
  tokens[idx].attrSet('rel', 'noopener noreferrer')
  return defaultRender(tokens, idx, options, env, self)
}

const displayText = computed(() => {
  if (isUser.value) return props.message.content
  let text = stripContentTags(props.message.content)
  if (inlineNewsLinks.value.length > 0 || inlineWebsitesLinks.value.length > 0) text = stripMarkdownLinks(text)
  if (nostrUris.value.length > 0) text = text.replace(NOSTR_URI_RE, '').replace(/\n{3,}/g, '\n\n').trim()
  if (cashuTokens.value.length > 0) {
    for (const token of cashuTokens.value) {
      text = text.replace(token, '')
    }
    text = text.replace(/\n{3,}/g, '\n\n').trim()
  }
  return text
})

const baseMarkdown = computed(() => md.render(displayText.value))
const renderedMarkdown = ref('')

// Render math/mermaid after markdown, batch on content change
watch(baseMarkdown, async (html) => {
  let result = html
  renderedMarkdown.value = html // Show immediately
  if (hasMath(displayText.value)) {
    result = await renderMathInHtml(result)
  }
  if (hasMermaid(displayText.value)) {
    result = await renderMermaidBlocks(result)
  }
  renderedMarkdown.value = result
}, { immediate: true })

const formattedTime = computed(() => {
  const d = new Date(props.message.timestamp)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
})

// Long-form article detection (>800 words + has headings)
const isLongForm = computed(() => {
  if (isUser.value) return false
  const text = props.message.content
  const words = text.split(/\s+/).length
  const hasHeadings = /^#{2,3}\s+.+$/m.test(text)
  return words > 800 && hasHeadings
})

// Token estimate (~4 chars per token)
const estimatedTokens = computed(() => Math.ceil(props.message.content.length / 4))
const showTokenCount = computed(() => props.message.content.length > 20)
const tokenLabel = computed(() => {
  const t = estimatedTokens.value
  if (t >= 1000) return `${(t / 1000).toFixed(1)}k tok`
  return `${t} tok`
})

function openPanelWithContext() {
  updatePanelFromText(props.message.content, props.triggeringQuery, props.message.webResults ?? [])
}

function handleFilmSelect(film: Film) {
  openPanelWithContext()
  closeSongDetail()
  closePodcastDetail()
  openFilmDetail(film)
}

function handleBookSelect(book: Book) {
  openPanelWithContext()
  closeFilmDetail()
  closeTVSeriesDetail()
  closeSongDetail()
  closePodcastDetail()
  openBookDetail(book)
}

function handleTVSeriesSelect(series: TVSeries) {
  openPanelWithContext()
  closeFilmDetail()
  closeBookDetail()
  closeSongDetail()
  closePodcastDetail()
  openTVSeriesDetail(series)
}

function handleSongSelect(song: Song) {
  openPanelWithContext()
  closeFilmDetail()
  closeBookDetail()
  closePodcastDetail()
  openSongDetail(song)
}

function handlePlaceSelect(place: Place) {
  openPanelWithContext()
  closeFilmDetail()
  closeBookDetail()
  closePlaceDetail()
  openPlaceDetail(place)
}

function handlePodcastSelect(podcast: Podcast) {
  openPanelWithContext()
  closeFilmDetail()
  closeSongDetail()
  openPodcastDetail(podcast)
}

function openPanel() {
  closeFilmDetail()
  closeBookDetail()
  closeTVSeriesDetail()
  closeSongDetail()
  closePodcastDetail()
  openPanelWithContext()
}

function handleArticleSelect(article: WebSearchResult) {
  openPanelWithContext()
  openArticleDetail(article)
}

function handleWebsiteSelect(article: WebSearchResult) {
  openPanelWithContext()
  openWebsiteDetail(article)
}

function activateCodeMode() {
  codeContext.enterCodeMode()
  panelOpen.value = true
  if (!availableTabs.value.includes('code')) {
    availableTabs.value = [...availableTabs.value, 'code']
  }
  setActiveTab('code')
}

function handleBubbleClick() {
  if (isCodeResponse.value) {
    activateCodeMode()
    return
  }
  if (hasContext.value) openPanel()
}

// Context menu
const contextMenuRef = ref<InstanceType<typeof ContextMenu> | null>(null)

function openContextMenu(e: MouseEvent) {
  contextMenuRef.value?.open(e.clientX, e.clientY)
}

function startEditingFromMenu() {
  contextMenuRef.value?.close()
  startEditing()
}

function emitRegenerate() {
  contextMenuRef.value?.close()
  emit('regenerate')
}

function emitReply() {
  contextMenuRef.value?.close()
  emit('reply', props.message.id, props.message.content)
}

function emitBranch() {
  contextMenuRef.value?.close()
  emit('branch', props.message.id)
}

function copyContent() {
  contextMenuRef.value?.close()
  navigator.clipboard.writeText(props.message.content).catch(() => {})
}
</script>
