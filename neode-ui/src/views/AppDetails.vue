<template>
  <div class="app-details-container pb-16 md:pb-16">
    <BackButton :label="backButtonText" desktop-margin="mb-6" @click="goBack" />

    <div v-if="pkg">
      <AppHeroSection
        :pkg="pkg"
        :app-id="appId"
        :package-key="packageKey"
        :can-launch="canLaunch"
        :is-web-only="isWebOnly"
        :pending-action="pendingAction"
        @launch="launchApp"
        @start="startApp"
        @stop="stopApp"
        @restart="restartApp"
        @uninstall="uninstallApp"
        @update="updateApp"
        @channels="router.push('/dashboard/apps/lnd/channels')"
      />

      <LndSeedBackup v-if="packageKey === 'lnd' && pkg.installed" />

      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <AppContentSection
          :pkg="pkg"
          :features="features"
          :needs-bitcoin-sync="needsBitcoinSync"
          :bitcoin-synced="bitcoinSynced"
          :bitcoin-sync-percent="bitcoinSyncPercent"
          :bitcoin-block-height="bitcoinBlockHeight"
        />

        <AppSidebar
          :pkg="pkg"
          :package-key="packageKey"
          :is-web-only="isWebOnly"
          :gateway-state="gatewayState"
          :interface-addresses="interfaceAddresses"
          :lan-url="lanUrl"
          :tor-url="torUrl"
          :show-tor-address="showTorAddress"
          :credentials="credentials"
          :credentials-loading="credentialsLoading"
        />
      </div>
    </div>

    <!-- App Not Found -->
    <div v-else class="glass-card p-12 text-center">
      <svg class="w-24 h-24 text-white/20 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <h3 class="text-2xl font-semibold text-white mb-2">{{ t('appDetails.notFoundTitle') }}</h3>
      <p class="text-white/70">{{ t('appDetails.notFoundMessage') }}</p>
    </div>

    <AppsUninstallModal
      :show="uninstallModal.show"
      :app-title="uninstallModal.appTitle"
      :uninstalling="pendingAction === 'uninstall'"
      @close="closeUninstallModal"
      @confirm="confirmUninstall"
    />

    <!-- Action error toast -->
    <Transition name="fade">
      <div v-if="actionError" class="fixed bottom-20 left-1/2 -translate-x-1/2 z-50 max-w-md w-full px-4" role="alert" aria-live="assertive">
        <div class="alert-error backdrop-blur-sm rounded-lg px-4 py-3 text-sm flex items-center justify-between gap-3">
          <span>{{ actionError }}</span>
          <button @click="actionError = ''" :aria-label="t('apps.dismissError')" class="text-red-300 hover:text-white shrink-0">&times;</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '../stores/app'
import { useAppLauncherStore } from '../stores/appLauncher'
import { dummyApps } from '../utils/dummyApps'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import type { AppCredentialsResponse } from '@/types/api'
import BackButton from '@/components/BackButton.vue'
import AppHeroSection from './appDetails/AppHeroSection.vue'
import AppContentSection from './appDetails/AppContentSection.vue'
import AppSidebar from './appDetails/AppSidebar.vue'
import LndSeedBackup from './appDetails/LndSeedBackup.vue'
import AppsUninstallModal from './apps/AppsUninstallModal.vue'
import { resolveAppUrl } from './appSession/appSessionConfig'
import { resolveAppCredentials } from './apps/appCredentials'
import { isWebsitePackage, resolveRuntimeLaunchUrl } from './apps/appsConfig'
import {
  WEB_ONLY_APP_URLS,
  PACKAGE_ALIASES,
  BITCOIN_DEPENDENT_APPS,
  resolvePackageKey,
  isRealOnionAddress,
} from './appDetails/appDetailsData'

const router = useRouter()
const route = useRoute()
const store = useAppStore()
const { t } = useI18n()

const appId = computed(() => {
  const id = route.params.id
  if (typeof id !== 'string' || !/^[a-z0-9][a-z0-9._-]*$/.test(id) || id.length > 64) {
    router.replace('/dashboard/apps')
    return ''
  }
  return id
})

const isWebOnly = computed(() => appId.value in WEB_ONLY_APP_URLS)

const pkg = computed(() => {
  const routeId = appId.value
  const pkgKey = resolvePackageKey(routeId)
  if (store.packages[pkgKey]) return store.packages[pkgKey]
  if (store.packages[routeId]) return store.packages[routeId]
  const aliases = PACKAGE_ALIASES[routeId]
  if (aliases) {
    for (const alias of aliases) {
      if (store.packages[alias]) return store.packages[alias]
    }
  }
  if (dummyApps[routeId]) return dummyApps[routeId]
  return null
})

const interfaceAddresses = computed(() => {
  const main = pkg.value?.installed?.['interface-addresses']?.main
  if (!main) return null
  if (!main['lan-address'] && !isRealOnionAddress(main['tor-address'])) return null
  return main
})

const lanUrl = computed(() => {
  const addr = interfaceAddresses.value?.['lan-address']
  if (!addr) return '#'
  if (addr.includes('localhost')) return addr.replace('localhost', window.location.hostname)
  return addr
})

const torUrl = computed(() => {
  const addr = interfaceAddresses.value?.['tor-address']
  if (!addr || !isRealOnionAddress(addr)) return ''
  return addr.startsWith('http') ? addr : `http://${addr}`
})

const showTorAddress = computed(() => isRealOnionAddress(interfaceAddresses.value?.['tor-address']))

const packageKey = computed(() => resolvePackageKey(appId.value))

const gatewayState = computed(() => {
  const gw = store.packages['fedimint-gateway']
  return gw ? gw.state : 'not installed'
})

const needsBitcoinSync = computed(() => BITCOIN_DEPENDENT_APPS.includes(packageKey.value))

// Keyed per app id (D-04): AppDetails is never instance-cached (no
// KeepAlive), but the route's `:key="route.path"` (DashboardRouterView.vue)
// means an id change always fully remounts this component, so the key can be
// computed once at setup time rather than re-derived via a watch.
const bitcoinSyncResource = useCachedResource<{ block_height: number; sync_progress: number }>({
  key: `app-details:bitcoin-sync:${appId.value}`,
  fetcher: (signal) => rpcClient.call<{ block_height: number; sync_progress: number }>({
    method: 'bitcoin.getinfo',
    signal,
    dedup: true,
    timeout: 5000,
  }),
  ttlMs: 30_000, // install/health-state-shaped data, per plan default
  persist: true, // public chain height/sync progress — no money amount or identity
  immediate: false, // kicked from onMounted, gated on needsBitcoinSync
})
const bitcoinSyncPercent = computed(() => (bitcoinSyncResource.data.value?.sync_progress ?? 0) * 100)
const bitcoinBlockHeight = computed(() => bitcoinSyncResource.data.value?.block_height ?? 0)
const bitcoinSynced = computed(() => bitcoinSyncPercent.value >= 99.9)

// Credential material — memory-only (persist: false), never written to
// sessionStorage (D-08 / T-02-01).
const credentialsResource = useCachedResource<AppCredentialsResponse | null>({
  key: `app-details:credentials:${appId.value}`,
  fetcher: async (signal) => {
    const result = await rpcClient.call<AppCredentialsResponse>({
      method: 'package.credentials',
      params: { app_id: packageKey.value },
      signal,
      dedup: true,
      timeout: 5000,
    })
    return resolveAppCredentials(packageKey.value, result)
  },
  ttlMs: 30_000,
  persist: false,
  immediate: false,
})
const credentials = computed(() => credentialsResource.data.value ?? resolveAppCredentials(packageKey.value, null))
const credentialsLoading = computed(() => credentialsResource.loadState.value === 'loading')

// refresh() is unconditional (force-fetch); only call it when the cached
// entry is missing or past its TTL, so a repeat open inside the TTL paints
// from cache with no new RPC.
function loadBitcoinSync() {
  if (!needsBitcoinSync.value) return
  if (bitcoinSyncResource.data.value === null || bitcoinSyncResource.isStale.value) {
    void bitcoinSyncResource.refresh()
  }
}

function loadCredentials() {
  if (!appId.value) return
  if (credentialsResource.data.value === null || credentialsResource.isStale.value) {
    void credentialsResource.refresh()
  }
}

const pendingAction = ref<'start' | 'stop' | 'restart' | 'update' | 'uninstall' | null>(null)

// Both loaders are independent (bitcoin sync state vs. this app's
// credentials) and were already fire-and-forget here before this plan — do
// not "fix" this into an awaited chain, it is already effectively parallel.
onMounted(() => {
  loadBitcoinSync()
  loadCredentials()
})

const actionError = ref('')
let errorTimer: ReturnType<typeof setTimeout> | undefined

function showActionError(msg: string) {
  actionError.value = msg
  if (errorTimer) clearTimeout(errorTimer)
  errorTimer = setTimeout(() => { actionError.value = '' }, 5000)
}

const uninstallModal = ref({ show: false, appTitle: '' })

function closeUninstallModal() {
  uninstallModal.value.show = false
}

const backButtonText = computed(() => {
  if (route.query.from === 'discover') return 'Back to Discover'
  if (route.query.from === 'marketplace') return t('appDetails.backToStore')
  return t('appDetails.backToApps')
})

const canLaunch = computed(() => {
  if (!pkg.value) return false
  if (isWebOnly.value) return true
  const hasRuntimeAddress = !!pkg.value.installed?.['interface-addresses']?.main?.['lan-address']
  const hasKnownLaunchUrl = typeof window !== 'undefined' && !!resolveAppUrl(pkg.value.manifest.id)
  const hasUI = !!(pkg.value.manifest.interfaces?.main?.ui || hasRuntimeAddress || hasKnownLaunchUrl)
  return hasUI && pkg.value.state === 'running' && pkg.value.health !== 'starting' && pkg.value.health !== 'unhealthy'
})

const features = computed(() => {
  if (packageKey.value === 'archipelago-source') {
    return [
      'Browse Archipelago source through the established GitWorkshop interface',
      'Clone with ngit and fetch Git objects from redundant GRASP servers',
      'Open issues, propose patches, and review changes over Nostr',
      'Use a selected node identity through an explicit consent prompt',
      'Contribute without receiving maintainer merge or release authority',
    ]
  }
  return [
    'Self-hosted and privacy-focused',
    'Easy installation and updates',
    'Automatic backups',
    'Secure by default',
  ]
})

function goBack() {
  if (route.query.from === 'discover') {
    router.push('/dashboard/discover').catch(() => {})
    return
  }
  if (route.query.from === 'marketplace') {
    router.push('/dashboard/marketplace').catch(() => {})
    return
  }
  router.push('/dashboard/apps').catch(() => {})
}

function launchApp() {
  if (!pkg.value) return
  const id = appId.value
  const isMobile = typeof window !== 'undefined' && window.innerWidth < 768

  const webOnlyUrl = WEB_ONLY_APP_URLS[id]
  if (webOnlyUrl) {
    useAppLauncherStore().open({ url: webOnlyUrl, title: pkg.value.manifest.title, openInNewTab: !isMobile })
    return
  }

  if (isWebsitePackage(id, pkg.value)) {
    const url = resolveRuntimeLaunchUrl(pkg.value)
    if (url) {
      useAppLauncherStore().open({ url, title: pkg.value.manifest.title, openInNewTab: !isMobile })
    }
    return
  }

  const runtimeUrl = resolveRuntimeLaunchUrl(pkg.value)
  if (runtimeUrl) {
    useAppLauncherStore().open({ url: runtimeUrl, title: pkg.value.manifest.title })
    return
  }

  // Container apps should launch through session routing so protocol/path
  // handling stays centralized in appSessionConfig.
  useAppLauncherStore().openSession(id)
}


/**
 * Hold the just-clicked action until the node's own state confirms it picked
 * the work up (or we give up waiting).
 *
 * The lifecycle RPCs return in milliseconds and do the real work in the
 * background, so clearing `pendingAction` on the promise left the buttons idle
 * while the app was still down — the operator sees a flash and assumes the
 * click did nothing. The hero section keeps the spinner running off the
 * backend's transitional state; this only has to bridge the gap until that
 * first state push lands. The timeout means a node that never reports back
 * still releases the controls instead of wedging them.
 */
async function holdUntilBackendPicksUp(timeoutMs = 12000) {
  const started = Date.now()
  while (Date.now() - started < timeoutMs) {
    const s = pkg.value?.state
    if (s === 'starting' || s === 'stopping' || s === 'restarting' || s === 'updating') return
    await new Promise((r) => setTimeout(r, 250))
  }
}

async function startApp() {
  pendingAction.value = 'start'
  try {
    await store.startPackage(appId.value)
    await holdUntilBackendPicksUp()
  } catch (err) {
    showActionError(`Failed to start: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    pendingAction.value = null
  }
}

async function stopApp() {
  pendingAction.value = 'stop'
  try {
    await store.stopPackage(appId.value)
    // Stopping the app can take its admin credentials offline — invalidate
    // rather than show a stale "healthy" credentials card (T-02-12).
    credentialsResource.invalidate()
    await holdUntilBackendPicksUp()
  } catch (err) {
    showActionError(`Failed to stop: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    pendingAction.value = null
  }
}

async function restartApp() {
  pendingAction.value = 'restart'
  try {
    await store.restartPackage(appId.value)
    // A restart can rotate credentials/admin URLs — invalidate so the next
    // read is fresh rather than the pre-restart cache (T-02-12).
    credentialsResource.invalidate()
    await holdUntilBackendPicksUp()
  } catch (err) {
    showActionError(`Failed to restart: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    pendingAction.value = null
  }
}

async function updateApp() {
  pendingAction.value = 'update'
  try {
    await store.updatePackage(appId.value)
  } catch (err) {
    showActionError(`Failed to update: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    pendingAction.value = null
  }
}

function showUninstallModal() {
  if (!pkg.value) return
  uninstallModal.value = { show: true, appTitle: pkg.value.manifest.title }
}

async function confirmUninstall(deleteAppData: boolean) {
  uninstallModal.value.show = false
  pendingAction.value = 'uninstall'
  try {
    await store.uninstallPackage(appId.value, { preserveData: !deleteAppData })
    // Invalidate before navigating away — the app no longer exists, so its
    // cached credentials must not be replayed if this screen is reopened
    // before the TTL lapses (T-02-12).
    credentialsResource.invalidate()
    router.push('/dashboard/apps').catch(() => {})
  } catch (err) {
    showActionError(`Failed to uninstall: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    pendingAction.value = null
  }
}

function uninstallApp() {
  showUninstallModal()
}
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.3s ease;
}

.modal-enter-active .glass-card,
.modal-leave-active .glass-card {
  transition: transform 0.3s ease, opacity 0.3s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from .glass-card,
.modal-leave-to .glass-card {
  transform: scale(0.95);
  opacity: 0;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
