<template>
  <div class="apps-view pb-6">
    <!-- Nav header -- tabs + categories + search -->
    <div class="mb-4">
      <!-- Desktop: page tabs + category tabs + search -->
      <div ref="appsHeaderRef" class="app-header-desktop items-center gap-4 relative">
        <div ref="appsPrimaryRef" class="flex-shrink-0">
          <div class="mode-switcher hidden md:inline-flex">
            <button class="mode-switcher-btn" :class="{ 'mode-switcher-btn-active': activeTab === 'apps' }" @click="activeTab = 'apps'; router.replace({ query: {} })">My Apps</button>
            <RouterLink to="/dashboard/discover" class="mode-switcher-btn">App Store</RouterLink>
            <button class="mode-switcher-btn" :class="{ 'mode-switcher-btn-active': activeTab === 'services' }" @click="activeTab = 'services'; router.replace({ query: { tab: 'services' } })">Services</button>
          </div>
        </div>
        <div v-show="activeTab === 'apps' && categoriesWithApps.length > 1 && !collapseCategories" class="mode-switcher category-tabs-wide hidden md:inline-flex">
          <button
            v-for="category in categoriesWithApps"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mode-switcher-btn"
            :class="{ 'mode-switcher-btn-active': selectedCategory === category.id }"
          >{{ category.name }}</button>
        </div>
        <div v-show="activeTab === 'services' && serviceCategoriesWithItems.length > 1" class="mode-switcher category-tabs-wide hidden md:inline-flex">
          <button
            v-for="category in serviceCategoriesWithItems"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mode-switcher-btn"
            :class="{ 'mode-switcher-btn-active': selectedCategory === category.id }"
          >{{ category.name }}</button>
        </div>
        <div v-show="activeTab === 'apps' && categoriesWithApps.length > 1 && collapseCategories" class="segmented-select flex-shrink-0">
          <label class="sr-only" for="apps-category-select">My Apps category</label>
          <select
            id="apps-category-select"
            v-model="selectedCategory"
            class="segmented-select-control"
          >
            <option
              v-for="category in categoriesWithApps"
              :key="category.id"
              :value="category.id"
            >
              {{ category.name }}
            </option>
          </select>
        </div>
        <div ref="appsCategoryProbeRef" class="mode-switcher category-tabs-probe" aria-hidden="true">
          <button
            v-for="category in categoriesWithApps"
            :key="category.id"
            class="mode-switcher-btn"
            type="button"
          >
            {{ category.name }}
          </button>
        </div>
        <div class="app-header-search-wrap flex items-center gap-2">
          <AppSearchField v-model="searchQuery" :placeholder="t('apps.searchPlaceholder')" :label="t('apps.searchLabel')" />
          <button
            type="button"
            class="sideload-icon-btn"
            aria-label="Sideload app"
            title="Sideload app"
            @click="showSideload = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 16V4m0 0l-4 4m4-4l4 4M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Compact: tabs for tablet, categories, search + sideload -->
      <div class="app-header-mobile mb-4">
        <div class="app-header-inline-tabs mode-switcher mode-switcher-full mb-3">
          <button class="mode-switcher-btn" :class="{ 'mode-switcher-btn-active': activeTab === 'apps' }" @click="activeTab = 'apps'; router.replace({ query: {} })">My Apps</button>
          <RouterLink to="/dashboard/discover" class="mode-switcher-btn">App Store</RouterLink>
          <button class="mode-switcher-btn" :class="{ 'mode-switcher-btn-active': activeTab === 'services' }" @click="activeTab = 'services'; router.replace({ query: { tab: 'services' } })">Services</button>
        </div>
        <div v-if="activeTab === 'apps' && categoriesWithApps.length > 1" class="mobile-category-strip mb-3" aria-label="My Apps categories">
          <button
            v-for="category in categoriesWithApps"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mobile-category-pill"
            :class="{ 'mobile-category-pill-active': selectedCategory === category.id }"
            type="button"
          >{{ category.name }}</button>
        </div>
        <div v-if="activeTab === 'services' && serviceCategoriesWithItems.length > 1" class="mobile-category-strip mb-3" aria-label="Services categories">
          <button
            v-for="category in serviceCategoriesWithItems"
            :key="category.id"
            @click="selectedCategory = category.id"
            class="mobile-category-pill"
            :class="{ 'mobile-category-pill-active': selectedCategory === category.id }"
            type="button"
          >{{ category.name }}</button>
        </div>
        <div class="flex items-center gap-2">
          <AppSearchField v-model="searchQuery" :placeholder="t('apps.searchPlaceholder')" :label="t('apps.searchLabel')" />
          <button
            type="button"
            class="sideload-icon-btn sideload-icon-btn-mobile"
            aria-label="Sideload app"
            title="Sideload app"
            @click="showSideload = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 16V4m0 0l-4 4m4-4l4 4M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Loading Skeleton -->
    <div v-if="isLoadingApps" class="text-center py-16 pb-6">
      <div class="glass-card p-8 max-w-md mx-auto">
        <svg class="animate-spin h-8 w-8 mx-auto mb-4 text-white/70" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <h3 class="text-lg font-semibold text-white mb-2">Loading apps</h3>
        <p class="text-white/60 text-sm">Checking the latest app status before showing launch controls.</p>
      </div>
    </div>

    <!-- Connection Error -->
    <div v-else-if="connectionError && sortedPackageEntries.length === 0" class="text-center py-12 pb-6">
      <div class="glass-card p-8 max-w-md mx-auto">
        <div class="alert-error mb-4">{{ connectionError }}</div>
        <button
          @click="connectionError = ''; store.connectWebSocket()"
          class="glass-button px-6 py-3 rounded-lg font-medium"
        >
          Retry Connection
        </button>
      </div>
    </div>

    <!-- Container scanner still warming up -->
    <div v-else-if="isCheckingContainers" class="text-center py-16 pb-6">
      <div class="glass-card p-8 max-w-md mx-auto">
        <svg class="animate-spin h-8 w-8 mx-auto mb-4 text-white/70" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <h3 class="text-xl font-semibold text-white mb-2">Checking containers</h3>
        <p class="text-white/70">Archipelago is scanning installed apps. Your apps will appear here as soon as the container list is ready.</p>
      </div>
    </div>

    <!-- Empty State -->
    <div v-else-if="sortedPackageEntries.length === 0 && !searchQuery" class="text-center py-16 pb-6">
      <div class="glass-card p-12 max-w-md mx-auto">
        <svg class="w-16 h-16 mx-auto text-white/40 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
        </svg>
        <h3 class="text-xl font-semibold text-white mb-2">{{ t('apps.noAppsTitle') }}</h3>
        <p class="text-white/70 mb-6">{{ t('apps.noAppsMessage') }}</p>
        <RouterLink
          to="/dashboard/marketplace"
          class="inline-block glass-button px-6 py-3 rounded-lg font-medium transition-all hover:bg-black/70 hover:border-white/30"
        >
          {{ t('apps.browseAppStore') }}
        </RouterLink>
      </div>
    </div>

    <!-- No Results -->
    <div v-if="filteredPackageEntries.length === 0 && marketplaceMatches.length === 0 && searchQuery" class="text-center py-12">
      <p class="text-white/70">{{ t('apps.noResults', { query: searchQuery }) }}</p>
    </div>

    <div v-if="marketplaceMatches.length > 0" class="mb-5">
      <div class="flex items-center gap-3 mb-3">
        <span class="discover-terminal-tag">app store</span>
        <h2 class="text-lg font-bold text-white">Available in Discover</h2>
        <div class="flex-1 h-px bg-white/10"></div>
      </div>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <button
          v-for="app in marketplaceMatches"
          :key="app.id"
          type="button"
          class="glass-card p-4 text-left flex items-center gap-3 hover:bg-orange-500/5 hover:border-orange-500/15 transition-colors"
          @click="openMarketplaceResult(app)"
        >
          <img v-if="app.icon" :src="app.icon" :alt="app.title" class="w-12 h-12 rounded-xl object-cover bg-white/10" />
          <div v-else class="w-12 h-12 rounded-xl bg-white/10 flex-shrink-0"></div>
          <div class="min-w-0 flex-1">
            <p class="font-semibold text-white truncate">{{ app.title }}</p>
            <p class="text-xs text-white/50 truncate">Available in App Store</p>
          </div>
        </button>
      </div>
    </div>

    <div
      v-if="isUsingLastKnownPackages && filteredPackageEntries.length > 0"
      class="mb-4 rounded-lg border border-yellow-400/20 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-100/85 flex items-center gap-2"
    >
      <svg class="animate-spin h-4 w-4 flex-shrink-0 text-yellow-200/80" viewBox="0 0 24 24" fill="none">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
      <span>Refreshing container state. Showing the last known app list until the scan finishes.</span>
    </div>

    <!-- Mobile: iPhone-style icon grid -->
    <div class="apps-icon-grid-mobile">
      <AppIconGrid
        :apps="filteredPackageEntries as [string, PackageDataEntry][]"
        @go-to-app="goToApp"
      />
    </div>

    <!-- Desktop: Card grid -->
    <div class="apps-card-grid-desktop grid-cols-2 lg:grid-cols-3 gap-4 pb-6">
      <AppCard
        v-for="([id, pkg], index) in filteredPackageEntries"
        :key="id"
        :id="id as string"
        :pkg="pkg"
        :index="index"
        :show-stagger="showStagger"
        :is-loading="!!actions.loadingActions.value[id as string]"
        :is-installing="serverStore.isInstalling(id as string)"
        :install-progress="serverStore.installingApps.get(id as string)"
        :is-uninstalling="actions.uninstallingApps.has(id as string)"
        @go-to-app="goToApp"
        @launch="launchApp"
        @start="actions.startApp"
        @stop="actions.stopApp"
        @restart="actions.restartApp"
        @update="updateApp"
        @show-uninstall="showUninstallModal"
      />
    </div>

    <AppsUninstallModal
      :show="uninstallModal.show"
      :app-id="uninstallModal.appId"
      :app-title="uninstallModal.appTitle"
      :uninstalling="actions.uninstalling.value"
      @close="closeUninstallModal"
      @confirm="onConfirmUninstall"
    />

    <!-- House modal (BaseModal glass-card), not a hand-rolled panel: the old
         one painted its own rgba(8,10,18,.98) navy card, which read as blue
         against every other modal in the app. BaseModal also brings the
         standard scroll contract, Esc/focus handling and body scroll lock. -->
    <BaseModal
      :show="credentialModal.show"
      :title="credentialModal.title"
      max-width="max-w-lg"
      z-index="z-[2700]"
      @close="closeCredentialModal"
    >
      <p v-if="credentialModal.description" class="text-sm text-white/55 -mt-1 mb-4">
        {{ credentialModal.description }}
      </p>
      <div class="space-y-3">
        <div v-for="cred in credentialModal.credentials" :key="cred.label" class="rounded-lg border border-white/10 bg-white/[0.04] p-3">
          <div class="flex items-center justify-between gap-3 mb-1">
            <span class="text-white/60 text-xs uppercase tracking-wide">{{ cred.label }}</span>
            <button type="button" class="text-xs text-orange-300 hover:text-orange-200" @click="copyModalCredential(cred.label, cred.value)">{{ credentialModal.copied === cred.label ? 'Copied' : 'Copy' }}</button>
          </div>
          <p class="font-mono text-sm text-white break-all">{{ cred.value }}</p>
        </div>
      </div>
      <template #footer>
        <div class="flex flex-col sm:flex-row gap-3">
          <button type="button" class="w-full sm:flex-1 glass-button px-4 py-3 rounded-lg" @click="closeCredentialModal">Cancel</button>
          <button type="button" class="w-full sm:flex-1 glass-button px-4 py-3 rounded-lg font-semibold" @click="continueCredentialLaunch">Continue to app</button>
        </div>
      </template>
    </BaseModal>

    <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="showSideload"
        class="fixed inset-0 z-[2600] flex items-end justify-center bg-black/60 backdrop-blur-md p-0 md:items-center md:p-6"
        @click.self="closeSideload"
      >
        <form class="sideload-modal" @submit.prevent="submitSideload">
          <div class="flex items-start justify-between gap-4 mb-5">
            <div>
              <h2 class="text-lg font-semibold text-white">Sideload app</h2>
              <p class="text-sm text-white/55 mt-1">Install a trusted Docker image with a simple web UI.</p>
            </div>
            <button type="button" class="sideload-close-btn" aria-label="Close" @click="closeSideload">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="space-y-4">
            <label class="block">
              <span class="sideload-label">App ID</span>
              <input v-model.trim="sideloadForm.id" class="sideload-input" placeholder="excalidraw" pattern="[a-z0-9][a-z0-9-]{0,63}" required />
            </label>
            <label class="block">
              <span class="sideload-label">Docker image</span>
              <input v-model.trim="sideloadForm.image" class="sideload-input" placeholder="docker.io/excalidraw/excalidraw:latest" required />
            </label>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <label class="block">
                <span class="sideload-label">Title</span>
                <input v-model.trim="sideloadForm.title" class="sideload-input" placeholder="Excalidraw" />
              </label>
              <label class="block">
                <span class="sideload-label">Port mapping</span>
                <input v-model.trim="sideloadForm.port" class="sideload-input" placeholder="3009:80" />
              </label>
            </div>
            <label class="block">
              <span class="sideload-label">Description</span>
              <input v-model.trim="sideloadForm.description" class="sideload-input" placeholder="Collaborative whiteboard" />
            </label>
          </div>

          <div v-if="sideloadError" class="alert-error mt-4 text-sm">{{ sideloadError }}</div>

          <div class="mt-5 rounded-xl border border-white/10 bg-white/[0.04] p-4 text-sm text-white/65">
            <p class="font-medium text-white/80 mb-2">Easy sources</p>
            <p>Use images from Docker Hub, GHCR, the Archipelago app registry, or localhost. Good first candidates: Excalidraw, Stirling PDF, FreshRSS, Wallabag, HedgeDoc, CyberChef, Mealie, or PairDrop.</p>
          </div>

          <div class="mt-5 flex gap-3">
            <button type="button" class="flex-1 glass-button px-4 py-3 rounded-lg" @click="closeSideload">Cancel</button>
            <button type="submit" class="flex-1 glass-button px-4 py-3 rounded-lg font-semibold" :disabled="sideloading">
              {{ sideloading ? 'Installing...' : 'Install' }}
            </button>
          </div>
        </form>
      </div>
    </Transition>
    </Teleport>

    <!-- Action error toast -->
    <Transition name="fade">
      <div v-if="actions.actionError.value" class="fixed bottom-20 left-1/2 -translate-x-1/2 z-50 max-w-md w-full px-4" role="alert" aria-live="assertive">
        <div class="alert-error backdrop-blur-sm rounded-lg px-4 py-3 text-sm flex items-center justify-between gap-3">
          <span>{{ actions.actionError.value }}</span>
          <button @click="actions.actionError.value = ''" :aria-label="t('apps.dismissError')" class="text-red-300 hover:text-white shrink-0">&times;</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script lang="ts">
// Module-level -- persists across component unmount/remount within same session
let appsAnimationDone = false
</script>

<script setup lang="ts">
import AppSearchField from '@/components/AppSearchField.vue'
import { computed, ref, watch, onActivated, onBeforeUnmount, onDeactivated, onMounted } from 'vue'
import { useRouter, useRoute, RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { useServerStore } from '@/stores/server'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { rpcClient } from '@/api/rpc-client'
import { type AppCredential, type AppCredentialsResponse, type PackageDataEntry, type PackageState } from '@/types/api'
import BaseModal from '@/components/BaseModal.vue'
import AppCard from './apps/AppCard.vue'
import AppIconGrid from './apps/AppIconGrid.vue'
import AppsUninstallModal from './apps/AppsUninstallModal.vue'
import { resolveAppCredentials } from './apps/appCredentials'
import { useLastKnownPackages } from './apps/appPackageCache'
import { useAppsActions } from './apps/useAppsActions'
import { validateSideloadRequest } from './apps/sideloadValidation'
import { useMarketplaceApp } from '@/composables/useMarketplaceApp'
import { useCollapsingHeaderTabs } from '@/composables/useCollapsingHeaderTabs'
import { useContainersScanTimeout } from '@/composables/useContainersScanTimeout'
import {
  type AppsTab, filterEntriesForTab, isWebOnlyApp, isWebsitePackage, opensInTab, resolveRuntimeLaunchUrl,
  WEB_ONLY_APPS, WEB_ONLY_APP_URLS, buildAllCategories, useCategoriesWithApps,
  buildServiceCategories, useServiceCategories,
} from './apps/appsConfig'
import { getCuratedAppList, INSTALLED_ALIASES, type MarketplaceApp } from './marketplace/marketplaceData'
import { IS_DEMO, isDemoApp } from '@/composables/useDemoIntro'

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const store = useAppStore()
const serverStore = useServerStore()
const actions = useAppsActions()
const { setCurrentApp } = useMarketplaceApp()
const showSideload = ref(false)
const sideloading = ref(false)
const sideloadError = ref('')
const sideloadForm = ref({
  id: '',
  image: '',
  title: '',
  port: '',
  description: '',
})
const credentialModal = ref({
  show: false,
  appId: '',
  title: '',
  description: '',
  credentials: [] as AppCredential[],
  copied: '',
})

// Only stagger-animate on first mount
const showStagger = !appsAnimationDone

// Tabs
const activeTab = ref<AppsTab>(
  route.query.tab === 'websites' || route.query.tab === 'services' ? 'services' : 'apps'
)

watch(() => route.query.tab, (tab) => {
  activeTab.value = tab === 'websites' || tab === 'services' ? 'services' : 'apps'
})

// Search (debounced)
const searchQuery = ref('')
const debouncedSearchQuery = ref('')
let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined
watch(searchQuery, (val) => {
  clearTimeout(searchDebounceTimer)
  searchDebounceTimer = setTimeout(() => { debouncedSearchQuery.value = val }, 150)
})
onBeforeUnmount(() => { clearTimeout(searchDebounceTimer) })

// Category filter (shared by My Apps and Services; reset when switching tabs so
// an apps-category selection never carries into the Services sub-nav).
const selectedCategory = ref('all')
watch(activeTab, () => { selectedCategory.value = 'all' })

const ALL_CATEGORIES = computed(() => buildAllCategories(t))
const SERVICE_CATEGORIES = computed(() => buildServiceCategories(t))

const livePackages = computed(() => store.packages || {})
// Field missing from server data = not scanned yet (consistent with Discover/Marketplace)
const containersScannedRaw = computed(() => store.data?.['server-info']?.['status-info']?.['containers-scanned'] ?? false)
// Escape hatch: never show "Checking containers…" forever — after a timeout,
// fall through to the real (empty) state even if the scanned flag never arrives.
const { effectiveContainersScanned: containersScanned } = useContainersScanTimeout(
  containersScannedRaw,
  computed(() => store.hasLoadedInitialData),
)
const {
  packages: stablePackages,
  isUsingLastKnownPackages,
} = useLastKnownPackages(livePackages, containersScanned)

// Merge real packages from store with web-only app bookmarks + installing placeholders
const packages = computed(() => {
  const realPackages = stablePackages.value
  const merged: Record<string, PackageDataEntry> = { ...WEB_ONLY_APPS, ...realPackages }

  // Inject placeholder entries for apps being installed that aren't in backend data yet
  for (const [appId, progress] of serverStore.installingApps) {
    if (!merged[appId]) {
      merged[appId] = {
        state: 'installing' as PackageState,
        manifest: {
          id: appId,
          title: progress.title,
          version: '',
          description: { short: '', long: '' },
          'release-notes': '', license: '', 'wrapper-repo': '', 'upstream-repo': '',
          'support-site': '', 'marketing-site': '', 'donation-url': null,
        },
        'static-files': { license: '', instructions: '', icon: '' },
      }
    }
  }

  return merged
})

const categoriesWithApps = useCategoriesWithApps(packages, ALL_CATEGORIES)
const serviceCategoriesWithItems = useServiceCategories(packages, SERVICE_CATEGORIES)
const appsHeaderRef = ref<HTMLElement | null>(null)
const appsPrimaryRef = ref<HTMLElement | null>(null)
const appsCategoryProbeRef = ref<HTMLElement | null>(null)
const { collapsed: collapseCategories } = useCollapsingHeaderTabs(
  appsHeaderRef,
  appsPrimaryRef,
  appsCategoryProbeRef,
  144
)

const curatedApps = getCuratedAppList()
const marketplaceMatches = computed(() => {
  const q = debouncedSearchQuery.value.trim().toLowerCase()
  if (!q || activeTab.value !== 'apps') return [] as MarketplaceApp[]
  return curatedApps.filter(app => {
    if (isInstalledInMyApps(app.id)) return false
    return app.title?.toLowerCase().includes(q) ||
      app.id.toLowerCase().includes(q) ||
      app.author?.toLowerCase().includes(q) ||
      (typeof app.description === 'string' && app.description.toLowerCase().includes(q))
  }).slice(0, 6)
})

const isLoadingApps = computed(() => !store.hasLoadedInitialData && !connectionError.value)
const isCheckingContainers = computed(() => (
  store.hasLoadedInitialData &&
  Object.keys(livePackages.value).length === 0 &&
  !isUsingLastKnownPackages.value &&
  sortedPackageEntries.value.length === 0 &&
  !searchQuery.value &&
  !containersScanned.value
))

// Connection error state
const connectionError = ref('')
let connectionTimer: ReturnType<typeof setTimeout> | undefined

// Entry-scoped guard: under a surviving instance the 15s "unable to connect"
// timer would otherwise arm once on the first visit and never again, so its
// diagnosis of a fresh entry would go stale. Re-armed on every activation
// (clearing any prior handle first — idempotent, so two consecutive
// activations never double-arm) and cleared on deactivation; the
// onBeforeUnmount clear stays for the non-cached mount path.
function armConnectionGuard() {
  if (connectionTimer) clearTimeout(connectionTimer)
  // A stale error from a previous visit must not persist into a fresh entry
  // — otherwise a since-reconnected node would keep showing "Unable to
  // connect" instantly (no fresh 15s grace period) once this instance
  // survives deactivation instead of fully remounting.
  connectionError.value = ''
  if (!store.isConnected) {
    connectionTimer = setTimeout(() => {
        if (!store.hasLoadedInitialData && sortedPackageEntries.value.length === 0) {
        connectionError.value = 'Unable to connect to server. Check that the backend is running.'
      }
    }, 15000)
  }
}
onActivated(() => armConnectionGuard())

// Once-per-session: the intro stagger flag. Also arms the connection guard
// directly here — onActivated is a no-op outside a <KeepAlive> boundary, so
// a bare mount (a unit test, or any future non-KeepAlive usage) must not
// silently skip it. armConnectionGuard() is idempotent, so the harmless
// extra pass this causes on a KeepAlive-wrapped first mount costs nothing.
onMounted(() => {
  appsAnimationDone = true
  armConnectionGuard()
})

onDeactivated(() => {
  if (connectionTimer) clearTimeout(connectionTimer)
})

onBeforeUnmount(() => {
  if (connectionTimer) clearTimeout(connectionTimer)
})

// Sorted entries: web-only first, then alphabetical by title
const sortedPackageEntries = computed(() => {
  const entries = Object.entries(packages.value)
  const filtered = filterEntriesForTab(entries, activeTab.value, selectedCategory.value)
  return filtered.sort(([idA, a], [idB, b]) => {
    const aWeb = isWebOnlyApp(idA) ? 0 : 1
    const bWeb = isWebOnlyApp(idB) ? 0 : 1
    if (aWeb !== bWeb) return aWeb - bWeb
    return (a.manifest?.title ?? '').localeCompare(b.manifest?.title ?? '', undefined, { sensitivity: 'base' })
  })
})

const filteredPackageEntries = computed(() => {
  if (!debouncedSearchQuery.value) return sortedPackageEntries.value
  const q = debouncedSearchQuery.value.toLowerCase()
  return sortedPackageEntries.value.filter(([id, pkg]) =>
    (pkg.manifest?.title ?? '').toLowerCase().includes(q) ||
    (pkg.manifest?.description?.short ?? '').toLowerCase().includes(q) ||
    id.toLowerCase().includes(q)
  )
})

function isInstalledInMyApps(appId: string): boolean {
  if (appId in packages.value) return true
  const aliases = INSTALLED_ALIASES[appId]
  return aliases ? aliases.some(alias => alias in packages.value) : false
}

function openMarketplaceResult(app: MarketplaceApp) {
  setCurrentApp(app)
  router.push({ name: 'marketplace-app-detail', params: { id: app.id }, query: { from: 'apps' } }).catch(() => {})
}

// Uninstall modal
const uninstallModal = ref({ show: false, appId: '', appTitle: '' })

function showUninstallModal(id: string, pkg: PackageDataEntry) {
  uninstallModal.value = { show: true, appId: id, appTitle: pkg.manifest.title }
}

function closeUninstallModal() {
  uninstallModal.value.show = false
}

async function onConfirmUninstall(deleteAppData: boolean) {
  const { appId } = uninstallModal.value
  // Close the modal immediately so the user can fire off concurrent
  // uninstalls. Each AppCard surfaces its own live stage label while
  // its uninstall is in flight.
  uninstallModal.value.show = false
  await actions.confirmUninstall(appId, { preserveData: !deleteAppData })
}

function goToApp(id: string) {
  router.push(`/dashboard/apps/${id}`).catch(() => {})
}

async function launchApp(id: string) {
  const shown = await maybeShowCredentialsBeforeLaunch(id)
  if (shown) return
  launchAppNow(id, true)
}

function launchAppNow(id: string, credentialsChecked = false) {
  const pkg = packages.value[id]
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768
  const webOnlyUrl = WEB_ONLY_APP_URLS[id]
  if (pkg && webOnlyUrl) {
    useAppLauncherStore().open({ url: webOnlyUrl, title: pkg.manifest.title, openInNewTab: !isMobile })
    return
  }
  if (pkg && isWebsitePackage(id, pkg)) {
    const url = resolveRuntimeLaunchUrl(pkg)
    if (url) {
      useAppLauncherStore().open({ url, title: pkg.manifest.title, openInNewTab: !isMobile })
    }
    return
  }
  // Demo: demoable apps are served same-origin by the mock backend, so the
  // tab-launch list (real apps with framing headers) doesn't apply.
  if (!isMobile && pkg && opensInTab(id) && !(IS_DEMO && isDemoApp(id))) {
    const url = resolveRuntimeLaunchUrl(pkg)
    if (url) {
      window.open(url, '_blank', 'noopener,noreferrer')
      return
    }
  }
  useAppLauncherStore().openSession(id, { skipCredentialPrompt: credentialsChecked })
}

// Per-app credentials memo: the pre-launch RPC could hold an Apps-tab launch
// hostage for its full 5s timeout over the mesh (home-card launches skip this
// gate entirely, which is why they always felt instant). First launch waits at
// most LAUNCH_CRED_BUDGET_MS; the RPC keeps running in the background and its
// answer is memoized, so every later launch of that app resolves instantly.
const LAUNCH_CRED_BUDGET_MS = 1200
const credentialsCache = new Map<string, AppCredentialsResponse | null>()

function fetchCredentials(id: string): Promise<AppCredentialsResponse | null> {
  return rpcClient
    .call<AppCredentialsResponse>({
      method: 'package.credentials',
      params: { app_id: id },
      timeout: 5000,
    })
    .then((r) => {
      credentialsCache.set(id, r)
      return r
    })
    .catch(() => {
      credentialsCache.set(id, null)
      return null
    })
}

async function maybeShowCredentialsBeforeLaunch(id: string): Promise<boolean> {
  const result = credentialsCache.has(id)
    ? credentialsCache.get(id) ?? null
    : await Promise.race([
        fetchCredentials(id),
        // Budget exceeded → launch with the static fallback config; the
        // in-flight RPC still lands in the cache for next time.
        new Promise<null>((resolve) => setTimeout(() => resolve(null), LAUNCH_CRED_BUDGET_MS)),
      ])
  const credentials = resolveAppCredentials(id, result)
  if (!credentials) return false
  credentialModal.value = {
    show: true,
    appId: id,
    title: credentials.title || `${packages.value[id]?.manifest.title || id} credentials`,
    description: credentials.description || 'Use these credentials when the app asks you to sign in.',
    credentials: credentials.credentials,
    copied: '',
  }
  return true
}

function closeCredentialModal() {
  credentialModal.value.show = false
}

function continueCredentialLaunch() {
  const id = credentialModal.value.appId
  closeCredentialModal()
  if (id) launchAppNow(id, true)
}

async function copyModalCredential(label: string, value: string) {
  try {
    await navigator.clipboard.writeText(value)
  } catch {
    const textarea = document.createElement('textarea')
    textarea.value = value
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
  }
  credentialModal.value.copied = label
}

async function updateApp(id: string) {
  try {
    await serverStore.updatePackage(id)
  } catch (err) {
    actions.actionError.value = `Failed to update ${id}: ${err instanceof Error ? err.message : 'Unknown error'}`
  }
}

function closeSideload() {
  if (sideloading.value) return
  showSideload.value = false
  sideloadError.value = ''
}

function inferPortMapping(image: string): string {
  const lower = image.toLowerCase()
  if (lower.includes('excalidraw')) return '3009:80'
  if (lower.includes('stirling')) return '3011:8080'
  if (lower.includes('freshrss')) return '3012:80'
  if (lower.includes('wallabag')) return '3013:80'
  if (lower.includes('hedgedoc')) return '3014:3000'
  if (lower.includes('cyberchef')) return '3015:80'
  if (lower.includes('mealie')) return '3016:9000'
  if (lower.includes('pairdrop')) return '3017:3000'
  return ''
}

async function submitSideload() {
  const id = sideloadForm.value.id.trim().toLowerCase()
  const image = sideloadForm.value.image.trim()
  const title = sideloadForm.value.title.trim() || id.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
  const port = sideloadForm.value.port.trim() || inferPortMapping(image)
  if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(id)) {
    sideloadError.value = 'Use lowercase letters, numbers, and hyphens only.'
    return
  }
  if (!image || !image.includes('/')) {
    sideloadError.value = 'Enter a full image name, for example docker.io/library/nginx:alpine.'
    return
  }
  const validationError = validateSideloadRequest(id, port, store.packages)
  if (validationError) {
    sideloadError.value = validationError
    return
  }
  sideloading.value = true
  sideloadError.value = ''
  const containerConfig: Record<string, unknown> = {}
  containerConfig.title = title
  if (sideloadForm.value.description.trim()) containerConfig.description = sideloadForm.value.description.trim()
  if (port) containerConfig.ports = [port]
  try {
    serverStore.setInstallProgress(id, {
      id,
      title,
      status: 'downloading',
      progress: 2,
      message: 'Sideload queued...',
      attempt: 0,
    })
    await rpcClient.call({
      method: 'package.install',
      params: {
        id,
        dockerImage: image,
        version: 'sideload',
        containerConfig,
      },
      timeout: 600000,
    })
    closeSideload()
    sideloadForm.value = { id: '', image: '', title: '', port: '', description: '' }
  } catch (err) {
    sideloadError.value = err instanceof Error ? err.message : 'Install failed'
    serverStore.setInstallProgress(id, {
      id,
      title,
      status: 'error',
      progress: 0,
      message: sideloadError.value,
      attempt: 0,
    })
  } finally {
    sideloading.value = false
  }
}
</script>

<style scoped>
.sideload-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  border-radius: 0.5rem;
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.78);
  transition: border-color 0.15s ease, background 0.15s ease, color 0.15s ease;
}
.sideload-icon-btn:hover,
.sideload-icon-btn:focus-visible {
  border-color: rgba(255, 255, 255, 0.38);
  background: rgba(255, 255, 255, 0.15);
  color: white;
}
.sideload-icon-btn-mobile {
  width: 52px;
  height: 52px;
}
.sideload-modal {
  width: 100%;
  max-width: 34rem;
  max-height: calc(100dvh - var(--safe-area-top, env(safe-area-inset-top, 0px)) - 12px);
  overflow-y: auto;
  border: 1px solid rgba(255, 255, 255, 0.14);
  /* Bottom sheet on mobile (flush with the screen edge, so only the top
     corners round); the md: breakpoint below switches to a centered
     floating card, matching the wrapper's own items-end -> md:items-center
     layout switch, so it gets fully rounded corners like every other modal. */
  border-radius: 1.5rem 1.5rem 0 0;
  background: rgba(8, 10, 18, 0.94);
  padding: 1.25rem;
  padding-bottom: calc(1.25rem + var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)));
  box-shadow: 0 -24px 70px rgba(0, 0, 0, 0.55);
}
@media (min-width: 768px) {
  .sideload-modal {
    border-radius: 1.5rem;
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.55);
  }
}
.sideload-close-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.75rem;
  color: rgba(255, 255, 255, 0.7);
  background: transparent;
  transition: background-color 0.15s ease, color 0.15s ease;
}
.sideload-close-btn:hover,
.sideload-close-btn:focus-visible {
  background: rgba(255, 255, 255, 0.1);
  color: white;
}
.sideload-label {
  display: block;
  margin-bottom: 0.4rem;
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.62);
}
.sideload-input {
  width: 100%;
  border-radius: 0.75rem;
  border: 1px solid rgba(255, 255, 255, 0.16);
  background: rgba(255, 255, 255, 0.08);
  padding: 0.75rem 0.9rem;
  color: white;
  outline: none;
}
.sideload-input::placeholder { color: rgba(255, 255, 255, 0.38); }
.sideload-input:focus { border-color: rgba(255, 255, 255, 0.38); }
</style>
