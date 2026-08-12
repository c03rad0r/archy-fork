import { ref } from 'vue'
import type { Film, Song, Podcast, Book, TVSeries, ImageItem, Place } from '@aiui/core/types/content'
import type { RecipeData } from './contentExtraction'
import type { WebSearchResult } from '@aiui/core/types/message'
import { mockFilms } from '@/mocks/films'
import { mockSongs } from '@/mocks/songs'
import { mockPodcasts } from '@/mocks/podcasts'

// Demo-site content pack (operator decision 2026-08-07): the library views
// fill from mocks in demo/dev builds only; production folds these to empty.
const demoFilms: Film[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockFilms : []
const demoSongs: Song[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockSongs : []
const demoPodcasts: Podcast[] = (import.meta.env.DEV || import.meta.env.VITE_DEMO_CONTENT === 'true') ? mockPodcasts : []
import { fetchRssFromUrls } from '@/composables/useRssFetch'
import {
  extractAllFilms, extractAllSongs, extractAllPodcasts, extractAllBooks,
  extractAllTVSeries, extractAllImages, extractAllPlaces,
  extractMagazineSections, extractMagazineHeroImage,
  extractMarkdownLinks, extractBoldDomainLinks, extractBareDomainLinks, mergeNewsResults,
  extractFilmIds, extractSongIds, extractPodcastIds,
  extractApps, extractCodeBlocks, extractRecipes,
  stripFilmTags, stripSongTags, stripPodcastTags, stripContentTags, stripMarkdownLinks,
} from './contentExtraction'
import type { AppEntry, CodeBlock } from './contentExtraction'
import {
  isNewsQuery, isNewsLikeResponse, isTVQuery,
  isNostrQuery, isNostrLikeResponse,
  isCodeQuery, isCodeLikeResponse,
  isRecipeLikeResponse,
  filterTabsByContext, extractQueryContext,
} from './contentFiltering'
export type { ContentTab, MagazineSection } from './contentFiltering'
import type { ContentTab, MagazineSection } from './contentFiltering'

const panelOpen = ref(false)
const panelFilms = ref<Film[]>([])
const panelBooks = ref<Book[]>([])
const panelTVSeries = ref<TVSeries[]>([])
const panelWebResults = ref<WebSearchResult[]>([])
const panelRssArticles = ref<WebSearchResult[]>([])
const panelWebsites = ref<WebSearchResult[]>([])
const panelMagazineSections = ref<MagazineSection[]>([])
const panelMagazineHeroImage = ref<string | null>(null)
const panelSongs = ref<Song[]>([])
const panelPodcasts = ref<Podcast[]>([])
const panelImages = ref<ImageItem[]>([])
const panelPlaces = ref<Place[]>([])
const panelRecipes = ref<RecipeData[]>([])
const panelApps = ref<AppEntry[]>([])
const panelCodeBlocks = ref<CodeBlock[]>([])
const selectedFilm = ref<Film | null>(null)
const selectedBook = ref<Book | null>(null)
const selectedTVSeries = ref<TVSeries | null>(null)
const selectedSong = ref<Song | null>(null)
const selectedPodcast = ref<Podcast | null>(null)
const selectedArticle = ref<WebSearchResult | null>(null)
const selectedImage = ref<ImageItem | null>(null)
const selectedPlace = ref<Place | null>(null)
const selectedWebsite = ref<WebSearchResult | null>(null)
const selectedRecipe = ref<RecipeData | null>(null)
const selectedApp = ref<AppEntry | null>(null)
const selectedMagazineSection = ref<MagazineSection | null>(null)
const magazineSectionIndex = ref(0)
const panelTitle = ref('Recommended Films')
const panelQuery = ref('')
const panelResponseText = ref('')
const contentType = ref<'film' | 'song' | 'podcast'>('film')
const activeTab = ref<ContentTab>('film')
const availableTabs = ref<ContentTab[]>([])
const selectedDesignSystemItem = ref<DesignSystemItem | null>(null)
const longFormArticle = ref<{ content: string; title?: string } | null>(null)
const pdfUrl = ref<{ url: string; title?: string } | null>(null)
const mapPlaces = ref<Place[]>([])
/** True once `setArchyContent` has populated panelFilms/panelSongs/
 * panelPodcasts from a real node (D-12). While true, `updatePanelFromText`
 * leaves those three buckets alone — the Archy-sourced grids are the
 * source of truth for them, not the model's own reply text. Every other
 * bucket (books, TV, images, places, magazine, code, recipes, news) has no
 * Archy source in this plan's scope and keeps using the regex path
 * unconditionally (13-PATTERNS.md: partial deprecation, not a removal). */
const archyContentActive = ref(false)
/** Which buckets the node's LATEST archy delivery actually filled. The
 * regex-extraction path must never overwrite a bucket the node supplied
 * (D-12 node truth), but a bucket the node left EMPTY stays writable so a
 * recommendation reply's [[film_ext:…]] previews still render as cards —
 * the global latch used to suppress exactly those (the "chat lost its rich
 * previews" regression: once mount-time content latched the flag, no
 * extracted card could ever render again). */
const archySupplied = ref({ film: false, song: false, podcast: false, image: false })
/** A chat turn that may produce content is in flight. Until it resolves,
 * the panel heading must NOT keep advertising the previous query's
 * results — the operator reads that as the answer to what they just
 * asked. `beginArchyContentLoad` raises this and `setArchyContent`
 * lowers it. */
const archyContentLoading = ref(false)

export interface DesignSystemItem {
  id: string
  name: string
  category: 'colors' | 'typography' | 'spacing' | 'atoms' | 'molecules' | 'organisms'
  description: string
  code: string
  preview?: 'inline'
  usedIn?: string
}

export function useContentPanel() {
  function updatePanelFromText(text: string, userQuery = '', webResults: WebSearchResult[] = []) {
    panelQuery.value = userQuery.trim()
    panelResponseText.value = text
    const songs = extractAllSongs(text, userQuery)
    let films = extractAllFilms(text)
    const podcasts = extractAllPodcasts(text)
    const books = extractAllBooks(text, userQuery)
    const tvSeries = extractAllTVSeries(text, userQuery)
    if (tvSeries.length > 0 && isTVQuery(userQuery)) {
      films = films.filter(f => !f.id.startsWith('ext-'))
    }
    const fromMarkdown = extractMarkdownLinks(text)
    const boldDomains = extractBoldDomainLinks(text)
    const bareDomains = extractBareDomainLinks(text)

    panelRssArticles.value = []

    const hasLinkableContent = fromMarkdown.length > 0 || boldDomains.length > 0 || bareDomains.length > 0
    const websitesFromMarkdown = hasLinkableContent ? fromMarkdown : []
    const mergedWebsites = mergeNewsResults(mergeNewsResults(websitesFromMarkdown, boldDomains), bareDomains)
    const hasWebsites = mergedWebsites.length > 0

    // App extraction
    const apps = extractApps(text, userQuery)
    const hasApps = apps.length > 0

    // Code block extraction
    const codeBlocks = extractCodeBlocks(text)
    const hasCode = codeBlocks.length > 0 && (isCodeQuery(userQuery) || isCodeLikeResponse(text))

    // Nostr detection
    const hasNostr = isNostrQuery(userQuery) || isNostrLikeResponse(text)

    const images = extractAllImages(text, userQuery)
    const places = extractAllPlaces(text, userQuery)
    const recipes = extractRecipes(text)
    const hasRecipes = recipes.length > 0 || isRecipeLikeResponse(text)

    // Magazine = bullet-style sections (- **Title**: Content)
    const magazineSections = extractMagazineSections(text)
    const hasAnyOtherContent = films.length > 0 || songs.length > 0 || podcasts.length > 0 ||
      books.length > 0 || tvSeries.length > 0 || images.length > 0 || places.length > 0
    const hasMagazine = magazineSections.length >= 1 && (
      isNewsQuery(userQuery) || isNewsLikeResponse(text) ||
      /sentiment|bearish|bull case|macro|%|BTC|bitcoin|BIP|protocol|debate|what'?s happening|ETF|inflow|trading at|key developments|price recovery|institutional|analyst watch|market cap/i.test(text) ||
      (!hasAnyOtherContent && !hasWebsites && magazineSections.length >= 2)
    )

    // News = actual articles (web search + RSS from website domains)
    const newsContext = isNewsQuery(userQuery) || isNewsLikeResponse(text)
    // Show news tab eagerly when query is news-like — results may arrive async
    const hasNews = newsContext && (webResults.length > 0 || mergedWebsites.length > 0 || isNewsQuery(userQuery))
    const mergedNews = mergeNewsResults(webResults, panelRssArticles.value)

    if (mergedWebsites.length > 0 && newsContext) {
      const urls = mergedWebsites.map((w) => w.url)
      fetchRssFromUrls(urls).then((articles) => {
        if (articles.length === 0) return
        panelRssArticles.value = articles
        const combined = mergeNewsResults(panelWebResults.value, articles)
        panelWebResults.value = combined
        if (!availableTabs.value.includes('news')) {
          // Insert before 'prompt' (which is always last)
          const promptIdx = availableTabs.value.indexOf('prompt')
          if (promptIdx >= 0) {
            availableTabs.value = [...availableTabs.value.slice(0, promptIdx), 'news', ...availableTabs.value.slice(promptIdx)]
          } else {
            availableTabs.value = ['news', ...availableTabs.value]
          }
          activeTab.value = 'news'
        }
        const ctx = extractQueryContext(panelQuery.value)
        panelTitle.value = ctx ? `${ctx} — ${combined.length} articles` : `${combined.length} Articles`
      })
    }

    const tabs = filterTabsByContext(userQuery, films.length > 0, songs.length > 0, podcasts.length > 0, books.length > 0, tvSeries.length > 0, images.length > 0, places.length > 0, hasNews, hasWebsites, hasMagazine, hasNostr, hasApps, hasCode, hasRecipes)
    // Real node content outranks anything inferred from the reply text.
    //
    // `setArchyContent` runs first and puts the node's films/songs/podcasts/
    // images on the tab bar; this function then ran and REPLACED the bar with
    // its own regex-derived tabs. Asking "show me my own shared content"
    // therefore ended on an "AI Brief" — a prose restatement — while the
    // populated image grid was no longer reachable. Guarding the panel*
    // arrays (above) was not enough: the arrays held the right data and the
    // tab bar had thrown away the way to see it.
    // Ordered by how much the node actually returned, not by a fixed type
    // preference: "show me my own shared content" on a node with 13 photos
    // and 2 tracks opened on Songs, so the answer's own subject was one
    // click away and the heading read "2 Songs" for a 15-item reply.
    const archyTabs: ContentTab[] = archyContentActive.value
      ? ([
          ['film', panelFilms.value.length],
          ['song', panelSongs.value.length],
          ['podcast', panelPodcasts.value.length],
          ['image', panelImages.value.length],
        ] as [ContentTab, number][])
          .filter(([, n]) => n > 0)
          .sort((a, b) => b[1] - a[1])
          .map(([t]) => t)
      : []
    const inferredTabs = tabs.filter((t) => !archyTabs.includes(t))
    const orderedTabs = [...archyTabs, ...inferredTabs]
    // Always append Prompt tab as the rightmost tab
    const tabsWithPrompt = orderedTabs.length > 0 ? [...orderedTabs, 'prompt' as const] : ['prompt' as const]
    availableTabs.value = tabsWithPrompt
    activeTab.value = orderedTabs[0] ?? 'prompt'

    const showFilms = tabs.includes('film')
    const showBooks = tabs.includes('book')
    const showTVSeries = tabs.includes('tvshow')
    const showImages = tabs.includes('image')
    const showPlaces = tabs.includes('place')
    const showSongs = tabs.includes('song')
    const showPodcasts = tabs.includes('podcast')
    const showNews = tabs.includes('news')
    const showWebsitesTab = tabs.includes('websites')
    const showMagazine = tabs.includes('magazine')
    const showRecipes = tabs.includes('recipe')
    const showApps = tabs.includes('app')
    const showCode = tabs.includes('code')

    const visibleFilms = showFilms ? films : []
    const visibleBooks = showBooks ? books : []
    const visibleTVSeries = showTVSeries ? tvSeries : []
    const visibleImages = showImages ? images : []
    const visiblePlaces = showPlaces ? places : []
    const visibleSongs = showSongs ? songs : []
    const visiblePodcasts = showPodcasts ? podcasts : []
    const visibleNews = showNews ? mergedNews : []
    const visibleWebsites = showWebsitesTab ? mergedWebsites : []
    const visibleMagazineSections = showMagazine ? magazineSections : []
    const visibleRecipes = showRecipes ? recipes : []
    const visibleApps = showApps ? apps : []
    const visibleCodeBlocks = showCode ? codeBlocks : []

    // Archy-sourced films/songs/podcasts/images are the source of truth
    // once a node has supplied them (D-12) — don't let this turn's regex
    // extraction of the model's own reply text overwrite them. Images
    // joined this set when the adapter started carrying shared photos;
    // leaving them out here would have let the regex path immediately
    // wipe the grid the node had just filled.
    // Per-bucket, not global: the node's truth wins a bucket it filled;
    // an empty bucket stays open to this turn's extracted previews.
    if (!archySupplied.value.film) panelFilms.value = visibleFilms
    if (!archySupplied.value.song) panelSongs.value = visibleSongs
    if (!archySupplied.value.podcast) panelPodcasts.value = visiblePodcasts
    if (!archySupplied.value.image) panelImages.value = visibleImages
    panelBooks.value = visibleBooks
    panelTVSeries.value = visibleTVSeries
    panelPlaces.value = visiblePlaces
    panelWebResults.value = visibleNews
    panelWebsites.value = visibleWebsites
    panelRecipes.value = visibleRecipes
    panelApps.value = visibleApps
    panelCodeBlocks.value = visibleCodeBlocks
    panelMagazineSections.value = visibleMagazineSections
    panelMagazineHeroImage.value = showMagazine
      ? (extractMagazineHeroImage(text) ?? webResults[0]?.imgSrc ?? null)
      : null
    selectedFilm.value = null
    selectedBook.value = null
    selectedTVSeries.value = null
    selectedImage.value = null
    selectedPlace.value = null
    selectedSong.value = null
    selectedPodcast.value = null
    selectedArticle.value = null
    selectedRecipe.value = null
    selectedApp.value = null
    selectedWebsite.value = null
    selectedMagazineSection.value = null

    if (visibleFilms.length > 0) contentType.value = 'film'
    else if (visibleBooks.length > 0) contentType.value = 'film'
    else if (visibleTVSeries.length > 0) contentType.value = 'film'
    else if (visibleSongs.length > 0) contentType.value = 'song'
    else if (visiblePodcasts.length > 0) contentType.value = 'podcast'
    else if (visibleNews.length > 0) contentType.value = 'film'
    else contentType.value = 'film'

    // Title follows the primary (first) tab — which, when the node supplied
    // content, is an Archy tab. Titling from `tabs[0]` (the regex path's
    // own first tab) would name a grid that is no longer the one on screen.
    const primary = orderedTabs[0]
    if (archyTabs.includes(primary as ContentTab)) {
      if (primary === 'image') panelTitle.value = `${panelImages.value.length} Images`
      else if (primary === 'film') {
        panelTitle.value = panelFilms.value.length === 1
          ? panelFilms.value[0]!.title
          : `${panelFilms.value.length} Films`
      } else if (primary === 'song') {
        panelTitle.value = panelSongs.value.length === 1
          ? panelSongs.value[0]!.title
          : `${panelSongs.value.length} Songs`
      } else {
        panelTitle.value = panelPodcasts.value.length === 1
          ? panelPodcasts.value[0]!.title
          : `${panelPodcasts.value.length} Podcasts`
      }
    } else if (primary === 'place' && visiblePlaces.length > 0) {
      panelTitle.value = visiblePlaces.length === 1 ? visiblePlaces[0].name : `${visiblePlaces.length} Places`
    } else if (primary === 'tvshow' && visibleTVSeries.length > 0) {
      panelTitle.value = visibleTVSeries.length === 1 ? visibleTVSeries[0].title : `${visibleTVSeries.length} TV Series`
    } else if (primary === 'book' && visibleBooks.length > 0) {
      panelTitle.value = visibleBooks.length === 1 ? visibleBooks[0].title : `${visibleBooks.length} Books`
    } else if (primary === 'image' && visibleImages.length > 0) {
      panelTitle.value = `${visibleImages.length} Images`
    } else if (visibleFilms.length === 1) panelTitle.value = visibleFilms[0].title
    else if (visibleFilms.length > 1) panelTitle.value = `${visibleFilms.length} Films`
    else if (visibleBooks.length === 1) panelTitle.value = visibleBooks[0].title
    else if (visibleBooks.length > 1) panelTitle.value = `${visibleBooks.length} Books`
    else if (visibleTVSeries.length === 1) panelTitle.value = visibleTVSeries[0].title
    else if (visibleTVSeries.length > 1) panelTitle.value = `${visibleTVSeries.length} TV Series`
    else if (visibleSongs.length === 1) panelTitle.value = visibleSongs[0].title
    else if (visibleSongs.length > 1) panelTitle.value = `${visibleSongs.length} Songs`
    else if (visiblePlaces.length === 1) panelTitle.value = visiblePlaces[0].name
    else if (visiblePlaces.length > 1) panelTitle.value = `${visiblePlaces.length} Places`
    else if (visiblePodcasts.length === 1) panelTitle.value = visiblePodcasts[0].title
    else if (visiblePodcasts.length > 1) panelTitle.value = `${visiblePodcasts.length} Podcasts`
    else if (visibleNews.length > 0) {
      const ctx = extractQueryContext(userQuery)
      panelTitle.value = ctx ? `${ctx} — ${visibleNews.length} articles` : `${visibleNews.length} Articles`
    }
    else if (visibleMagazineSections.length > 0) {
      const ctx = extractQueryContext(userQuery)
      panelTitle.value = ctx ? `${ctx} — Brief` : 'AI Brief'
    }
    else if (visibleRecipes.length === 1) panelTitle.value = visibleRecipes[0].title
    else if (visibleRecipes.length > 1) panelTitle.value = `${visibleRecipes.length} Recipes`
    else if (visibleCodeBlocks.length > 0) panelTitle.value = `${visibleCodeBlocks.length} Code Blocks`
    else if (visibleApps.length > 0) panelTitle.value = `${visibleApps.length} Apps`
    else if (visibleWebsites.length > 0) panelTitle.value = `${visibleWebsites.length} Websites`
    else if (hasNostr) panelTitle.value = 'Nostr'
    // Zero extraction tabs: leave the title alone when an archy delivery
    // just said 'Nothing found' (or is still 'Loading…') — the closed
    // panel's heading must not silently become 'Content'.
    else if (panelTitle.value !== 'Nothing found' && panelTitle.value !== 'Loading…') panelTitle.value = 'Content'

    // Open from the ORDERED tab list (Archy tabs + inferred), not the
    // regex-only `tabs`: a node-supplied grid whose reply text happens to
    // match no extraction pattern (a plain markdown list of purchased
    // files) used to CLOSE the panel setArchyContent had just opened —
    // the probe showed surfaces=1 images=3 arriving and the user saw prose.
    panelOpen.value = orderedTabs.length > 0
  }

  /**
   * Populate the film/song/podcast grids directly from a node's real
   * content (D-12) — `useArchy.ts`'s `requestArchyContent` calls this from
   * its `content:push` handler. Bypasses `updatePanelFromText`'s regex
   * path entirely for these three buckets and marks `archyContentActive`
   * so a later `updatePanelFromText` call (from an unrelated chat turn)
   * does not clobber them back to a regex-scraped or empty state.
   *
   * TMDB posters, web search and RSS remain unavailable on a node (their
   * Vite plugins are dev-server-only — 13-CONTEXT.md landmines), so a
   * `Film`/`Song` adapted from peer/own-node data has no `posterUrl`/
   * `coverUrl`; `FilmGrid`/`SongGrid` already render their existing
   * no-artwork fallback for that case (unchanged by this plan, D-12).
   */
  /** A content-capable chat turn just started. Clears the stale heading
   * so the panel says what it is doing rather than what it last found. */
  function beginArchyContentLoad() {
    archyContentLoading.value = true
    panelTitle.value = 'Loading…'
  }

  function setArchyContent(bundle: {
    films?: Film[]
    songs?: Song[]
    podcasts?: Podcast[]
    images?: ImageItem[]
  }) {
    panelFilms.value = bundle.films ?? []
    panelSongs.value = bundle.songs ?? []
    panelPodcasts.value = bundle.podcasts ?? []
    panelImages.value = bundle.images ?? []
    archySupplied.value = {
      film: panelFilms.value.length > 0,
      song: panelSongs.value.length > 0,
      podcast: panelPodcasts.value.length > 0,
      image: panelImages.value.length > 0,
    }
    // Latch only on a non-empty delivery: an empty tool result must not
    // suppress the recommendation previews extraction can still produce.
    archyContentActive.value =
      archySupplied.value.film || archySupplied.value.song ||
      archySupplied.value.podcast || archySupplied.value.image

    // 13-11 (GAP-FOUND 2026-08-03): `availableTabs`/`activeTab`/`panelOpen`
    // were previously untouched here — only `updatePanelFromText`'s regex
    // path ever set them, so real Archy-sourced content could sit fully
    // populated in these three refs while the tab bar and grid stayed
    // whatever the last (or no) chat turn left them: closed, or showing
    // only 'prompt' (13-06's own Known Limitations, this plan's must_haves
    // GAP-FOUND). Only touches these three refs when Archy actually
    // supplied non-empty content — an empty/never-granted library must not
    // force the panel open on every mount.
    // Largest bucket first — same reasoning as updatePanelFromText's copy:
    // the tab that opens should be the one holding most of the answer.
    const archyTabs: ContentTab[] = ([
      ['film', panelFilms.value.length],
      ['song', panelSongs.value.length],
      ['podcast', panelPodcasts.value.length],
      ['image', panelImages.value.length],
    ] as [ContentTab, number][])
      .filter(([, n]) => n > 0)
      .sort((a, b) => b[1] - a[1])
      .map(([t]) => t)
    if (archyTabs.length > 0) {
      availableTabs.value = [...archyTabs, 'prompt']
      if (!archyTabs.includes(activeTab.value)) activeTab.value = archyTabs[0]!
      const lead = archyTabs[0]!
      if (lead === 'image') panelTitle.value = `${panelImages.value.length} Images`
      else if (lead === 'film') {
        panelTitle.value = panelFilms.value.length === 1
          ? panelFilms.value[0]!.title : `${panelFilms.value.length} Films`
      } else if (lead === 'song') {
        panelTitle.value = panelSongs.value.length === 1
          ? panelSongs.value[0]!.title : `${panelSongs.value.length} Songs`
      } else {
        panelTitle.value = panelPodcasts.value.length === 1
          ? panelPodcasts.value[0]!.title : `${panelPodcasts.value.length} Podcasts`
      }
      panelOpen.value = true
    } else if (archyContentLoading.value) {
      // The turn asked the node for content and got none back. Leaving the
      // previous query's heading up would claim those results answer THIS
      // question; say plainly that there was nothing.
      panelTitle.value = 'Nothing found'
    }
    archyContentLoading.value = false
  }

  function setActiveTab(tab: ContentTab) {
    if (availableTabs.value.includes(tab)) activeTab.value = tab
  }

  /** Contextual content for inline cards (prompt index badges) */
  function getContextualInlineContent(text: string, userQuery: string, webResults: WebSearchResult[] = []) {
    let films = extractAllFilms(text)
    const songs = extractAllSongs(text, userQuery)
    const podcasts = extractAllPodcasts(text)
    const books = extractAllBooks(text, userQuery)
    const tvSeries = extractAllTVSeries(text, userQuery)
    if (tvSeries.length > 0 && isTVQuery(userQuery)) {
      films = films.filter(f => !f.id.startsWith('ext-'))
    }
    const magazineSections = extractMagazineSections(text)
    const fromMarkdown = extractMarkdownLinks(text)
    const boldDomains = extractBoldDomainLinks(text)
    const bareDomains = extractBareDomainLinks(text)
    const hasNews = webResults.length > 0 && (isNewsQuery(userQuery) || isNewsLikeResponse(text))
    const newsLinks = hasNews ? webResults : []
    const hasLinkableContent = fromMarkdown.length > 0 || boldDomains.length > 0 || bareDomains.length > 0
    const websitesFromMd = hasLinkableContent ? fromMarkdown : []
    const websitesLinks = mergeNewsResults(mergeNewsResults(websitesFromMd, boldDomains), bareDomains)
    const hasWebsites = websitesLinks.length > 0
    const apps = extractApps(text, userQuery)
    const hasApps = apps.length > 0
    const codeBlocks = extractCodeBlocks(text)
    const hasCode = codeBlocks.length > 0 && (isCodeQuery(userQuery) || isCodeLikeResponse(text))
    const hasNostr = isNostrQuery(userQuery) || isNostrLikeResponse(text)
    const images = extractAllImages(text, userQuery)
    const places = extractAllPlaces(text, userQuery)
    const inlineRecipes = extractRecipes(text)
    const hasRecipes = inlineRecipes.length > 0 || isRecipeLikeResponse(text)
    const hasAnyOtherInline = films.length > 0 || songs.length > 0 || podcasts.length > 0 ||
      books.length > 0 || tvSeries.length > 0 || images.length > 0 || places.length > 0
    const hasMagazine = magazineSections.length >= 1 && (
      isNewsQuery(userQuery) || isNewsLikeResponse(text) ||
      /sentiment|bearish|bull case|macro|%|BTC|bitcoin|BIP|protocol|debate|what'?s happening|ETF|inflow|trading at|key developments|price recovery|institutional|analyst watch|market cap/i.test(text) ||
      (!hasAnyOtherInline && !hasWebsites && magazineSections.length >= 2)
    )
    const tabs = filterTabsByContext(userQuery, films.length > 0, songs.length > 0, podcasts.length > 0, books.length > 0, tvSeries.length > 0, images.length > 0, places.length > 0, hasNews, hasWebsites, hasMagazine, hasNostr, hasApps, hasCode, hasRecipes)
    return {
      films: tabs.includes('film') ? films : [],
      books: tabs.includes('book') ? books : [],
      tvSeries: tabs.includes('tvshow') ? tvSeries : [],
      images: tabs.includes('image') ? images : [],
      places: tabs.includes('place') ? places : [],
      songs: tabs.includes('song') ? songs : [],
      podcasts: tabs.includes('podcast') ? podcasts : [],
      newsLinks: tabs.includes('news') ? newsLinks : [],
      websitesLinks: tabs.includes('websites') ? websitesLinks : [],
      magazineSections: tabs.includes('magazine') ? magazineSections : [],
      apps: tabs.includes('app') ? apps : [],
      codeBlocks: tabs.includes('code') ? codeBlocks : [],
      hasNostr,
    }
  }

  // ─── Detail open/close ────────────────────────────────────────

  function clearAllSelections() {
    selectedFilm.value = null
    selectedBook.value = null
    selectedTVSeries.value = null
    selectedImage.value = null
    selectedPlace.value = null
    selectedSong.value = null
    selectedPodcast.value = null
    selectedArticle.value = null
    selectedRecipe.value = null
    selectedApp.value = null
    selectedWebsite.value = null
    selectedMagazineSection.value = null
    selectedDesignSystemItem.value = null
    longFormArticle.value = null
    pdfUrl.value = null
    mapPlaces.value = []
  }

  function openFilmDetail(film: Film) { clearAllSelections(); selectedFilm.value = film }
  function closeFilmDetail() { selectedFilm.value = null }

  function openBookDetail(book: Book) { clearAllSelections(); selectedBook.value = book }
  function closeBookDetail() { selectedBook.value = null }

  function openSongDetail(song: Song) { clearAllSelections(); selectedSong.value = song }
  function closeSongDetail() { selectedSong.value = null }

  function openPodcastDetail(podcast: Podcast) { clearAllSelections(); selectedPodcast.value = podcast }
  function closePodcastDetail() { selectedPodcast.value = null }

  function openArticleDetail(article: WebSearchResult) { clearAllSelections(); selectedArticle.value = article; panelOpen.value = true }
  function closeArticleDetail() { selectedArticle.value = null }

  function openWebsiteDetail(website: WebSearchResult) { clearAllSelections(); selectedWebsite.value = website; panelOpen.value = true }
  function closeWebsiteDetail() { selectedWebsite.value = null }

  function openLongFormArticle(content: string, title?: string) { clearAllSelections(); longFormArticle.value = { content, title }; panelOpen.value = true }
  function closeLongFormArticle() { longFormArticle.value = null }

  function openPdfViewer(url: string, title?: string) { clearAllSelections(); pdfUrl.value = { url, title }; panelOpen.value = true }
  function closePdfViewer() { pdfUrl.value = null }

  function openMapView(places: Place[]) { clearAllSelections(); mapPlaces.value = places; panelOpen.value = true }
  function closeMapView() { mapPlaces.value = [] }

  function openMagazineSectionDetail(section: MagazineSection, index: number) {
    clearAllSelections()
    selectedMagazineSection.value = section
    magazineSectionIndex.value = index
  }
  function closeMagazineSectionDetail() { selectedMagazineSection.value = null }

  function navigateMagazineSection(direction: 'prev' | 'next') {
    const sections = panelMagazineSections.value
    if (!sections.length) return
    let idx = magazineSectionIndex.value
    idx += direction === 'next' ? 1 : -1
    if (idx < 0) idx = sections.length - 1
    if (idx >= sections.length) idx = 0
    magazineSectionIndex.value = idx
    selectedMagazineSection.value = sections[idx]
  }

  function openTVSeriesDetail(series: TVSeries) { clearAllSelections(); selectedTVSeries.value = series }
  function closeTVSeriesDetail() { selectedTVSeries.value = null }

  function openImageDetail(image: ImageItem) { clearAllSelections(); selectedImage.value = image }
  function closeImageDetail() { selectedImage.value = null }

  function openPlaceDetail(place: Place) { clearAllSelections(); selectedPlace.value = place }
  function closePlaceDetail() { selectedPlace.value = null }

  function openRecipeDetail(recipe: RecipeData) { clearAllSelections(); selectedRecipe.value = recipe }
  function closeRecipeDetail() { selectedRecipe.value = null }

  function openAppDetail(app: AppEntry) { clearAllSelections(); selectedApp.value = app }
  function closeAppDetail() { selectedApp.value = null }

  function openDesignSystemItem(item: DesignSystemItem) { clearAllSelections(); selectedDesignSystemItem.value = item }
  function closeDesignSystemItem() { selectedDesignSystemItem.value = null }

  function enterDesignSystemMode() {
    panelOpen.value = true
    activeTab.value = 'design-system'
    availableTabs.value = ['design-system']
    panelTitle.value = 'Design System'
    clearAllSelections()
  }

  function closePanel() {
    panelOpen.value = false
    clearAllSelections()
    activeTab.value = 'film'
    availableTabs.value = []
  }

  function showAllFilms() {
    panelFilms.value = [...demoFilms]
    panelSongs.value = []
    panelPodcasts.value = []
    panelTitle.value = 'Your Film Library'
    contentType.value = 'film'
    panelOpen.value = true
    clearAllSelections()
  }

  function showAllSongs() {
    panelFilms.value = []
    panelSongs.value = [...demoSongs]
    panelPodcasts.value = []
    panelTitle.value = 'Your Song Library'
    contentType.value = 'song'
    panelOpen.value = true
    clearAllSelections()
  }

  function showAllPodcasts() {
    panelFilms.value = []
    panelSongs.value = []
    panelPodcasts.value = [...demoPodcasts]
    panelTitle.value = 'Your Podcast Library'
    contentType.value = 'podcast'
    panelOpen.value = true
    clearAllSelections()
  }

  return {
    panelOpen,
    panelFilms,
    panelBooks,
    panelTVSeries,
    panelImages,
    panelPlaces,
    panelSongs,
    panelPodcasts,
    archyContentActive,
    archyContentLoading,
    beginArchyContentLoad,
    setArchyContent,
    panelWebResults,
    panelWebsites,
    panelRecipes,
    panelApps,
    panelCodeBlocks,
    panelMagazineSections,
    panelMagazineHeroImage,
    selectedFilm,
    selectedBook,
    selectedTVSeries,
    selectedImage,
    selectedPlace,
    selectedSong,
    selectedPodcast,
    selectedArticle,
    selectedRecipe,
    selectedApp,
    selectedWebsite,
    selectedMagazineSection,
    magazineSectionIndex,
    panelTitle,
    panelQuery,
    panelResponseText,
    contentType,
    activeTab,
    availableTabs,
    setActiveTab,
    extractFilmIds,
    extractAllFilms,
    extractAllBooks,
    extractAllTVSeries,
    extractSongIds,
    extractAllSongs,
    extractPodcastIds,
    extractAllPodcasts,
    extractCodeBlocks,
    getContextualInlineContent,
    updatePanelFromText,
    stripFilmTags,
    stripSongTags,
    stripPodcastTags,
    stripContentTags,
    stripMarkdownLinks,
    openFilmDetail,
    closeFilmDetail,
    openBookDetail,
    closeBookDetail,
    openSongDetail,
    closeSongDetail,
    openPodcastDetail,
    closePodcastDetail,
    openArticleDetail,
    closeArticleDetail,
    openWebsiteDetail,
    closeWebsiteDetail,
    openRecipeDetail,
    closeRecipeDetail,
    openAppDetail,
    closeAppDetail,
    openMagazineSectionDetail,
    closeMagazineSectionDetail,
    navigateMagazineSection,
    openTVSeriesDetail,
    closeTVSeriesDetail,
    openImageDetail,
    closeImageDetail,
    openPlaceDetail,
    closePlaceDetail,
    selectedDesignSystemItem,
    openDesignSystemItem,
    closeDesignSystemItem,
    longFormArticle,
    openLongFormArticle,
    closeLongFormArticle,
    pdfUrl,
    openPdfViewer,
    closePdfViewer,
    mapPlaces,
    openMapView,
    closeMapView,
    enterDesignSystemMode,
    closePanel,
    showAllFilms,
    showAllSongs,
    showAllPodcasts,
  }
}
