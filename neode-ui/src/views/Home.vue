<template>
  <div class="pb-6">
    <div class="mb-4 md:mb-8 flex items-start justify-between gap-4">
      <div class="min-h-[4.5rem]">
        <div class="flex items-center gap-2">
          <h1 class="text-3xl font-bold text-white mb-2 drop-shadow-[0_2px_8px_rgba(0,0,0,0.6)]">
            {{ line1Text }}<span v-if="showCaretLine1" class="typing-caret"></span>
          </h1>
          <RefreshIndicator :state="homeRefreshIndicatorState" label="Refreshing wallet balance" />
        </div>
        <p class="text-white/80">
          {{ line2Text }}<span v-if="showCaretLine2" class="typing-caret"></span>
        </p>
      </div>
      <!-- Desktop: tabs inline with header -->
      <div
        v-if="!uiMode.isChat"
        role="tablist"
        class="hidden md:flex mode-switcher flex-shrink-0 transition-opacity duration-500"
        :class="{ 'opacity-0 pointer-events-none': showWelcomeBlock && !animateCards }"
      >
        <button class="mode-switcher-btn" role="tab" :aria-selected="homeTab === 'dashboard'" :class="{ 'mode-switcher-btn-active': homeTab === 'dashboard' }" @click="homeTab = 'dashboard'">{{ t('home.dashboardTab') }}</button>
        <button class="mode-switcher-btn" role="tab" :aria-selected="homeTab === 'setup'" :class="{ 'mode-switcher-btn-active': homeTab === 'setup' }" @click="homeTab = 'setup'">{{ t('home.setupTab') }}</button>
      </div>
    </div>

    <!-- Update notification banner -->
    <div
      v-if="updateAvailable && !updateDismissed"
      role="alert"
      class="mb-6 glass-card p-4 flex items-center justify-between gap-4 border-l-4 border-orange-400 transition-opacity duration-300"
      :class="{ 'opacity-0 pointer-events-none': showWelcomeBlock && !animateCards }"
    >
      <div class="flex items-center gap-3 min-w-0">
        <svg class="w-6 h-6 text-orange-400 shrink-0" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
        <div class="min-w-0">
          <p class="text-sm font-medium text-white">{{ t('home.updateAvailable', { version: updateVersion }) }}</p>
          <p v-if="updateChangelog" class="text-xs text-white/60 truncate">{{ updateChangelog }}</p>
        </div>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <RouterLink to="/dashboard/settings/update" class="glass-button rounded-lg px-4 py-2 text-sm font-medium">{{ t('home.updateNow') }}</RouterLink>
        <button @click="dismissUpdate" aria-label="Dismiss update notification" class="text-white/40 hover:text-white/80 transition-colors p-1" title="Dismiss">
          <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
        </button>
      </div>
    </div>

    <!-- Tab bar + content (all non-chat modes) -->
    <template v-if="!uiMode.isChat">
      <!-- Mobile: full-width tabs -->
      <div
        role="tablist"
        class="md:hidden mode-switcher mb-6 w-full transition-opacity duration-500"
        :class="{ 'opacity-0 pointer-events-none': showWelcomeBlock && !animateCards }"
      >
        <button class="mode-switcher-btn" role="tab" :aria-selected="homeTab === 'dashboard'" :class="{ 'mode-switcher-btn-active': homeTab === 'dashboard' }" @click="homeTab = 'dashboard'">{{ t('home.dashboardTab') }}</button>
        <button class="mode-switcher-btn" role="tab" :aria-selected="homeTab === 'setup'" :class="{ 'mode-switcher-btn-active': homeTab === 'setup' }" @click="homeTab = 'setup'">{{ t('home.setupTab') }}</button>
      </div>

      <!-- Setup tab -->
      <EasyHome v-if="homeTab === 'setup'" :show="!showWelcomeBlock || animateCards" :animate="animateCards" />

      <!-- Dashboard tab: overview cards -->
      <div
        v-else
        class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8 transition-opacity duration-300"
        :class="{ 'opacity-0 pointer-events-none': showWelcomeBlock && !animateCards }"
      >
        <!-- My Apps Overview -->
        <div data-controller-container tabindex="0" class="home-card controller-focusable order-1 lg:order-none" :class="{ 'home-card-animate': animateCards }" style="--card-stagger: 0">
          <div class="home-card-shell">
            <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
              <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
                <div class="home-card-text">
                  <h2 class="text-xl font-semibold text-white mb-1">{{ t('home.myApps') }}</h2>
                  <p class="text-sm text-white/70">{{ t('home.myAppsDesc') }}</p>
                </div>
                <RouterLink to="/dashboard/apps" :aria-label="t('home.goToApps')" class="text-white/60 hover:text-white transition-colors">
                  <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
                </RouterLink>
              </div>
              <div class="home-card-stats grid grid-cols-1 sm:grid-cols-2 gap-4 mb-4 flex-1 min-h-0">
                <div class="p-4 bg-white/5 rounded-lg">
                  <p class="text-xs text-white/60 mb-1">Installed / Running</p>
                  <p class="text-2xl font-bold text-white">{{ appCount }}/{{ runningCount }}</p>
                </div>
                <div class="p-4 bg-white/5 rounded-lg flex items-center justify-around">
                  <button v-for="app in quickLaunchApps" :key="app.id" @click="useAppLauncherStore().openSession(app.id)" class="group" :title="app.name">
                    <div class="w-14 h-14 rounded-xl overflow-hidden transition-all group-hover:-translate-y-1 group-hover:shadow-lg flex items-center justify-center">
                      <img :src="app.icon" :alt="app.name" class="w-full h-full rounded-xl archy-app-icon" @error="handleImageError" />
                    </div>
                  </button>
                </div>
              </div>
              <div class="home-card-buttons flex gap-2 mt-auto pt-4 shrink-0">
                <RouterLink to="/dashboard/marketplace" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.browseStore') }}</RouterLink>
                <RouterLink to="/dashboard/apps" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.manageApps') }}</RouterLink>
              </div>
            </div>
          </div>
        </div>

        <!-- Cloud Overview -->
        <div data-controller-container tabindex="0" class="home-card controller-focusable order-3 lg:order-none" :class="{ 'home-card-animate': animateCards }" style="--card-stagger: 1">
          <div class="home-card-shell">
            <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
              <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
                <div class="home-card-text">
                  <h2 class="text-xl font-semibold text-white mb-1">{{ t('home.cloud') }}</h2>
                  <p class="text-sm text-white/70">{{ t('home.cloudDesc') }}</p>
                </div>
                <RouterLink to="/dashboard/cloud" :aria-label="t('home.goToCloud')" class="text-white/60 hover:text-white transition-colors">
                  <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
                </RouterLink>
              </div>
              <div class="home-card-stats grid grid-cols-1 sm:grid-cols-2 gap-4 mb-4 flex-1 min-h-0">
                <div class="p-4 bg-white/5 rounded-lg"><p class="text-xs text-white/60 mb-1">{{ t('home.storageUsed') }}</p><p class="text-2xl font-bold text-white">{{ cloudStorageDisplay }}</p></div>
                <div class="p-4 bg-white/5 rounded-lg"><p class="text-xs text-white/60 mb-1">{{ t('home.folders') }}</p><p class="text-2xl font-bold text-white">{{ cloudFolderDisplay }}</p></div>
              </div>
              <div class="home-card-buttons flex gap-2 mt-auto pt-4 shrink-0">
                <RouterLink to="/dashboard/cloud" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.viewFolders') }}</RouterLink>
                <button @click="uploadFiles" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.uploadFiles') }}</button>
              </div>
            </div>
          </div>
        </div>

        <!-- Wallet Overview -->
        <HomeWalletCard
          class="order-2 lg:order-none"
          :animate="animateCards"
          :wallet-connected="walletConnected"
          :wallet-balance-unavailable="walletBalanceUnavailable"
          :wallet-onchain="walletOnchain"
          :wallet-lightning="walletLightning"
          :wallet-ecash="walletEcash"
          :wallet-fedimint="walletFedimint"
          :wallet-ark="walletArk"
          :wallet-transactions="walletTransactions"
          :is-dev="isDev"
          @show-scan="showScanModal = true"
          @show-send="showSendModal = true"
          @show-receive="showReceiveModal = true"
          @show-transactions="showTransactionsModal = true"
          @show-wallet-settings="showWalletSettingsModal = true"
          @faucet="devFaucet"
          @open-in-mempool="openInMempool"
        />

        <!-- Network Overview -->
        <div data-controller-container tabindex="0" class="home-card controller-focusable order-4 lg:order-none" :class="{ 'home-card-animate': animateCards }" style="--card-stagger: 3">
          <div class="home-card-shell">
            <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
              <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
                <div class="home-card-text">
                  <h2 class="text-xl font-semibold text-white mb-1">{{ t('home.network') }}</h2>
                  <p class="text-sm text-white/70">{{ t('home.networkDesc') }}</p>
                </div>
                <RouterLink to="/dashboard/server" :aria-label="t('home.goToNetwork')" class="text-white/60 hover:text-white transition-colors">
                  <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
                </RouterLink>
              </div>
              <div class="home-card-stats space-y-3 mb-4 flex-1 min-h-0">
                <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
                  <div class="flex items-center gap-3"><div class="w-2 h-2 rounded-full" :class="torConnected ? 'bg-purple-400' : 'bg-white/40'"></div><span class="text-sm text-white/80">Tor</span></div>
                  <span class="text-sm font-medium" :class="torConnected ? 'text-purple-400' : 'text-white/40'">{{ torConnected ? 'Connected' : 'Offline' }}</span>
                </div>
                <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
                  <div class="flex items-center gap-3"><div class="w-2 h-2 rounded-full" :class="vpnDotClass"></div><span class="text-sm text-white/80">VPN</span></div>
                  <span class="text-sm font-medium" :class="vpnTextClass">{{ vpnStatusLabel }}</span>
                </div>
                <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
                  <div class="flex items-center gap-3"><div class="w-2 h-2 rounded-full" :class="bitcoinDotClass"></div><span class="text-sm text-white/80">Bitcoin</span></div>
                  <span class="text-sm font-medium" :class="bitcoinTextClass">{{ bitcoinSyncDisplay }}</span>
                </div>
                <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
                  <div class="flex items-center gap-3"><div class="w-2 h-2 rounded-full" :class="fipsDotClass"></div><span class="text-sm text-white/80">FIPS</span></div>
                  <span class="text-sm font-medium" :class="fipsTextClass">{{ fipsStatusLabel }}</span>
                </div>
                <div v-if="homeStatus.tollgateStatus" class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
                  <div class="flex items-center gap-3"><div class="w-2 h-2 rounded-full" :class="tollgateDotClass"></div><span class="text-sm text-white/80">TollGate</span></div>
                  <span class="text-sm font-medium" :class="tollgateTextClass">{{ tollgateStatusLabel }}</span>
                </div>
              </div>
              <div class="home-card-buttons flex gap-2 mt-auto pt-4 shrink-0">
                <RouterLink to="/dashboard/server" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.manageNetwork') }}</RouterLink>
              </div>
            </div>
          </div>
        </div>

        <!-- App Store Recommendations -->
        <div
          v-if="homeRecommendedApps.length > 0"
          data-controller-container
          tabindex="0"
          class="home-card controller-focusable lg:col-span-2 order-5 lg:order-none"
          :class="{ 'home-card-animate': animateCards }"
          style="--card-stagger: 4"
        >
          <div class="home-card-shell">
            <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
              <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
                <div class="home-card-text">
                  <h2 class="text-xl font-semibold text-white mb-1">{{ t('home.recommendedApps') }}</h2>
                  <p class="text-sm text-white/70">{{ t('home.recommendedAppsDesc') }}</p>
                </div>
                <RouterLink to="/dashboard/marketplace" :aria-label="t('home.goToAppStore')" class="text-white/60 hover:text-white transition-colors">
                  <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
                </RouterLink>
              </div>
              <div class="home-card-stats grid grid-cols-1 md:grid-cols-3 gap-3 mb-4 flex-1 min-h-0">
                <button
                  v-for="app in homeRecommendedApps"
                  :key="app.id"
                  type="button"
                  class="w-full flex items-center gap-3 p-3 bg-white/5 rounded-lg text-left transition-colors hover:bg-white/10"
                  @click="viewRecommendedApp(app)"
                >
                  <img
                    v-if="app.icon"
                    :src="app.icon"
                    :alt="app.title || app.id"
                    class="w-10 h-10 rounded-lg archy-app-icon shrink-0"
                    @error="handleImageError"
                  />
                  <div v-else class="w-10 h-10 rounded-lg bg-white/10 shrink-0"></div>
                  <div class="min-w-0 flex-1">
                    <p class="text-sm font-medium text-white truncate">{{ app.title || app.id }}</p>
                    <p class="text-xs text-white/55 truncate">{{ marketplaceDescription(app) }}</p>
                  </div>
                </button>
              </div>
              <div class="home-card-buttons flex gap-2 mt-auto pt-4 shrink-0">
                <RouterLink to="/dashboard/marketplace" class="home-card-btn flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">{{ t('home.browseStore') }}</RouterLink>
              </div>
            </div>
          </div>
        </div>

        <!-- Quick Start Goals -->
        <div
          v-if="showQuickStart"
          class="home-card lg:col-span-2 transition-opacity duration-300 order-6 lg:order-none"
          :class="{ 'home-card-animate': animateCards, 'opacity-0 pointer-events-none': showWelcomeBlock && !animateCards }"
          style="--card-stagger: 5"
        >
          <div class="home-card-shell">
            <div class="home-card-inner px-6 py-6 flex flex-col h-full min-h-0">
              <div class="flex items-start justify-between mb-2 shrink-0">
                <div>
                  <h2 class="text-xl font-semibold text-white/96 mb-1">{{ t('home.quickStartGoals') }}</h2>
                  <p class="text-sm text-white/60 mb-4">{{ t('home.quickStartDesc') }}</p>
                </div>
                <button @click="dismissQuickStart" aria-label="Dismiss Quick Start" class="text-white/40 hover:text-white/80 transition-colors p-1 -mt-1 -mr-1" title="Dismiss">
                  <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
                </button>
              </div>
              <div class="grid grid-cols-1 gap-3 mt-auto">
                <RouterLink v-for="goal in topGoals" :key="goal.id" :to="goal.route || `/dashboard/goals/${goal.id}`" class="home-card-btn path-action-button path-action-button--continue flex items-center justify-center gap-3">
                  <span>{{ goal.title }}</span>
                </RouterLink>
              </div>
            </div>
          </div>
        </div>

        <!-- System Stats -->
        <HomeSystemCard
          class="order-7 lg:order-none"
          :animate="animateCards"
          :loaded="systemStatsLoaded"
          :stats="systemStats"
          :uptime-display="systemUptimeDisplay"
        />
      </div>
    </template>

    <!-- Chat Mode -->
    <div v-if="uiMode.isChat" class="flex flex-col items-center justify-center min-h-[40vh]">
      <RouterLink to="/dashboard/chat" class="glass-button rounded-lg px-8 py-4 text-lg font-medium">{{ t('home.openAI') }}</RouterLink>
    </div>

    <!-- Wallet Modals -->
    <WalletScanModal :show="showScanModal" @close="showScanModal = false" @sent="loadWeb5Status()" />
    <SendBitcoinModal :show="showSendModal" @close="showSendModal = false" @sent="loadWeb5Status()" @scan="showSendModal = false; showScanModal = true" />
    <ReceiveBitcoinModal :show="showReceiveModal" @close="showReceiveModal = false" @received="loadWeb5Status()" @scan="showReceiveModal = false; showScanModal = true" />
    <TransactionsModal :show="showTransactionsModal" :transactions="walletTransactions" @updated="loadWeb5Status" @close="showTransactionsModal = false" />
    <WalletSettingsModal :show="showWalletSettingsModal" @close="showWalletSettingsModal = false" @changed="loadWeb5Status()" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onActivated, onBeforeUnmount, onDeactivated, onMounted } from 'vue'
import { useCachedResource } from '@/composables/useCachedResource'
import RefreshIndicator from '@/components/RefreshIndicator.vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import WalletScanModal from '@/components/WalletScanModal.vue'
import SendBitcoinModal from '@/components/SendBitcoinModal.vue'
import ReceiveBitcoinModal from '@/components/ReceiveBitcoinModal.vue'
import TransactionsModal from '@/components/TransactionsModal.vue'
import WalletSettingsModal from '@/components/WalletSettingsModal.vue'
import { useAppStore } from '../stores/app'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { wsClient } from '@/api/websocket'
import { useTxExplorer } from '@/composables/useTxExplorer'
import { useLoginTransitionStore } from '../stores/loginTransition'
import { useUIModeStore } from '@/stores/uiMode'
import { useHomeStatusStore } from '@/stores/homeStatus'
import { PackageState } from '../types/api'
import { playTypingSound } from '@/composables/useLoginSounds'
import { GOALS } from '@/data/goals'
import EasyHome from '@/components/EasyHome.vue'
import { fileBrowserClient } from '@/api/filebrowser-client'
import { rpcClient } from '@/api/rpc-client'
import { getAppUsage } from '@/utils/appUsage'
import { handleImageError, isServicePackage, isWebsitePackage, resolveAppIcon } from './apps/appsConfig'
import { useMarketplaceApp } from '@/composables/useMarketplaceApp'
import { getCuratedAppList, type MarketplaceApp } from './marketplace/marketplaceData'
import { getHomeRecommendedApps } from './home/homeRecommendations'
import HomeWalletCard from './home/HomeWalletCard.vue'
import HomeSystemCard from './home/HomeSystemCard.vue'
import type { WalletTransaction } from './home/HomeWalletCard.vue'

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const uiMode = useUIModeStore()
const isDev = import.meta.env.DEV
// ?tab=setup lands on the Setup tab (e.g. "Back to Goals" from a goal wizard)
const homeTab = ref<'dashboard' | 'setup'>(route.query.tab === 'setup' ? 'setup' : 'dashboard')
watch(() => route.query.tab, (tab) => {
  if (tab === 'setup') homeTab.value = 'setup'
  else if (tab === 'dashboard') homeTab.value = 'dashboard'
})
const topGoals = GOALS.slice(0, 3)

const QUICK_START_APPS = [...new Set(topGoals.flatMap((g) => g.requiredApps))]
const QUICK_START_KEY = 'archipelago-quick-start-dismissed'
const QUICK_START_RESHOW_LOGINS = 5

const store = useAppStore()
const homeStatus = useHomeStatusStore()
const loginTransition = useLoginTransitionStore()
const { setCurrentApp } = useMarketplaceApp()

const LINE1 = t('home.title')
const LINE2 = t('home.subtitle')
const MS_PER_CHAR = 55

const displayLine1 = ref('')
const displayLine2 = ref('')
const showCaretLine1 = ref(false)
const showCaretLine2 = ref(false)
const showWelcomeBlock = ref(false)
const hasTypedWelcome = ref(false)
const animateCards = ref(false)
let typingInterval: ReturnType<typeof setInterval> | null = null

const line1Text = computed(() => showWelcomeBlock.value ? displayLine1.value : LINE1)
const line2Text = computed(() => showWelcomeBlock.value ? displayLine2.value : LINE2)

onBeforeUnmount(() => {
  if (typingInterval) clearInterval(typingInterval)
  disarmLiveDataPolling()
})

watch(() => loginTransition.pendingWelcomeTyping, (pending) => { if (pending) showWelcomeBlock.value = true })

watch(() => loginTransition.startWelcomeTyping, (shouldStart) => {
  if (!shouldStart || hasTypedWelcome.value) return
  hasTypedWelcome.value = true; showWelcomeBlock.value = true
  displayLine1.value = ''; displayLine2.value = ''
  showCaretLine1.value = true; showCaretLine2.value = false
  playTypingSound(); animateCards.value = true
  let i = 0
  typingInterval = setInterval(() => {
    if (i < LINE1.length) { displayLine1.value = LINE1.slice(0, i + 1); i++ }
    else if (i < LINE1.length + LINE2.length) { showCaretLine1.value = false; showCaretLine2.value = true; displayLine2.value = LINE2.slice(0, i - LINE1.length + 1); i++ }
    else { if (typingInterval) clearInterval(typingInterval); typingInterval = null; showCaretLine2.value = false; loginTransition.setStartWelcomeTyping(false) }
  }, MS_PER_CHAR)
}, { immediate: true })

const packages = computed(() => store.packages)
const appCount = computed(() => Object.keys(packages.value || {}).length)
const runningCount = computed(() => Object.values(packages.value || {}).filter(pkg => pkg.state === PackageState.Running).length)

const quickLaunchApps = computed(() => {
  const usage = getAppUsage()
  return Object.entries(packages.value || {})
    .filter(([id, pkg]) => !isServicePackage(id, pkg) && !isWebsitePackage(id, pkg))
    .map(([id, pkg]) => ({
      id,
      name: pkg.manifest?.title || id,
      icon: resolveAppIcon(id, pkg),
      state: pkg.state,
      usage: usage[id]?.count || 0,
      lastLaunchedAt: usage[id]?.lastLaunchedAt || 0,
    }))
    .sort((a, b) => {
      if (b.usage !== a.usage) return b.usage - a.usage
      if (b.lastLaunchedAt !== a.lastLaunchedAt) return b.lastLaunchedAt - a.lastLaunchedAt
      if ((b.state === PackageState.Running ? 1 : 0) !== (a.state === PackageState.Running ? 1 : 0)) {
        return (b.state === PackageState.Running ? 1 : 0) - (a.state === PackageState.Running ? 1 : 0)
      }
      return a.name.localeCompare(b.name)
    })
    .slice(0, 3)
})

const homeRecommendedApps = computed(() => getHomeRecommendedApps(getCuratedAppList(), packages.value, 3))

function viewRecommendedApp(app: MarketplaceApp) {
  setCurrentApp(app)
  router.push({ name: 'marketplace-app-detail', params: { id: app.id }, query: { from: 'home' } }).catch(() => {})
}

function marketplaceDescription(app: MarketplaceApp) {
  if (typeof app.description === 'string') return app.description
  return app.description?.short || app.description?.long || ''
}

// Network card data
// Tor liveness, NOT "was an onion ever provisioned". This used to return
// `!!server-info['tor-address']`, but that address is read from the
// hidden-service hostname file on disk and outlives the daemon — so this card
// showed "Connected" on three fleet nodes whose Tor had been dead for days
// (2026-08-09), which is precisely why nobody noticed. `tor-running` is a live
// probe of 127.0.0.1:9050 performed server-side.
const torConnected = computed(() => store.data?.['server-info']?.['tor-running'] === true)
const vpnConnected = computed(() => homeStatus.vpnStatus.connected === true || (!!packages.value['tailscale'] && packages.value['tailscale'].state === PackageState.Running))
const vpnDotClass = computed(() => {
  if (vpnConnected.value) return 'bg-orange-400'
  return homeStatus.vpnKnown ? 'bg-white/40' : 'bg-white/25 animate-pulse'
})
const vpnTextClass = computed(() => vpnConnected.value ? 'text-orange-400' : (homeStatus.vpnKnown ? 'text-white/40' : 'text-white/50'))
const vpnStatusLabel = computed(() => {
  if (vpnConnected.value) return homeStatus.vpnStatus.provider || 'WireGuard'
  if (!homeStatus.vpnKnown) return 'Checking…'
  return 'Not configured'
})
const fipsDotClass = computed(() => {
  const s = homeStatus.fipsStatus
  if (!s || !s.installed) return 'bg-white/40'
  if (!s.service_active) return 'bg-white/40'
  // Active but no anchor = degraded, not fully green
  if (s.anchor_connected === false) return 'bg-orange-400'
  return 'bg-green-400'
})
const fipsTextClass = computed(() => {
  const s = homeStatus.fipsStatus
  if (!s || !s.installed) return 'text-white/40'
  if (!s.service_active) return 'text-white/40'
  if (s.anchor_connected === false) return 'text-orange-400'
  return 'text-green-400'
})
const fipsStatusLabel = computed(() => {
  const s = homeStatus.fipsStatus
  if (!s) return homeStatus.fipsLoadState === 'loading' ? 'Checking…' : '…'
  if (!s.installed) return 'Not installed'
  if (!s.service_active) {
    if (!s.key_present) return 'Awaiting seed'
    return 'Inactive'
  }
  // Service is active — reflect anchor reachability in the label so the
  // Home and Server rows flip in sync with the FIPS card.
  if (s.anchor_connected === false) return 'No anchor'
  const peers = s.authenticated_peer_count ?? 0
  return peers === 1 ? 'Active · 1 peer' : `Active · ${peers} peers`
})
const tollgateDotClass = computed(() => {
  const s = homeStatus.tollgateStatus
  if (!s || !s.installed) return 'bg-white/40'
  return s.enabled ? 'bg-green-400' : 'bg-yellow-400'
})
const tollgateTextClass = computed(() => {
  const s = homeStatus.tollgateStatus
  if (!s || !s.installed) return 'text-white/40'
  return s.enabled ? 'text-green-400' : 'text-yellow-400'
})
const tollgateStatusLabel = computed(() => {
  const s = homeStatus.tollgateStatus
  if (!s) return homeStatus.tollgateLoadState === 'loading' ? 'Checking…' : 'Not configured'
  if (!s.installed) return 'Not installed'
  return s.enabled ? 'Enabled' : 'Disabled'
})
const bitcoinSyncDisplay = computed(() => {
  if (homeStatus.stats.bitcoinAvailable === null) return 'Checking…'
  if (!homeStatus.stats.bitcoinAvailable) return 'Not running'
  if (homeStatus.stats.bitcoinSyncPercent >= 99.9) return 'Synced'
  if (homeStatus.stats.bitcoinSyncPercent < 0.01 && homeStatus.stats.bitcoinBlockHeight === 0) return 'Loading...'
  return `${homeStatus.stats.bitcoinSyncPercent.toFixed(1)}%`
})
const bitcoinDotClass = computed(() => {
  if (homeStatus.stats.bitcoinAvailable === true) return 'bg-orange-400'
  if (homeStatus.stats.bitcoinAvailable === false) return 'bg-white/40'
  return 'bg-white/25 animate-pulse'
})
const bitcoinTextClass = computed(() => homeStatus.stats.bitcoinAvailable ? 'text-orange-400' : (homeStatus.stats.bitcoinAvailable === null ? 'text-white/50' : 'text-white/40'))

// Quick Start
const quickStartDismissed = ref(false)
const allQuickStartAppsInstalled = computed(() => QUICK_START_APPS.every((appId) => Object.keys(packages.value).includes(appId)))
const showQuickStart = computed(() => !allQuickStartAppsInstalled.value && !quickStartDismissed.value)

function loadQuickStartState() {
  try { const raw = localStorage.getItem(QUICK_START_KEY); if (!raw) { quickStartDismissed.value = false; return }; const data = JSON.parse(raw); if (!data.dismissed) { quickStartDismissed.value = false; return }; const loginCount = (data.loginCount || 0) + 1; localStorage.setItem(QUICK_START_KEY, JSON.stringify({ dismissed: true, loginCount })); quickStartDismissed.value = loginCount % QUICK_START_RESHOW_LOGINS !== 0 } catch { quickStartDismissed.value = false }
}
function dismissQuickStart() { quickStartDismissed.value = true; try { localStorage.setItem(QUICK_START_KEY, JSON.stringify({ dismissed: true, loginCount: 0 })) } catch { /* ignore */ } }
loadQuickStartState()

// Update notification
const updateAvailable = ref(false); const updateDismissed = ref(false); const updateVersion = ref(''); const updateChangelog = ref('')
async function checkUpdateStatus() {
  try { const res = await rpcClient.call<{ update_available: boolean }>({ method: 'update.status', dedup: true }); updateAvailable.value = res.update_available } catch { /* unavailable */ }
  if (updateAvailable.value) { try { const detail = await rpcClient.call<{ update: { version: string; changelog: string[] } | null }>({ method: 'update.check', dedup: true }); if (detail.update) { updateVersion.value = detail.update.version; updateChangelog.value = detail.update.changelog.slice(0, 2).join('; ') } } catch { /* unavailable */ } }
}
async function dismissUpdate() { updateDismissed.value = true; try { await rpcClient.call({ method: 'update.dismiss' }) } catch { /* ignore */ } }

// Cloud data
const cloudStorageUsed = ref<number | null>(null); const cloudFolderCount = ref<number | null>(null)
function formatBytes(bytes: number): string { if (bytes === 0) return '0 B'; const units = ['B', 'KB', 'MB', 'GB', 'TB']; const i = Math.floor(Math.log(bytes) / Math.log(1024)); const val = bytes / Math.pow(1024, i); return `${val < 10 ? val.toFixed(1) : Math.round(val)} ${units[i]}` }
const cloudStorageDisplay = computed(() => cloudStorageUsed.value !== null ? formatBytes(cloudStorageUsed.value) : '...')
const cloudFolderDisplay = computed(() => cloudFolderCount.value !== null ? String(cloudFolderCount.value) : '...')

// 02-06: system stats, update status and cloud storage usage on keyed
// useCachedResource entries (side-effect-only, per the Mesh.vue/02-05
// pattern — each fetcher wraps an existing function that already sets its
// own reactive refs, resolving to a sentinel timestamp rather than the real
// payload). homeStatus.refresh() is a Pinia store action, so — mirroring
// 02-05's finding that a store's own defineStore(id, setup) runs in a bare
// effectScope where onActivated() silently no-ops — the useCachedResource
// wrapper lives here in Home.vue, not inside stores/homeStatus.ts.
//
// Web5.vue's own two resources (web5.networking-profits, web5.lnd-info)
// were read and are NOT shared here: web5.networking-profits is an
// unrelated dataset (routing/content-sale profit totals), and
// web5.lnd-info now declares persist: false explicitly (fixed, CR-01) so its
// independent refresh cycle can never leak balance data to sessionStorage —
// sharing that key would still either corrupt Web5.vue's differently-shaped
// entry.data (a real number here vs. its typed balance object there) or
// require Home to also opt into persist: false for no benefit.
// Home's own wallet fetch is also a strictly broader 7-call composite
// (lnd.getinfo + ecash/fedimint/ark balances + 3 histories), not the same
// single-call dataset. See 02-06-SUMMARY.md for the full finding.
const systemStatsRes = useCachedResource<number>({
  key: 'home.system-stats',
  immediate: false,
  ttlMs: 10_000, // matches the pre-existing 10s poll cadence
  persist: true, // aggregate system/bitcoin/vpn/fips/tollgate status, no identity payload
  fetcher: async () => { await loadSystemStats(); return Date.now() },
})
const updateStatusRes = useCachedResource<number>({
  key: 'home.update-status',
  immediate: false,
  ttlMs: 300_000, // near-static — an available update doesn't appear/disappear quickly
  persist: true,
  fetcher: async () => { await checkUpdateStatus(); return Date.now() },
})
const cloudUsageRes = useCachedResource<number>({
  key: 'home.cloud-usage',
  immediate: false,
  ttlMs: 30_000, // default tier
  persist: true,
  fetcher: async () => {
    try {
      const usage = await fileBrowserClient.getUsage()
      cloudStorageUsed.value = usage.totalSize
      cloudFolderCount.value = usage.folderCount
    } catch { /* not running */ }
    return Date.now()
  },
})
const homeCachedGroups = [systemStatsRes, updateStatusRes, cloudUsageRes]
function refreshHomeGroupIfStale(res: typeof systemStatsRes): Promise<void> {
  if (res.entry.data === null || res.isStale.value) return res.refresh()
  return Promise.resolve()
}

// Wallet is the exception (T-02-13): a money figure must never be presented
// as current without a visible re-check, so this resource revalidates
// UNCONDITIONALLY on every activation rather than TTL-gated — never via
// refreshHomeGroupIfStale above. persist:false — the fetcher wraps
// loadWeb5Status(), which sets the wallet balance/transaction refs directly
// as a side effect; hydrateWalletSnapshot() (localStorage, a separate
// mechanism) is the view's own last-known-figures path and is untouched by
// this resource.
const walletStatusRes = useCachedResource<number>({
  key: 'home.wallet-status',
  immediate: false,
  persist: false,
  fetcher: async () => { await loadWeb5Status(); return Date.now() },
})

// Only-while-visible side effects (system/update/storage cache revalidation,
// wallet poll, wallet websocket push) are armed on every activation
// (including the first mount — Vue fires onActivated on first mount too)
// and torn down on deactivation, so an off-screen Home costs nothing and a
// re-entered Home revalidates the liveness-critical wallet figures
// immediately rather than waiting out the 10s/30s intervals (T-02-13).
// Idempotent: any existing handle is cleared before a new one is armed, so
// two consecutive activations never double-arm.
function armLiveDataPolling() {
  void Promise.allSettled(homeCachedGroups.map(refreshHomeGroupIfStale))
  if (systemStatsInterval) clearInterval(systemStatsInterval)
  systemStatsInterval = setInterval(() => void systemStatsRes.refresh(), 10000)

  // Poll wallet balances/transactions like Web5.vue does — without this a
  // pending on-chain receive (or a fresh instant payment) only shows up
  // after a manual wallet action or a remount. Unconditional refresh, not
  // staleness-gated (T-02-13) — see walletStatusRes above.
  void walletStatusRes.refresh()
  if (walletRefreshInterval) clearInterval(walletRefreshInterval)
  walletRefreshInterval = setInterval(() => void walletStatusRes.refresh(), 30000)

  // Real-time wallet push (2026-07-22): the backend streams LND transaction
  // events and nudges /ws/db the moment a tx hits the mempool. Any push =
  // something changed → refetch the wallet (debounced; the RPC is cheap).
  // This is what makes an incoming 0-conf tx appear in seconds instead of
  // waiting out the 30s poll above.
  if (unsubscribeWs) { unsubscribeWs(); unsubscribeWs = null }
  unsubscribeWs = wsClient.subscribe(() => {
    if (wsWalletDebounce) clearTimeout(wsWalletDebounce)
    wsWalletDebounce = setTimeout(() => { void walletStatusRes.refresh() }, 800)
  })
}

// Drives the header RefreshIndicator (D-05/T-02-13): visible whenever the
// wallet resource is revalidating, so a resumed tab never presents a
// paused-poll balance as current without a visible signal.
const homeRefreshIndicatorState = computed(() => walletStatusRes.loadState.value)

function disarmLiveDataPolling() {
  if (systemStatsInterval) { clearInterval(systemStatsInterval); systemStatsInterval = null }
  if (walletRefreshInterval) { clearInterval(walletRefreshInterval); walletRefreshInterval = null }
  if (unsubscribeWs) { unsubscribeWs(); unsubscribeWs = null }
  if (wsWalletDebounce) { clearTimeout(wsWalletDebounce); wsWalletDebounce = null }
}

// Vue fires onActivated immediately after onMounted on a KeepAlive-wrapped
// component's first mount. Unlike a plain interval re-arm, armLiveDataPolling()
// also fires the wallet's 7-call Promise.allSettled immediately — firing it
// twice back-to-back on every fresh session start is real, avoidable waste,
// not a harmless no-op. This flag lets onMounted's call count as the first
// activation's arm; onActivated only re-arms on a genuine later reactivation.
let homeFreshMount = true
onActivated(() => {
  if (homeFreshMount) { homeFreshMount = false; return }
  armLiveDataPolling()
})
onDeactivated(() => { disarmLiveDataPolling() })

onMounted(() => {
  // Once-per-session: paint last-known wallet figures BEFORE any network
  // round-trip. Not polled, so it stays here rather than moving to
  // onActivated — a resumed tab doesn't need it re-run.
  hydrateWalletSnapshot()

  // 02-06: update status and cloud storage usage are now every-entry,
  // TTL-gated cached resources (see homeCachedGroups above) — armed
  // concurrently (Promise.allSettled, no sequential await chain) alongside
  // system stats and the wallet from the single armLiveDataPolling() call
  // below, rather than a separate trailing-await block here.
  //
  // Also arm here directly: onActivated is a no-op outside a <KeepAlive>
  // boundary (Vue only auto-fires it on first mount for a component that
  // already has a KeepAlive ancestor), so a bare mount — a unit test, or any
  // future non-KeepAlive usage — must not silently skip every poll/
  // subscription this view owns.
  armLiveDataPolling()
})

// Wallet modals
const showScanModal = ref(false); const showSendModal = ref(false); const showReceiveModal = ref(false); const showTransactionsModal = ref(false); const showWalletSettingsModal = ref(false)

async function devFaucet() { try { await rpcClient.call({ method: 'dev.faucet', params: { amount_sats: 1_000_000 } }); await loadWeb5Status() } catch { /* ignore */ } }

// Balances start as `null`, not 0. Zero is a *number*, and it is the one
// number that frightens people — rendering it before any call has returned
// told someone opening the dashboard, in the wallet's own typeface, that
// their money was gone. `null` means "not known yet" and paints a pixel
// readout instead; a rail only becomes a number when a call actually
// succeeds, so a real 0 is still a real 0.
const walletConnected = ref(false)
const walletBalanceUnavailable = ref(false)
const walletOnchain = ref<number | null>(null)
const walletLightning = ref<number | null>(null)
const walletEcash = ref<number | null>(null)
const walletFedimint = ref<number | null>(null)
let walletInfoFailures = 0
const walletArk = ref<number | null>(null)
const walletTransactions = ref<WalletTransaction[]>([])

// Overlay the local Mempool app when it's running; otherwise route through
// the external-explorer consent flow (pruned nodes can't run Mempool).
const txExplorer = useTxExplorer()
function openInMempool(txHash: string) { txExplorer.openTx(txHash) }

// wallet.ecash-history's shape (see handle_wallet_ecash_history in
// api/rpc/wallet.rs) — distinct from the LND-shaped WalletTransaction used
// elsewhere in this file, so it's mapped into that shape below rather than
// widening WalletTransaction itself with a pile of ecash-only fields.
interface EcashTransaction {
  id: string
  tx_type: 'send' | 'receive'
  amount_sats: number
  timestamp: string
  description: string
  mint_url: string
  peer: string
  kind: 'cashu' | 'fedimint' | 'ark'
}

function ecashToWalletTransaction(tx: EcashTransaction): WalletTransaction {
  return {
    tx_hash: tx.id,
    amount_sats: tx.amount_sats,
    direction: tx.tx_type === 'receive' ? 'incoming' : 'outgoing',
    num_confirmations: 1,
    time_stamp: Math.floor(new Date(tx.timestamp).getTime() / 1000),
    total_fees: 0,
    dest_addresses: [],
    label: tx.description,
    block_height: 0,
    kind: tx.kind,
  }
}

// Last-known wallet snapshot, hydrated before ANY network round-trip so the
// card paints real figures instantly (app-launch-speed doctrine: over the
// mesh every serialized RPC costs a full RTT — never make the user watch it).
const WALLET_SNAPSHOT_KEY = 'archy-wallet-snapshot-v1'

function hydrateWalletSnapshot() {
  try {
    const raw = localStorage.getItem(WALLET_SNAPSHOT_KEY)
    if (!raw) return
    const s = JSON.parse(raw)
    // A snapshot key that isn't there was never known — leave it unknown
    // rather than inventing a zero for it.
    walletOnchain.value = s.onchain ?? null
    walletLightning.value = s.lightning ?? null
    walletEcash.value = s.ecash ?? null
    walletFedimint.value = s.fedimint ?? null
    walletArk.value = s.ark ?? null
    walletConnected.value = s.connected === true
    if (Array.isArray(s.transactions)) walletTransactions.value = s.transactions
  } catch { /* corrupt/absent snapshot — fresh load fills in */ }
}

function persistWalletSnapshot() {
  try {
    localStorage.setItem(WALLET_SNAPSHOT_KEY, JSON.stringify({
      onchain: walletOnchain.value,
      lightning: walletLightning.value,
      ecash: walletEcash.value,
      fedimint: walletFedimint.value,
      ark: walletArk.value,
      connected: walletConnected.value,
      // Enough for the Transactions modal's first paint; refresh replaces it.
      transactions: walletTransactions.value.slice(0, 50),
    }))
  } catch { /* storage full — snapshot is best-effort */ }
}

async function loadWeb5Status() {
  // A transient RPC timeout must NOT flash the balance to 0 ("wallet says 0 when
  // there is a balance"). On failure keep the last-known value — the refs start
  // from the persisted snapshot, so 0 only ever shows on a genuinely fresh node.
  //
  // All seven calls are independent — fire them TOGETHER. Serialized, this
  // block cost 7 × (mesh RTT + backend time); parallel it costs one slowest
  // call, which is what makes the card feel like an app launch.
  const balances = Promise.allSettled([
    rpcClient.call<{ balance_sats: number; channel_balance_sats: number }>({ method: 'lnd.getinfo', timeout: 5000, dedup: true })
      .then(res => {
        if (!Number.isSafeInteger(res.balance_sats) || res.balance_sats < 0 ||
            !Number.isSafeInteger(res.channel_balance_sats) || res.channel_balance_sats < 0) {
          throw new Error('LND balance is unavailable')
        }
        walletOnchain.value = res.balance_sats
        walletLightning.value = res.channel_balance_sats
        walletConnected.value = true
        walletBalanceUnavailable.value = false
        walletInfoFailures = 0
      })
      .catch(() => {
        // A single slow poll must NOT flip the card to "disconnected" and
        // hide balances the user already knows — busy nodes routinely blow
        // the 5s budget mid-payment or during IO storms (a test node user
        // report: balances vanished while a payment settled). Only call it
        // disconnected after three consecutive failures (~30s of silence).
        walletBalanceUnavailable.value = true
        walletInfoFailures += 1
        if (walletInfoFailures >= 3) walletConnected.value = false
      }),
    rpcClient.call<{ balance_sats: number }>({ method: 'wallet.ecash-balance', timeout: 5000, dedup: true })
      .then(res => { walletEcash.value = res.balance_sats ?? 0 }).catch(() => { /* keep last-known */ }),
    rpcClient.call<{ balance_sats: number }>({ method: 'wallet.fedimint-balance', timeout: 5000, dedup: true })
      .then(res => { walletFedimint.value = res.balance_sats ?? 0 }).catch(() => { /* keep last-known */ }),
    rpcClient.call<{ spendable_sats: number }>({ method: 'wallet.ark-balance', timeout: 5000, dedup: true })
      .then(res => { walletArk.value = res.spendable_sats ?? 0 }).catch(() => { /* keep last-known */ }),
  ])
  // Merge LND transactions with ecash/Fedimint history (wallet.ecash-history
  // already unifies both) so Cashu/Fedimint receives appear in the modal.
  const histories = Promise.allSettled([
    rpcClient.call<{ transactions: WalletTransaction[]; incoming_pending_count: number }>({ method: 'lnd.gettransactions', timeout: 5000, dedup: true })
      .then(res => (res.transactions || []).map(tx => ({ ...tx, kind: 'onchain' as const }))).catch(() => [] as WalletTransaction[]),
    rpcClient.call<{ transactions: WalletTransaction[] }>({ method: 'lnd.lightning-history', timeout: 5000, dedup: true })
      .then(res => res.transactions || []).catch(() => [] as WalletTransaction[]),
    rpcClient.call<{ transactions: EcashTransaction[] }>({ method: 'wallet.ecash-history', timeout: 5000, dedup: true })
      .then(res => (res.transactions || []).map(ecashToWalletTransaction)).catch(() => [] as WalletTransaction[]),
  ]).then((results) => {
    const merged = results.flatMap(r => (r.status === 'fulfilled' ? r.value : []))
    // Keep last-known list when every history call failed this round.
    if (merged.length > 0 || results.some(r => r.status === 'fulfilled')) {
      walletTransactions.value = merged.sort((a, b) => b.time_stamp - a.time_stamp)
    }
  })
  await Promise.allSettled([balances, histories])
  persistWalletSnapshot()
}

// System stats
const systemStatsLoaded = computed(() => homeStatus.systemStatsLoaded)
const systemStats = computed(() => ({
  ...homeStatus.stats,
  bitcoinAvailable: homeStatus.stats.bitcoinAvailable === true,
  bitcoinStale: homeStatus.bitcoinStale,
}))
const systemUptimeDisplay = computed(() => { if (homeStatus.stats.uptimeSecs === 0) return t('home.systemMonitoring'); const days = Math.floor(homeStatus.stats.uptimeSecs / 86400); const hours = Math.floor((homeStatus.stats.uptimeSecs % 86400) / 3600); if (days > 0) return `Uptime: ${days}d ${hours}h`; const mins = Math.floor((homeStatus.stats.uptimeSecs % 3600) / 60); return `Uptime: ${hours}h ${mins}m` })

let systemStatsInterval: ReturnType<typeof setInterval> | null = null
let walletRefreshInterval: ReturnType<typeof setInterval> | null = null
let unsubscribeWs: (() => void) | null = null
let wsWalletDebounce: ReturnType<typeof setTimeout> | null = null

async function loadSystemStats() {
  await homeStatus.refresh(packages.value)
}

function uploadFiles() { const pkg = packages.value['filebrowser']; if (pkg && pkg.state === PackageState.Running) { const host = window.location.hostname; useAppLauncherStore().open({ url: `http://${host}:8083`, title: 'File Browser' }) } else { router.push('/dashboard/cloud') } }

defineExpose({
  loadWeb5Status,
  homeRefreshIndicatorState,
})
</script>

<style scoped>
.typing-caret::after { content: ''; display: inline-block; width: 3px; height: 1.1em; background: #fbbf24; margin-left: 2px; vertical-align: text-bottom; animation: caret-blink 0.7s step-end infinite; }
@keyframes caret-blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
</style>

<style>
/* Home card styles — unscoped so they reach child components (HomeWalletCard, HomeSystemCard) */
.grid > .home-card { min-height: 280px; }
.home-card-shell { background-color: rgba(0, 0, 0, 0.65); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); border-radius: 1rem; overflow: hidden; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45); border: 1px solid transparent; height: 100%; }
.home-card-animate .home-card-shell { animation: card-fly-in 1.2s cubic-bezier(0.25, 0.46, 0.45, 0.94) forwards; animation-delay: calc(var(--card-stagger) * 0.18s); opacity: 0; transform: translateY(50px) scale(0.92); }
@keyframes card-fly-in { 0% { opacity: 0; transform: translateY(50px) scale(0.92); border-color: transparent; } 75% { opacity: 1; transform: translateY(0) scale(1); border-color: transparent; } 100% { opacity: 1; transform: translateY(0) scale(1); border-color: rgba(255, 255, 255, 0.18); } }
.home-card-inner { overflow: hidden; opacity: 0; }
.home-card-animate .home-card-inner { animation: inner-draw 0.8s cubic-bezier(0.25, 0.46, 0.45, 0.94) forwards; animation-delay: calc(var(--card-stagger) * 0.18s + 0.9s); }
@keyframes inner-draw { 0% { opacity: 0; clip-path: inset(0 100% 0 0); } 15% { opacity: 1; } 100% { opacity: 1; clip-path: inset(0 0 0 0); } }
.home-card-text { overflow: hidden; }
.home-card-stats { overflow: hidden; }
.home-card-btn { opacity: 0; transform: scale(0.5); border-color: transparent; min-height: 44px; padding-top: 10px; padding-bottom: 10px; }
.home-card-animate .home-card-btn { animation: btn-pop 0.5s cubic-bezier(0.34, 1.56, 0.64, 1) forwards; animation-delay: calc(var(--card-stagger) * 0.18s + 1.5s); }
@keyframes btn-pop { 0% { opacity: 0; transform: scale(0.5); border-color: transparent; } 85% { opacity: 1; transform: scale(1); border-color: transparent; } 100% { opacity: 1; transform: scale(1); border-color: rgba(255, 255, 255, 0.18); } }
.home-card:not(.home-card-animate) .home-card-inner, .home-card:not(.home-card-animate) .home-card-btn { opacity: 1; animation: none; clip-path: none; transform: none; border-color: rgba(255, 255, 255, 0.18); }
.home-card:not(.home-card-animate) .home-card-shell { border-color: rgba(255, 255, 255, 0.18); }
</style>
