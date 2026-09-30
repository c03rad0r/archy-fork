<template>
  <div class="marketplace-container">
    <!-- Header Section -->
    <div>
      <!-- Desktop: tabs + categories + search -->
      <div ref="marketplaceHeaderRef" class="app-header-desktop mb-4 items-center gap-4 relative">
        <div ref="marketplacePrimaryRef" class="flex-shrink-0">
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
            @click="selectAppStoreSection(section.id)"
            class="mode-switcher-btn"
            :class="{ 'mode-switcher-btn-active': selectedCategory === section.id }"
          >
            {{ section.name }}
            <span v-if="section.id === 'nostr' && nostrApps.length > 0" class="ml-1 text-xs px-1.5 py-0.5 rounded-full bg-white/10">+{{ nostrApps.length }}</span>
          </button>
        </div>
        <div v-show="collapseCategories" class="segmented-select flex-shrink-0">
          <label class="sr-only" for="marketplace-category-select">App Store category</label>
          <select
            id="marketplace-category-select"
            :value="selectedCategory"
            class="segmented-select-control"
            @change="selectAppStoreSection(($event.target as HTMLSelectElement).value)"
          >
            <option
              v-for="section in appStoreSections"
              :key="section.id"
              :value="section.id"
            >
              {{ section.name }}{{ section.id === 'nostr' && nostrApps.length > 0 ? ` +${nostrApps.length}` : '' }}
            </option>
          </select>
        </div>
        <div ref="marketplaceCategoryProbeRef" class="mode-switcher category-tabs-probe" aria-hidden="true">
          <button
            v-for="section in appStoreSections"
            :key="section.id"
            class="mode-switcher-btn"
            type="button"
          >
            {{ section.name }}
          </button>
        </div>
        <input
          v-model="searchQuery"
          type="text"
          :placeholder="t('marketplace.searchPlaceholder')"
          :aria-label="t('marketplace.searchApps')"
          class="app-header-search px-4 py-2 bg-white/10 border border-white/20 rounded-lg text-white placeholder-white/50 focus:outline-none focus:border-white/40 transition-colors"
        />
        <RefreshIndicator :state="catalogResource.entry.loadState" label="Refreshing app store catalog" />
      </div>

      <!-- Mobile: categories + search (tabs handled by Dashboard.vue header) -->
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
            @click="selectAppStoreSection(section.id)"
            class="mobile-category-pill"
            :class="{ 'mobile-category-pill-active': selectedCategory === section.id }"
            type="button"
          >{{ section.name }}</button>
        </div>
        <input
          v-model="searchQuery"
          type="text"
          :placeholder="t('marketplace.searchPlaceholder')"
          :aria-label="t('marketplace.searchApps')"
          class="w-full px-4 py-3 md:py-2 bg-white/10 border border-white/20 rounded-lg text-white placeholder-white/50 focus:outline-none focus:border-white/40 transition-colors"
        />
      </div>
    </div>

    <!-- Scrollable Apps Section -->
    <div class="pb-8">
      <!-- Community Load Error -->
      <div v-if="communityError" class="alert-error mb-4">
        {{ communityError }}
        <button @click="loadCommunityMarketplace()" class="ml-2 underline hover:no-underline">Retry</button>
      </div>

      <!-- Apps Grid -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        <template v-if="(loadingCommunity || nostrLoading) && filteredApps.length === 0">
          <div
            v-for="index in 6"
            :key="`loading-${index}`"
            class="glass-card p-6 flex flex-col app-card-skeleton"
            aria-hidden="true"
          >
            <div class="flex items-start gap-4 mb-4">
              <div class="app-card-skeleton-icon"></div>
              <div class="flex-1 min-w-0 pt-1">
                <div class="app-card-skeleton-line w-3/4 mb-3"></div>
                <div class="app-card-skeleton-line w-1/3"></div>
              </div>
            </div>
            <div class="app-card-skeleton-line w-full mb-2"></div>
            <div class="app-card-skeleton-line w-5/6 mb-2"></div>
            <div class="app-card-skeleton-line w-2/3 mb-5"></div>
            <div class="app-card-skeleton-button mt-auto"></div>
          </div>
        </template>
        <MarketplaceAppCard
          v-for="(app, index) in filteredApps"
          :key="app.id"
          :app="app"
          :index="index"
          :stagger="showStagger"
          :installed="isInstalled(app.id)"
          :installing="installingApps.has(app.id)"
          :install-progress="installingApps.get(app.id)"
          :installed-state="getInstalledState(app.id)"
          :starting-up="isStartingUp(app.id)"
          :containers-scanned="containersScanned"
          :tier-label="getAppTier(app.id)"
          :install-blocked-reason="installBlockedReason(app.id)"
          @view="viewAppDetails"
          @install="handleInstall(app)"
          @launch="launchInstalledApp"
        />
      </div>

      <!-- Empty State -->
      <div v-if="filteredApps.length === 0 && !(loadingCommunity || nostrLoading)" class="text-center py-12">
        <div v-if="nostrError && selectedCategory === 'nostr'" class="flex flex-col items-center gap-4">
          <p class="text-white/70">{{ t('marketplace.noCommunityApps') }}</p>
          <p class="text-white/40 text-sm">{{ nostrError }}</p>
          <button @click="nostrApps = []; loadNostrMarketplace()" class="px-4 py-2 glass-button rounded-lg text-sm">{{ t('common.retry') }}</button>
        </div>
        <p v-else class="text-white/70">{{ searchQuery && selectedCategory !== 'all' ? t('marketplace.noResults', { category: categories.find(c => c.id === selectedCategory)?.name, query: searchQuery }) : searchQuery ? t('marketplace.noResultsSearch', { query: searchQuery }) : t('marketplace.noResultsCategory', { category: categories.find(c => c.id === selectedCategory)?.name }) }}</p>
      </div>
    </div>
    <!-- End Scrollable Apps Section -->
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
let marketplaceAnimationDone = false
</script>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import { useRouter, useRoute, RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
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
import InstallVersionModal from '@/components/InstallVersionModal.vue'
import { APP_STORE_CATEGORIES, APP_STORE_SECTIONS } from './appStoreCategories'
import MarketplaceAppCard from './marketplace/MarketplaceAppCard.vue'
import {
  type MarketplaceApp,
  INSTALLED_ALIASES,
  MULTI_VERSION_APP_IDS,
  getAppTier,
  categorizeCommunityApp,
  getCuratedAppList,
} from './marketplace/marketplaceData'
import { fetchAppCatalog } from './discover/curatedApps'
import { isServiceContainer } from './apps/serviceNames'

const router = useRouter()
const route = useRoute()
const store = useAppStore()
const server = useServerStore()
const { t } = useI18n()

const showStagger = !marketplaceAnimationDone
const { setCurrentApp } = useMarketplaceApp()
const appLauncher = useAppLauncherStore()
const toast = useToast()

// Category state — read initial value from query param (set by Discover page navigation)
const selectedCategory = ref((route.query.category as string) || 'all')

const categories = computed(() => APP_STORE_CATEGORIES)
const appStoreSections = computed(() => APP_STORE_SECTIONS)

// Installation state — uses global store so it persists across navigation
const installingApps = server.installingApps
const showInstallModal = ref(false)
const installModalApp = ref<MarketplaceApp | null>(null)
const electrumxArchiveWarning = 'You need a full archival bitcoin node before downloading ElectrumX'

function installToast(app: MarketplaceApp) {
  const service = isServiceContainer(app.id)
  const tab = service ? 'services' : 'apps'
  const destination = service ? 'Services' : 'My Apps'
  toast.action(
    `Installing ${app.title ?? app.id} — it will appear in ${destination}`,
    { label: `View ${destination}`, onClick: () => router.push({ path: '/dashboard/apps', query: service ? { tab } : {} }) },
    { variant: 'info', duration: 15000 },
  )
}

// Install progress tracking is now in serverStore (global watcher on WebSocket data)
// so it works regardless of which page is active

// Select category and trigger Nostr relay discovery when 'nostr' is chosen
function selectCategory(id: string) {
  selectedCategory.value = id
  const query = id === 'all' ? {} : { category: id }
  router.replace({ name: 'marketplace', query }).catch(() => {})
  if (id === 'nostr' && nostrApps.value.length === 0 && !nostrLoading.value) {
    loadNostrMarketplace()
  }
}

function selectAppStoreSection(id: string) {
  if (id === 'discover') {
    router.push('/dashboard/discover')
    return
  }
  selectCategory(id)
}

watch(() => route.query.category, (category) => {
  const next = typeof category === 'string' && category ? category : 'all'
  selectedCategory.value = next
  if (next === 'nostr' && nostrApps.value.length === 0 && !nostrLoading.value) {
    loadNostrMarketplace()
  }
})

// Community marketplace state — cached (D-09/D-06: near-static catalog, long
// TTL) behind a shared key so Discover.vue's identical loader picks up the
// same cache entry without its own conversion (plan 02-04). Non-sensitive
// and small, so it persists across reloads. Dynamic-catalog-first: the
// daemon-verified signed catalog is what makes a newly published app appear
// without a dashboard release — the static list below is only the offline
// fallback (same fetcher contract as Discover.vue for this shared key).
const catalogResource = useCachedResource<MarketplaceApp[]>({
  key: 'app-catalog',
  fetcher: async () => {
    const catalog = await fetchAppCatalog()
    if (catalog && catalog.apps.length) return catalog.apps
    return getCuratedAppList()
  },
  ttlMs: 300_000,
  persist: true,
})
const communityApps = computed(() => catalogResource.data.value ?? [])
const loadingCommunity = computed(() => catalogResource.entry.loadState === 'loading')
// Keep-last-value error banner (D-07): a failed background refresh never
// raises a toast, it only surfaces here — content stays on screen either way.
const communityError = computed(() => catalogResource.error.value ?? '')

interface BitcoinStatusResponse {
  blockchain_info?: { pruned?: boolean }
}

// Prune status — the default TTL (30s) matches the rest of the app; also
// non-sensitive and small, so it persists too.
const pruneStatusResource = useCachedResource<BitcoinStatusResponse | null>({
  key: 'bitcoin.prune-status',
  fetcher: async (signal) => {
    const res = await fetch('/bitcoin-status', { credentials: 'include', signal })
    if (!res.ok) throw new Error(`bitcoin-status responded ${res.status}`)
    return res.json()
  },
  persist: true,
})

const searchQuery = ref('')
const bitcoinPruned = computed(() => pruneStatusResource.data.value?.blockchain_info?.pruned === true)
const marketplaceHeaderRef = ref<HTMLElement | null>(null)
const marketplacePrimaryRef = ref<HTMLElement | null>(null)
const marketplaceCategoryProbeRef = ref<HTMLElement | null>(null)
const { collapsed: collapseCategories } = useCollapsingHeaderTabs(
  marketplaceHeaderRef,
  marketplacePrimaryRef,
  marketplaceCategoryProbeRef,
  144
)

// Nostr community marketplace state
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
      // Default to `missing` rather than leaving it undefined: a node running
      // an older backend returns no field at all, and "we couldn't check" must
      // never render as "signed".
      signature: app.signature ?? { status: 'missing' as const },
      authorDid: app.manifest.author.did,
    }))
  } catch (e) {
    nostrError.value = e instanceof Error ? e.message : 'Discovery failed'
    if (import.meta.env.DEV) console.warn('Nostr marketplace discovery failed:', e)
  } finally {
    nostrLoading.value = false
  }
}

const installedPackages = computed(() => {
  return store.data?.['package-data'] || {}
})

const containersScannedRaw = computed(() => {
  return store.data?.['server-info']?.['status-info']?.['containers-scanned'] ?? false
})
// Escape hatch: never leave app cards on "Checking..." forever — after a
// timeout, treat the scan as done so cards render their normal install state.
const { effectiveContainersScanned: containersScanned } = useContainersScanTimeout(
  containersScannedRaw,
  computed(() => store.hasLoadedInitialData),
)

// Combine curated apps with Nostr relay-discovered apps
const allApps = computed(() => {
  const local: (MarketplaceApp & { category: string; source: string })[] = []

  const community = communityApps.value.map(app => {
    const category = categorizeCommunityApp(app)
    return { ...app, category, source: 'community' }
  })

  const base = [...local, ...community]

  if (nostrApps.value.length > 0) {
    const existingIds = new Set(base.map(a => a.id))
    const nostrMerged = nostrApps.value
      .filter(app => !existingIds.has(app.id))
      .map(app => {
        const category = app.category || categorizeCommunityApp(app)
        return { ...app, category, source: 'nostr' }
      })
    return [...base, ...nostrMerged]
  }

  return base
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

  // Hide installed, installing, and web-only apps (no dockerImage = not installable)
  apps = apps.filter(app => !isInstalled(app.id) && !installingApps.has(app.id) && app.dockerImage)

  return apps
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

function launchInstalledApp(app: MarketplaceApp) {
  appLauncher.openSession(app.id)
}

// onMounted fires exactly once for the lifetime of this kept-alive instance
// (D-01/plan 02-01). marketplaceAnimationDone is genuinely once-per-session
// intro state, so it stays here. The catalog and prune-status loads used to
// live here too, fetching fresh on every remount; they're now cache-gated
// useCachedResource entries whose own onActivated revalidates them on every
// tab re-entry (staleness-checked, so a quick revisit issues no RPC) — they
// need no per-view mount/activation hook of their own.
onMounted(() => {
  marketplaceAnimationDone = true
})

/** Force-refresh the Bitcoin prune-status cache entry (deduped with any
 * in-flight refresh, per useCachedResource). Exposed for tests. */
function loadBitcoinPruneStatus() {
  return pruneStatusResource.refresh()
}

function installBlockedReason(appId: string): string | undefined {
  if (!bitcoinPruned.value) return undefined
  if (appId !== 'electrumx' && appId !== 'electrs' && appId !== 'mempool-electrs') return undefined
  return electrumxArchiveWarning
}

/** Force-refresh the app-catalog cache entry (used by the Retry button and
 * exposed for tests). */
function loadCommunityMarketplace() {
  return catalogResource.refresh()
}

function viewAppDetails(app: MarketplaceApp) {
  if (import.meta.env.DEV) console.log('[Marketplace] Navigating to app detail:', app)

  try {
    if (isInstalled(app.id)) {
      if (import.meta.env.DEV) console.log('[Marketplace] App is installed, navigating to app details page')
      router.push({ name: 'app-details', params: { id: app.id } })
    } else {
      setCurrentApp(app)
      if (import.meta.env.DEV) console.log('[Marketplace] App data stored in composable')
      router.push({ name: 'marketplace-app-detail', params: { id: app.id } })
    }
  } catch (e) {
    if (import.meta.env.DEV) console.error('[Marketplace] Navigation error:', e)
  }
}

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

function queueInstall(app: MarketplaceApp) {
  server.setInstallProgress(app.id, {
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
  server.setInstallProgress(app.id, {
    id: app.id,
    title: app.title ?? app.id,
    status: 'error',
    progress: 0,
    message,
    attempt: 0,
  })
  trackTimeout(() => { server.clearInstallProgress(app.id) }, 5000)
}

function handleInstall(app: MarketplaceApp) {
  if (installingApps.has(app.id) || isInstalled(app.id)) return
  const blocked = installBlockedReason(app.id)
  if (blocked) {
    toast.error(blocked)
    return
  }
  if (MULTI_VERSION_APP_IDS.has(app.id)) {
    installModalApp.value = app
    showInstallModal.value = true
    return
  }
  startInstall(app)
}

function startInstall(app: MarketplaceApp, version?: string, prune?: boolean) {
  if (app.source === 'local') void installApp(app, version, prune)
  else void installCommunityApp(app, version, prune)
}

function onInstallModalConfirm(version: string, prune?: boolean) {
  const app = installModalApp.value
  showInstallModal.value = false
  installModalApp.value = null
  if (app) startInstall(app, version, prune)
}

async function installApp(app: MarketplaceApp, versionOverride?: string, prune?: boolean) {
  if (installingApps.has(app.id) || isInstalled(app.id)) return
  const blocked = installBlockedReason(app.id)
  if (blocked) {
    toast.error(blocked)
    return
  }

  queueInstall(app)
  // Stay on the store page: the tile itself shows install progress via the
  // global watcher, and a forced jump to My Apps yanked the user out of the
  // page they were deliberately browsing.
  installToast(app)

  try {
    const installUrl = app.url || app.manifestUrl || app.s9pkUrl
    await rpcClient.call({
      method: 'package.install',
      params: { id: app.id, url: installUrl, version: versionOverride || app.version, ...(prune === undefined ? {} : { prune }) },
      timeout: 600000,
    })
  } catch (err) {
    if (import.meta.env.DEV) console.error('Installation failed:', err)
    failInstall(app, err)
  }
}

async function installCommunityApp(app: MarketplaceApp, versionOverride?: string, prune?: boolean) {
  if (installingApps.has(app.id) || isInstalled(app.id) || !app.dockerImage) return
  const blocked = installBlockedReason(app.id)
  if (blocked) {
    toast.error(blocked)
    return
  }

  queueInstall(app)
  // Stay on the store page (see installApp).
  installToast(app)

  try {
    const installParams: Record<string, unknown> = { id: app.id, dockerImage: app.dockerImage, version: versionOverride || app.version, ...(prune === undefined ? {} : { prune }) }
    if (app.containerConfig) installParams.containerConfig = app.containerConfig
    await rpcClient.call({
      method: 'package.install',
      params: installParams,
      timeout: 600000,
    })
  } catch (err) {
    if (import.meta.env.DEV) console.error('[Marketplace] Installation failed:', err)
    failInstall(app, err)
  }
}

// Exposed for tests only (mirrors Cloud.vue's `defineExpose({ loadPeers })`):
// lets a test trigger a background refresh directly without waiting on TTL
// staleness or a real KeepAlive round-trip.
defineExpose({ loadCommunityMarketplace, loadBitcoinPruneStatus })
</script>

<style scoped>
/* Custom scrollbar styling for apps section */
.marketplace-container ::-webkit-scrollbar {
  width: 8px;
}

.marketplace-container ::-webkit-scrollbar-track {
  background: rgba(255, 255, 255, 0.05);
  border-radius: 4px;
}

.marketplace-container ::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 4px;
  transition: background 0.3s ease;
}

.marketplace-container ::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.3);
}

/* Firefox scrollbar */
.marketplace-container {
  scrollbar-width: thin;
  scrollbar-color: rgba(255, 255, 255, 0.2) rgba(255, 255, 255, 0.05);
}

.app-card-skeleton {
  min-height: 255px;
  pointer-events: none;
}

.app-card-skeleton-icon,
.app-card-skeleton-line,
.app-card-skeleton-button {
  position: relative;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.08);
}

.app-card-skeleton-icon::after,
.app-card-skeleton-line::after,
.app-card-skeleton-button::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.12), transparent);
  animation: skeleton-shimmer 1.8s ease-in-out infinite;
}

.app-card-skeleton-icon {
  width: 4rem;
  height: 4rem;
  border-radius: 0.5rem;
  flex: 0 0 auto;
}

.app-card-skeleton-line {
  height: 0.75rem;
  border-radius: 999px;
}

.app-card-skeleton-button {
  height: 2.5rem;
  border-radius: 0.5rem;
}

@keyframes skeleton-shimmer {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(100%); }
}
</style>
