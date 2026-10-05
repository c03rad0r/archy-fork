<template>
  <div class="discover-container">
    <!-- Navigation Bar (always at top) -->
    <div>
      <!-- Desktop: tabs + categories + search -->
      <div ref="discoverHeaderRef" class="app-header-desktop mb-6 items-center gap-4 relative">
        <div ref="discoverPrimaryRef" class="flex-shrink-0">
          <div class="mode-switcher hidden md:inline-flex">
            <RouterLink to="/dashboard/apps" class="mode-switcher-btn">My Apps</RouterLink>
            <RouterLink to="/dashboard/discover" class="mode-switcher-btn mode-switcher-btn-active">App Store</RouterLink>
            <RouterLink to="/dashboard/apps?tab=services" class="mode-switcher-btn">Services</RouterLink>
          </div>
        </div>
        <div v-show="!collapseCategories" class="mode-switcher category-tabs-wide hidden md:inline-flex">
          <button
            v-for="section in appStoreSections"
            :key="section.id"
            @click="selectDiscoverCategory(section.id)"
            class="mode-switcher-btn"
            :class="{ 'mode-switcher-btn-active': section.id === 'discover' }"
          >
            {{ section.name }}
          </button>
        </div>
        <div v-show="collapseCategories" class="segmented-select flex-shrink-0">
          <label class="sr-only" for="discover-category-select">App Store category</label>
          <select
            id="discover-category-select"
            class="segmented-select-control"
            value="discover"
            @change="selectDiscoverCategory(($event.target as HTMLSelectElement).value)"
          >
            <option
              v-for="section in appStoreSections"
              :key="section.id"
              :value="section.id"
            >
              {{ section.name }}
            </option>
          </select>
        </div>
        <div ref="discoverCategoryProbeRef" class="mode-switcher category-tabs-probe" aria-hidden="true">
          <button
            v-for="section in appStoreSections"
            :key="section.id"
            class="mode-switcher-btn"
            type="button"
          >
            {{ section.name }}
          </button>
        </div>
        <AppSearchField v-model="searchQuery" placeholder="Search apps..." label="Search apps" />
        <RefreshIndicator :state="catalogResource.entry.loadState" label="Refreshing app store catalog" />
      </div>

      <!-- Mobile: categories + search -->
      <div class="app-header-mobile mb-4">
        <div class="app-header-inline-tabs mode-switcher mode-switcher-full mb-3">
          <RouterLink to="/dashboard/apps" class="mode-switcher-btn">My Apps</RouterLink>
          <RouterLink to="/dashboard/discover" class="mode-switcher-btn mode-switcher-btn-active">App Store</RouterLink>
          <RouterLink to="/dashboard/apps?tab=services" class="mode-switcher-btn">Services</RouterLink>
        </div>
        <div class="flex items-center gap-2 mb-3">
          <span class="discover-terminal-tag">discover</span>
          <h1 class="text-lg font-bold text-white">App Store</h1>
          <RefreshIndicator :state="catalogResource.entry.loadState" label="Refreshing app store catalog" />
        </div>
        <div class="mobile-category-strip mb-3" aria-label="App Store categories">
          <button
            v-for="section in appStoreSections"
            :key="section.id"
            @click="selectDiscoverCategory(section.id)"
            class="mobile-category-pill"
            :class="{ 'mobile-category-pill-active': section.id === 'discover' }"
            type="button"
          >{{ section.name }}</button>
        </div>
        <AppSearchField v-model="searchQuery" placeholder="Search apps..." label="Search apps" />
      </div>
    </div>

    <!-- Hero + registry-owned storefront (only when no search) -->
    <template v-if="!searchQuery">
      <DiscoverHero
        :total-apps="allApps.length"
        :installed-count="installedCount"
      />

      <!-- A registry storefront extends the featured story; it does not
           replace it. Keep the primary banner above Popular, then place
           registry promotions after the popular rows. -->
      <div
        v-if="catalogStorefront && featuredBanner"
        class="featured-banner glass-card mb-8 relative overflow-hidden cursor-pointer"
        @click="featuredBannerApp && viewAppDetails(featuredBannerApp)"
      >
        <img
          :src="featuredBanner.banner"
          :alt="featuredBanner.headline"
          class="featured-banner-img"
          @error="(e: Event) => (e.target as HTMLImageElement).style.display = 'none'"
        />
        <div class="featured-banner-overlay">
          <div class="flex items-center gap-3 mb-2">
            <span class="discover-terminal-tag">featured</span>
            <span class="text-white/50 text-sm font-mono">{{ featuredBanner.tag }}</span>
          </div>
          <h2 class="text-3xl md:text-4xl font-extrabold text-white mb-2 tracking-tight">{{ featuredBanner.headline }}</h2>
          <p class="text-white/80 text-base md:text-lg max-w-2xl leading-relaxed mb-4">{{ featuredBanner.description }}</p>
          <div class="flex items-center gap-3">
            <button
              v-if="featuredBannerApp && isInstalled(featuredBannerApp.id) && !isStartingUp(featuredBannerApp.id)"
              @click.stop="launchInstalledApp(featuredBannerApp)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium"
            >Launch</button>
            <button
              v-else-if="featuredBannerApp && !isInstalled(featuredBannerApp.id) && featuredBannerApp.dockerImage"
              @click.stop="handleInstall(featuredBannerApp)"
              :disabled="installingApps.has(featuredBannerApp.id)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium disabled:opacity-50"
            >
              <span v-if="installingApps.has(featuredBannerApp.id)">Installing...</span>
              <span v-else>Install</span>
            </button>
            <span class="text-white/40 text-sm">{{ featuredBannerApp?.title }} {{ $ver(featuredBannerApp?.version) }}</span>
          </div>
        </div>
      </div>

      <!-- New catalogs own the store composition. Six popular cards make two
           rows on desktop; registry operators can reorder them without an OS
           release. Older catalogs retain the legacy featured fallback. -->
      <div v-if="catalogStorefront" class="flex items-center gap-3 mb-5">
        <span class="discover-terminal-tag">popular</span>
        <h2 class="text-xl font-bold text-white">Popular Apps</h2>
        <div class="flex-1 h-px bg-white/10"></div>
        <span class="text-white/30 text-sm">{{ popularApps.length }} apps</span>
      </div>

      <AppGrid
        v-if="catalogStorefront"
        :filtered-apps="popularApps"
        :show-stagger="showStagger"
        :stagger-offset="0"
        :containers-scanned="containersScanned"
        :installing-apps="installingApps"
        :is-installed="isInstalled"
        :is-starting-up="isStartingUp"
        :get-installed-state="getInstalledState"
        :get-app-tier="getAppTier"
        :is-loading="loadingCommunity"
        loading-message="Loading..."
        :nostr-error="''"
        :is-nostr-category="false"
        :search-query="''"
        @view-details="viewAppDetails"
        @launch="launchInstalledApp"
        @install="handleInstall"
      />

      <FeaturedApps
        v-else
        :featured-apps="featuredApps"
        :show-stagger="showStagger"
        :containers-scanned="containersScanned"
        :installing-apps="installingApps"
        :is-installed="isInstalled"
        :is-starting-up="isStartingUp"
        :get-app-tier="getAppTier"
        @view-details="viewAppDetails"
        @launch="launchInstalledApp"
        @install="handleInstall"
      />

      <!-- Featured App Banner (from catalog or hardcoded) -->
      <div
        v-if="!catalogStorefront && featuredBanner"
        class="featured-banner glass-card mb-8 relative overflow-hidden cursor-pointer"
        @click="featuredBannerApp && viewAppDetails(featuredBannerApp)"
      >
        <img
          :src="featuredBanner.banner"
          :alt="featuredBanner.headline"
          class="featured-banner-img"
          @error="(e: Event) => (e.target as HTMLImageElement).style.display = 'none'"
        />
        <div class="featured-banner-overlay">
          <div class="flex items-center gap-3 mb-2">
            <span class="discover-terminal-tag">featured</span>
            <span class="text-white/50 text-sm font-mono">{{ featuredBanner.tag }}</span>
          </div>
          <h2 class="text-3xl md:text-4xl font-extrabold text-white mb-2 tracking-tight">{{ featuredBanner.headline }}</h2>
          <p class="text-white/80 text-base md:text-lg max-w-2xl leading-relaxed mb-4">{{ featuredBanner.description }}</p>
          <div class="flex items-center gap-3">
            <button
              v-if="featuredBannerApp && isInstalled(featuredBannerApp.id) && !isStartingUp(featuredBannerApp.id)"
              @click.stop="launchInstalledApp(featuredBannerApp)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium"
            >Launch</button>
            <button
              v-else-if="featuredBannerApp && !isInstalled(featuredBannerApp.id) && featuredBannerApp.dockerImage"
              @click.stop="handleInstall(featuredBannerApp)"
              :disabled="installingApps.has(featuredBannerApp.id)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium disabled:opacity-50"
            >
              <span v-if="installingApps.has(featuredBannerApp.id)">Installing...</span>
              <span v-else>Install</span>
            </button>
            <span class="text-white/40 text-sm">{{ featuredBannerApp?.title }} {{ $ver(featuredBannerApp?.version) }}</span>
          </div>
        </div>
      </div>

      <!-- Promotions are ordered and written by the registry catalog. -->
      <div
        v-for="promotion in storefrontPromotions"
        :key="promotion.id"
        class="featured-banner source-banner glass-card mb-8 relative overflow-hidden cursor-pointer"
        @click="viewAppDetails(promotion.app)"
      >
        <img
          :src="promotion.banner"
          alt=""
          class="featured-banner-img source-banner-img"
          @error="(e: Event) => (e.target as HTMLImageElement).style.display = 'none'"
        />
        <div class="featured-banner-overlay source-banner-overlay">
          <div class="flex items-center gap-3 mb-2">
            <span class="discover-terminal-tag">{{ promotion.eyebrow }}</span>
            <span class="text-white/50 text-sm font-mono">{{ promotion.tag }}</span>
          </div>
          <h2 class="text-3xl md:text-4xl font-extrabold text-white mb-2 tracking-tight">{{ promotion.headline }}</h2>
          <p class="text-white/80 text-base md:text-lg max-w-2xl leading-relaxed mb-4">{{ promotion.description }}</p>
          <div class="flex flex-wrap items-center gap-3">
            <button
              v-if="isInstalled(promotion.app.id) && !isStartingUp(promotion.app.id)"
              @click.stop="launchInstalledApp(promotion.app, promotion.path)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium"
            >{{ promotion.launchLabel || 'Launch' }}</button>
            <button
              v-else-if="!isInstalled(promotion.app.id) && promotion.app.dockerImage"
              @click.stop="handleInstall(promotion.app)"
              :disabled="installingApps.has(promotion.app.id)"
              class="glass-button rounded-lg px-6 py-2.5 text-sm font-medium disabled:opacity-50"
            >{{ installingApps.has(promotion.app.id) ? 'Installing…' : (promotion.installLabel || 'Install') }}</button>
            <button
              @click.stop="viewAppDetails(promotion.app)"
              class="rounded-lg px-4 py-2.5 text-sm font-medium text-white/70 hover:text-white"
            >{{ promotion.detailsLabel || 'View details →' }}</button>
          </div>
        </div>
      </div>

      <!-- Category Section Divider -->
      <div class="flex items-center gap-3 mb-5">
        <span class="discover-terminal-tag">all</span>
        <h2 class="text-xl font-bold text-white">All Apps</h2>
        <div class="flex-1 h-px bg-white/10"></div>
        <span class="text-white/30 text-sm">{{ remainingApps.length }} apps</span>
      </div>
    </template>

    <!-- Search results header -->
    <div v-else class="flex items-center gap-3 mb-5">
      <span class="discover-terminal-tag">search</span>
      <h2 class="text-xl font-bold text-white">Search Results</h2>
      <div class="flex-1 h-px bg-white/10"></div>
      <span class="text-white/30 text-sm">{{ filteredApps.length }} apps</span>
    </div>

    <!-- Community Load Error -->
    <div v-if="communityError" class="alert-error mb-4">
      {{ communityError }}
      <button @click="loadCommunityMarketplace()" class="ml-2 underline hover:no-underline">Retry</button>
    </div>

    <AppGrid
      :filtered-apps="gridApps"
      :show-stagger="showStagger"
      :stagger-offset="selectedCategory === 'all' && !searchQuery ? 4 : 0"
      :containers-scanned="containersScanned"
      :installing-apps="installingApps"
      :is-installed="isInstalled"
      :is-starting-up="isStartingUp"
      :get-installed-state="getInstalledState"
      :get-app-tier="getAppTier"
      :is-loading="loadingCommunity || nostrLoading"
      :loading-message="nostrLoading ? 'Querying Nostr relays...' : 'Loading...'"
      :nostr-error="nostrError"
      :is-nostr-category="selectedCategory === 'nostr'"
      :search-query="searchQuery"
      @view-details="viewAppDetails"
      @launch="launchInstalledApp"
      @install="handleInstall"
      @retry-nostr="retryNostr"
    />

    <!-- The node-owned companion download is operational UI, not registry
         merchandising, so it follows the complete app listing. -->
    <CompanionBanner v-if="!searchQuery" />

    <!-- Manifesto Footer (only when no search) -->
    <div v-if="!searchQuery && filteredApps.length > 0" class="discover-manifesto glass-card p-8 mt-4 mb-8">
      <div class="flex items-center gap-3 mb-4">
        <span class="discover-terminal-tag text-orange-400/80">manifesto</span>
        <div class="flex-1 h-px bg-white/10"></div>
      </div>
      <blockquote class="text-white/80 text-xl leading-relaxed italic max-w-3xl">
        "Privacy is not about having something to hide. Privacy is about having the right to choose
        what to reveal. In a world of surveillance capitalism, self-hosting is an act of resistance.
        Every service you run on your own hardware is a vote for a future where individuals &mdash; not
        corporations &mdash; control their digital lives."
      </blockquote>
      <p class="text-white/60 text-xl mt-4 font-mono">// Cypherpunks write code. We run nodes.</p>
    </div>

    <!-- First-install version chooser (Bitcoin Knots / Core) -->
    <InstallVersionModal
      :show="showInstallModal"
      :app-id="installModalApp?.id || ''"
      :app="installModalApp"
      @close="showInstallModal = false; installModalApp = null"
      @confirm="onInstallModalConfirm"
    />

  </div>
</template>

<script lang="ts">
let discoverAnimationDone = false
</script>

<script setup lang="ts">
import { normalizeStoreApps } from './apps/serviceNames'
import AppSearchField from '@/components/AppSearchField.vue'
import { ref, computed, onBeforeUnmount, onMounted } from 'vue'
import { useRouter, RouterLink } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { useServerStore } from '@/stores/server'
import { rpcClient } from '@/api/rpc-client'
import { useMarketplaceApp } from '@/composables/useMarketplaceApp'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useToast } from '@/composables/useToast'
import { useCollapsingHeaderTabs } from '@/composables/useCollapsingHeaderTabs'
import { useContainersScanTimeout } from '@/composables/useContainersScanTimeout'
import { useCachedResource } from '@/composables/useCachedResource'
import RefreshIndicator from '@/components/RefreshIndicator.vue'
import { APP_STORE_SECTIONS } from './appStoreCategories'
import DiscoverHero from './discover/DiscoverHero.vue'
import FeaturedApps from './discover/FeaturedApps.vue'
import CompanionBanner from './discover/CompanionBanner.vue'
import AppGrid from './discover/AppGrid.vue'
import InstallVersionModal from '@/components/InstallVersionModal.vue'
import type { MarketplaceApp, FeaturedApp } from './discover/types'
import { getCuratedAppList, INSTALLED_ALIASES, FEATURED_DEFINITIONS, categorizeCommunityApp, fetchAppCatalog, type CatalogFeatured, type CatalogStorefront } from './discover/curatedApps'
import { isServiceContainer } from './apps/serviceNames'

const router = useRouter()
const store = useAppStore()
const serverStore = useServerStore()

const showStagger = !discoverAnimationDone
const { setCurrentApp } = useMarketplaceApp()
const appLauncher = useAppLauncherStore()

const selectedCategory = ref('all')
const searchQuery = ref('')

// Community marketplace + Bitcoin prune-status — routed through the SAME
// shared cache keys the 02-02 tracer introduced on Marketplace.vue
// ('app-catalog', 'bitcoin.prune-status') rather than duplicating the
// fetch (02-04). Both views populate the same cache entry, whichever one
// happens to trigger a given fetch — the store doesn't care which caller's
// fetcher ran, only the resulting cached value.
const catalogResource = useCachedResource<MarketplaceApp[]>({
  key: 'app-catalog',
  // Preserves this view's existing dynamic-catalog-first behavior (Marketplace's
  // own fetcher for this key is the simpler getCuratedAppList()-only form —
  // both are valid producers of the same shared cache entry).
  fetcher: async () => {
    const catalog = await fetchAppCatalog()
    if (catalog) {
      if (import.meta.env.DEV) console.log('Loaded app catalog from registry:', catalog.apps.length, 'apps')
      return catalog.apps
    }
    if (import.meta.env.DEV) console.log('Using hardcoded app list (catalog.json unavailable)')
    return getCuratedAppList()
  },
  ttlMs: 300_000,
  persist: true,
})
// Featured banner payload lives on its own cache key (WR-01), deliberately
// NOT derived from catalogResource's fetcher: 'app-catalog' is a shared key
// with Marketplace.vue, whose simpler getCuratedAppList()-only fetcher can
// win the in-flight dedup race and never populate the featured payload.
// fetchAppCatalog() has its own internal memoization (1h TTL + localStorage
// fallback), so this second subscription is normally a cache hit, not an
// extra network round trip.
const catalogFeatured = useCachedResource<CatalogFeatured | null>({
  key: 'app-catalog:featured',
  fetcher: async () => (await fetchAppCatalog())?.featured ?? null,
  ttlMs: 300_000,
  persist: true,
}).data
const catalogStorefront = useCachedResource<CatalogStorefront | null>({
  key: 'app-catalog:storefront',
  fetcher: async () => (await fetchAppCatalog())?.storefront ?? null,
  ttlMs: 300_000,
  persist: true,
}).data
const communityApps = computed(() => normalizeStoreApps(catalogResource.data.value ?? []))
const loadingCommunity = computed(() => catalogResource.entry.loadState === 'loading')
// Keep-last-value error banner (D-07): a failed background refresh never
// raises a toast, it only surfaces here — content stays on screen either way.
const communityError = computed(() => catalogResource.error.value ?? '')
function loadCommunityMarketplace() {
  return catalogResource.refresh()
}

interface BitcoinStatusResponse {
  blockchain_info?: { pruned?: boolean }
}
const pruneStatusResource = useCachedResource<BitcoinStatusResponse | null>({
  key: 'bitcoin.prune-status',
  fetcher: async (signal) => {
    const res = await fetch('/bitcoin-status', { credentials: 'include', signal })
    if (!res.ok) throw new Error(`bitcoin-status responded ${res.status}`)
    return res.json()
  },
  persist: true,
})
const bitcoinPruned = computed(() => pruneStatusResource.data.value?.blockchain_info?.pruned === true)
function loadBitcoinPruneStatus() {
  return pruneStatusResource.refresh()
}

const electrumxArchiveWarning = 'You need a full archival bitcoin node before downloading ElectrumX'
const discoverHeaderRef = ref<HTMLElement | null>(null)
const discoverPrimaryRef = ref<HTMLElement | null>(null)
const discoverCategoryProbeRef = ref<HTMLElement | null>(null)
const { collapsed: collapseCategories } = useCollapsingHeaderTabs(
  discoverHeaderRef,
  discoverPrimaryRef,
  discoverCategoryProbeRef,
  144
)

const appStoreSections = computed(() => APP_STORE_SECTIONS)

// Installation state — uses global store so it persists across navigation.
// The store's watcher (stores/server.ts) handles install-progress updates
// globally, so this view doesn't need its own watcher. Previously had a
// local watcher that duplicated logic using byte counters only — it has
// been removed in favour of the store's phase-aware mapping.
const installingApps = serverStore.installingApps

// First-install version-choice modal (multi-version apps: Bitcoin Knots / Core)
const showInstallModal = ref(false)
const installModalApp = ref<MarketplaceApp | null>(null)

function navigateToMarketplace(categoryId: string) {
  router.push({ name: 'marketplace', query: { category: categoryId } })
}

function selectDiscoverCategory(categoryId: string) {
  if (categoryId === 'discover') {
    router.push('/dashboard/discover')
    return
  }
  navigateToMarketplace(categoryId)
}

// Nostr community marketplace state (community/catalog state is declared
// above, alongside the shared catalogResource/pruneStatusResource)
const nostrApps = ref<MarketplaceApp[]>([])
const nostrLoading = ref(false)
const nostrError = ref('')

async function loadNostrMarketplace() {
  if (nostrApps.value.length > 0 || nostrLoading.value) return
  nostrLoading.value = true
  nostrError.value = ''
  try {
    const res = await rpcClient.marketplaceDiscover()
    nostrApps.value = res.apps.map(app => ({
      id: app.manifest.app_id,
      title: app.manifest.name,
      version: app.manifest.version,
      description: typeof app.manifest.description === 'string'
        ? app.manifest.description
        : app.manifest.description,
      icon: app.manifest.icon_url || '',
      author: app.manifest.author.name,
      dockerImage: app.manifest.container.image,
      repoUrl: app.manifest.repo_url,
      category: app.manifest.category,
      source: 'nostr',
      trustScore: app.trust_score,
      trustTier: app.trust_tier,
      relayCount: app.relay_count,
    }))
  } catch (e) {
    nostrError.value = e instanceof Error ? e.message : 'Discovery failed'
    if (import.meta.env.DEV) console.warn('Nostr marketplace discovery failed:', e)
  } finally {
    nostrLoading.value = false
  }
}

function retryNostr() {
  nostrApps.value = []
  loadNostrMarketplace()
}

const installedPackages = computed(() => store.data?.['package-data'] || {})
const containersScannedRaw = computed(() => store.data?.['server-info']?.['status-info']?.['containers-scanned'] ?? false)
// Escape hatch: never leave app cards on "Checking..." forever — after a
// timeout, treat the scan as done so cards render their normal install state.
const { effectiveContainersScanned: containersScanned } = useContainersScanTimeout(
  containersScannedRaw,
  computed(() => store.hasLoadedInitialData),
)


const allApps = computed(() => {
  const local: (MarketplaceApp & { category: string; source: string })[] = []
  const community = communityApps.value.map(app => ({
    ...app,
    category: categorizeCommunityApp(app),
    source: 'community'
  }))
  const base = [...local, ...community]

  if (nostrApps.value.length > 0) {
    const existingIds = new Set(base.map(a => a.id))
    const nostrMerged = nostrApps.value
      .filter(app => !existingIds.has(app.id))
      .map(app => ({ ...app, category: app.category || categorizeCommunityApp(app), source: 'nostr' }))
    return normalizeStoreApps([...base, ...nostrMerged])
  }
  return normalizeStoreApps(base)
})

const filteredApps = computed(() => {
  let apps = allApps.value
  if (selectedCategory.value && selectedCategory.value !== 'all' && !searchQuery.value) {
    apps = apps.filter(app => app.category === selectedCategory.value)
  }
  if (searchQuery.value) {
    const query = searchQuery.value.toLowerCase()
    apps = apps.filter(app =>
      app.title?.toLowerCase().includes(query) ||
      (typeof app.description === 'string' && app.description.toLowerCase().includes(query)) ||
      (typeof app.description === 'object' && app.description?.short?.toLowerCase().includes(query)) ||
      app.id?.toLowerCase().includes(query) ||
      app.author?.toLowerCase().includes(query)
    )
  }
  // Hide installed apps and web-only links (no dockerImage = not installable)
  apps = apps.filter(app => !isInstalled(app.id) && app.dockerImage)
  return apps
})

const popularApps = computed(() => {
  const byId = new Map(allApps.value.map(app => [app.id, app]))
  const popular: MarketplaceApp[] = []
  for (const id of catalogStorefront.value?.popular ?? []) {
    const app = byId.get(id)
    if (app?.dockerImage) popular.push(app)
  }
  return popular
})

const popularIds = computed(() => new Set(popularApps.value.map(app => app.id)))
const remainingApps = computed(() =>
  catalogStorefront.value
    ? filteredApps.value.filter(app => !popularIds.value.has(app.id))
    : filteredApps.value
)
const gridApps = computed(() => searchQuery.value ? filteredApps.value : remainingApps.value)

const storefrontPromotions = computed(() => {
  if (!catalogStorefront.value) return []
  return catalogStorefront.value.promotions.flatMap(promotion => {
    const app = allApps.value.find(candidate => candidate.id === promotion.id)
    return app ? [{ ...promotion, app }] : []
  })
})

const installedCount = computed(() => {
  return allApps.value.filter(app => isInstalled(app.id)).length
})

// Featured banner — from catalog.json or first FEATURED_DEFINITIONS entry with banner
const featuredBanner = computed(() => {
  if (catalogFeatured.value) return catalogFeatured.value
  const first = FEATURED_DEFINITIONS.find(f => f.banner)
  if (!first) return null
  const app = allApps.value.find(a => a.id === first.id)
  if (!app) return null
  return { id: first.id, banner: first.banner!, headline: app.title ?? first.id, description: first.desc, tag: first.tag }
})

const featuredBannerApp = computed(() => {
  if (!featuredBanner.value) return null
  return allApps.value.find(a => a.id === featuredBanner.value!.id) ?? null
})

const featuredApps = computed<FeaturedApp[]>(() => {
  return FEATURED_DEFINITIONS
    .map(f => {
      const app = allApps.value.find(a => a.id === f.id)
      if (!app) return null
      return { ...app, featuredDescription: f.desc, privacyTag: f.tag } as FeaturedApp
    })
    .filter((a): a is FeaturedApp => a !== null)
})


function isInstalled(appId: string): boolean {
  if (appId in installedPackages.value) return true
  const aliases = INSTALLED_ALIASES[appId]
  return aliases ? aliases.some((a) => a in installedPackages.value) : false
}

function getInstalledState(appId: string): string | null {
  const pkg = installedPackages.value[appId]
  if (pkg) return pkg.state
  const aliases = INSTALLED_ALIASES[appId]
  if (aliases) {
    for (const a of aliases) {
      const aliasPkg = installedPackages.value[a]
      if (aliasPkg) return aliasPkg.state
    }
  }
  return null
}

function isStartingUp(appId: string): boolean {
  const state = getInstalledState(appId)
  return state !== null && state !== 'running' && state !== 'stopped' && state !== 'exited'
}

function getAppTier(appId: string): string {
  const core = ['bitcoin-knots', 'bitcoin', 'lnd', 'mempool', 'btcpay-server', 'filebrowser']
  const recommended = ['fedimint', 'vaultwarden', 'uptime-kuma', 'grafana', 'searxng', 'tailscale', 'netbird', 'portainer']
  if (core.includes(appId)) return 'core'
  if (recommended.includes(appId)) return 'recommended'
  return 'optional'
}

function launchInstalledApp(app: MarketplaceApp, path?: string) {
  appLauncher.openSession(app.id, path ? { path } : undefined)
}

async function handleInstall(app: MarketplaceApp) {
  const blocked = installBlockedReason(app.id)
  if (blocked) {
    toast.error(blocked)
    return
  }
  if (installingApps.has(app.id) || isInstalled(app.id)) return
  if (['bitcoin-core', 'bitcoin-knots'].includes(app.id)) {
    installModalApp.value = app
    showInstallModal.value = true
    return
  }
  // Multi-version apps (Bitcoin Knots / Core): let the runner pick a version up
  // front via a full-screen modal (latest pre-selected) instead of silently
  // installing the default. Best-effort — if the lookup fails we install directly.
  try {
    const info = await rpcClient.getPackageVersions(app.id)
    if (info.supportsVersions && info.versions.length > 1) {
      installModalApp.value = app
      showInstallModal.value = true
      return
    }
  } catch { /* no catalog versions — fall through to direct install */ }
  startInstall(app)
}

function startInstall(app: MarketplaceApp, versionOverride?: string, prune?: boolean) {
  if (app.source === 'local') {
    installApp(app, versionOverride, prune)
  } else {
    installCommunityApp(app, versionOverride, prune)
  }
}

function onInstallModalConfirm(version: string, prune?: boolean) {
  const app = installModalApp.value
  showInstallModal.value = false
  installModalApp.value = null
  if (app) startInstall(app, version, prune)
}

function viewAppDetails(app: MarketplaceApp) {
  try {
    if (isInstalled(app.id)) {
      router.push({ name: 'app-details', params: { id: app.id }, query: { from: 'discover' } })
    } else {
      setCurrentApp(app)
      router.push({ name: 'marketplace-app-detail', params: { id: app.id }, query: { from: 'discover' } })
    }
  } catch (e) {
    if (import.meta.env.DEV) console.error('[Discover] Navigation error:', e)
  }
}

// Timer management
const activeTimers: ReturnType<typeof setTimeout>[] = []

function trackTimeout(fn: () => void, ms: number) {
  const id = setTimeout(() => {
    const idx = activeTimers.indexOf(id)
    if (idx !== -1) activeTimers.splice(idx, 1)
    fn()
  }, ms)
  activeTimers.push(id)
  return id
}

onBeforeUnmount(() => {
  for (const t of activeTimers) clearTimeout(t)
  activeTimers.length = 0
})

const toast = useToast()

function installToast(app: MarketplaceApp) {
  const service = isServiceContainer(app.id)
  const destination = service ? 'Services' : 'My Apps'
  toast.action(
    `Installing ${app.title ?? app.id} — it will appear in ${destination}`,
    { label: `View ${destination}`, onClick: () => router.push({ path: '/dashboard/apps', query: service ? { tab: 'services' } : {} }) },
    { variant: 'info', duration: 15000 },
  )
}

function installBlockedReason(appId: string): string | undefined {
  if (!bitcoinPruned.value) return undefined
  if (appId !== 'electrumx' && appId !== 'electrs' && appId !== 'mempool-electrs') return undefined
  return electrumxArchiveWarning
}

function queueInstall(app: MarketplaceApp) {
  serverStore.setInstallProgress(app.id, {
    id: app.id,
    title: app.title ?? app.id,
    status: 'downloading',
    progress: 2,
    message: 'Queued…',
    attempt: 0,
  })
}

function failInstall(app: MarketplaceApp, err: unknown) {
  const message = "Failed: " + (err instanceof Error ? err.message : String(err))
  serverStore.setInstallProgress(app.id, {
    id: app.id,
    title: app.title ?? app.id,
    status: 'error',
    progress: 0,
    message,
    attempt: 0,
  })
  trackTimeout(() => { serverStore.clearInstallProgress(app.id) }, 5000)
}

async function installApp(app: MarketplaceApp, versionOverride?: string, prune?: boolean) {
  if (installingApps.has(app.id) || isInstalled(app.id)) return
  queueInstall(app)
  installToast(app)
  try {
    const installUrl = app.url || app.manifestUrl || app.s9pkUrl
    await rpcClient.call({ method: 'package.install', params: { id: app.id, url: installUrl, version: versionOverride || app.version, ...(prune === undefined ? {} : { prune }) }, timeout: 600000 })
  } catch (err) {
    if (import.meta.env.DEV) console.error('Installation failed:', err)
    failInstall(app, err)
  }
}

async function installCommunityApp(app: MarketplaceApp, versionOverride?: string, prune?: boolean) {
  if (installingApps.has(app.id) || isInstalled(app.id) || !app.dockerImage) return
  queueInstall(app)
  installToast(app)
  try {
    const installParams: Record<string, unknown> = { id: app.id, dockerImage: app.dockerImage, version: versionOverride || app.version, ...(prune === undefined ? {} : { prune }) }
    if ((app as Record<string, unknown>).containerConfig) {
      installParams.containerConfig = (app as Record<string, unknown>).containerConfig
    }
    await rpcClient.call({ method: 'package.install', params: installParams, timeout: 600000 })
  } catch (err) {
    if (import.meta.env.DEV) console.error('[Discover] Installation failed:', err)
    failInstall(app, err)
  }
}

// Once-per-session: the intro stagger flag. The catalog/prune-status loads
// need no onMounted or onActivated call of their own — catalogResource and
// pruneStatusResource are useCachedResource-backed with `immediate` at its
// default (true), so each fetches itself on setup and revalidates itself,
// staleness-gated, on every later reactivation via the composable's own
// onActivated hook (same precedent as Marketplace.vue in 02-02).
onMounted(() => {
  discoverAnimationDone = true
})

// Exposed for tests only (mirrors Marketplace.vue's own defineExpose for the
// same shared-key resources).
defineExpose({ loadCommunityMarketplace, loadBitcoinPruneStatus })
</script>
