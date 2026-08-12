<template>
  <div class="pb-6">
    <BackButton :label="t('goalDetail.backToGoals')" desktop-margin="mb-6" @click="goBack" />

    <!-- Goal not found -->
    <div v-if="!goal" class="glass-card p-12 text-center">
      <p class="text-white/70">{{ t('goalDetail.notFound') }}</p>
    </div>

    <!-- Goal wizard -->
    <template v-else>
      <div class="mb-8">
        <h1 class="text-3xl font-bold text-white mb-2 drop-shadow-[0_2px_8px_rgba(0,0,0,0.6)]">{{ goal.title }}</h1>
        <p class="text-white/70">{{ goal.subtitle }}</p>
      </div>

      <!-- Progress bar -->
      <div class="mb-8">
        <div class="flex items-center justify-between mb-2">
          <span class="text-sm text-white/60">{{ t('goalDetail.stepOf', { current: currentStepDisplay, total: goal.steps.length }) }}</span>
          <span class="goal-status-badge" :class="statusBadgeClass">{{ statusLabel }}</span>
        </div>
        <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
          <div
            class="h-full rounded-full transition-all duration-500 ease-out"
            :class="overallStatus === 'completed' ? 'bg-green-400' : 'bg-orange-400'"
            :style="{ width: `${progressPercent}%` }"
          />
        </div>
      </div>

      <!-- Sovereignty patience message for bitcoin-dependent goals -->
      <div
        v-if="showSyncMessage"
        class="glass-card p-6 mb-6 border-l-4 border-orange-400"
      >
        <h3 class="text-lg font-semibold text-white mb-1">{{ t('goalDetail.syncTitle') }}</h3>
        <p class="text-white/60 text-sm leading-relaxed">
          {{ t('goalDetail.syncMessage') }}
        </p>
      </div>

      <!-- Steps -->
      <div class="space-y-3">
        <div
          v-for="(step, idx) in goal.steps"
          :key="step.id"
          class="glass-card p-0 overflow-hidden"
        >
          <div
            class="goal-step"
            :class="{
              'goal-step-completed': isStepCompleted(step),
              'goal-step-active': idx === activeStepIndex && overallStatus !== 'completed',
              'goal-step-pending': idx > activeStepIndex && !isStepCompleted(step),
            }"
          >
            <div class="flex items-start gap-4">
              <!-- Step indicator -->
              <div class="mt-0.5 shrink-0">
                <div v-if="isStepCompleted(step)" class="w-6 h-6 rounded-full bg-green-500/20 flex items-center justify-center">
                  <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <div v-else-if="idx === activeStepIndex && isInstalling" class="w-6 h-6 rounded-full bg-orange-500/20 flex items-center justify-center">
                  <svg class="w-4 h-4 text-orange-400 animate-spin" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                  </svg>
                </div>
                <div v-else class="w-6 h-6 rounded-full bg-white/10 flex items-center justify-center">
                  <span class="text-xs text-white/40 font-medium">{{ idx + 1 }}</span>
                </div>
              </div>

              <!-- App icon -->
              <img
                v-if="stepIconUrl(step)"
                :src="stepIconUrl(step)"
                :alt="step.title"
                class="w-7 h-7 rounded-md object-contain shrink-0 mt-0.5"
                @error="($event.target as HTMLImageElement).style.display = 'none'"
              />

              <!-- Step content -->
              <div class="flex-1 min-w-0">
                <h3 class="text-base font-semibold text-white/90 mb-1">{{ step.title }}</h3>
                <p class="text-sm text-white/55 leading-relaxed">{{ step.description }}</p>

                <!-- Action button for active step -->
                <div v-if="idx === activeStepIndex && overallStatus !== 'completed'" class="mt-4">
                  <button
                    v-if="step.action === 'install' && step.appId && !isAppInstalled(step.appId)"
                    @click="installApp(step)"
                    :disabled="isInstalling"
                    class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium"
                  >
                    {{ isInstalling ? t('common.installing') : t('goalDetail.installApp', { name: step.title.replace('Install ', '') }) }}
                  </button>

                  <!-- Fund the bitcoin wallet: IBD-gated, with live sync timer -->
                  <div v-else-if="step.action === 'fund'" class="space-y-3">
                    <div v-if="!bitcoinSynced" class="p-3 rounded-lg bg-orange-500/10 border border-orange-500/20">
                      <div class="flex items-center justify-between gap-3 mb-1.5">
                        <span class="text-xs text-white/75">Bitcoin is syncing — funding unlocks when it finishes</span>
                        <span class="text-xs font-mono text-orange-300 shrink-0">{{ bitcoinSyncLoaded ? bitcoinSyncPercent.toFixed(1) + '%' : '…' }}</span>
                      </div>
                      <div class="h-1.5 bg-white/10 rounded-full overflow-hidden mb-1.5">
                        <div class="h-full bg-orange-400 rounded-full transition-all duration-700" :style="{ width: `${Math.min(100, bitcoinSyncPercent)}%` }" />
                      </div>
                      <p class="text-xs text-white/50">
                        <span v-if="bitcoinSyncEtaText" class="text-white/70 font-medium">~{{ bitcoinSyncEtaText }} remaining</span>
                        <span v-else>Estimating time remaining…</span>
                        <span v-if="bitcoinBlockHeight"> · Block {{ bitcoinBlockHeight.toLocaleString() }}</span>
                      </p>
                      <p class="text-xs text-white/45 mt-1.5">We'll pop a notification here the moment it's done.</p>
                    </div>

                    <template v-else>
                      <div class="p-3 rounded-lg bg-white/5 border border-white/10">
                        <div class="flex items-center justify-between gap-3">
                          <span class="text-xs text-white/60">On-chain wallet balance</span>
                          <span class="text-sm font-mono" :class="walletOnchainSats >= ZEUS_CHANNEL_MIN_SATS ? 'text-green-400' : 'text-white/85'">
                            {{ walletOnchainSats.toLocaleString() }} sats
                          </span>
                        </div>
                        <p class="text-xs text-white/45 mt-1">Minimum 150,000 · maximum 1,500,000 on-chain sats required.</p>
                      </div>
                      <div class="flex flex-wrap gap-2">
                        <button
                          @click="showFundModal = true"
                          class="glass-button glass-button-warning glass-button-sm rounded-lg px-5 py-2 text-sm font-medium"
                        >
                          Fund Wallet
                        </button>
                        <button
                          @click="completeFundStep(step)"
                          :disabled="walletOnchainSats <= 0"
                          class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium disabled:opacity-40"
                        >
                          {{ walletOnchainSats > 0 ? 'Continue' : 'Waiting for funds…' }}
                        </button>
                      </div>
                    </template>
                  </div>

                  <button
                    v-else-if="step.action === 'configure'"
                    @click="openConfigureStep(step)"
                    class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium"
                  >
                    {{ step.ctaLabel ?? t('goalDetail.openAndConfigure') }}
                  </button>
                  <button
                    v-else-if="step.action === 'verify'"
                    @click="completeVerifyStep(step)"
                    class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium"
                  >
                    {{ t('goalDetail.checkAndContinue') }}
                  </button>
                  <button
                    v-else-if="step.action === 'info'"
                    @click="completeInfoStep(step)"
                    class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium"
                  >
                    {{ t('goalDetail.iveDoneThis') }}
                  </button>
                  <button
                    v-else-if="isStepCompleted(step) || isAppInstalled(step.appId || '')"
                    disabled
                    class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium opacity-50"
                  >
                    {{ t('goalDetail.complete') }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Completion -->
      <div v-if="overallStatus === 'completed'" class="glass-card p-8 mt-6 text-center">
        <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-green-500/20 flex items-center justify-center">
          <svg class="w-8 h-8 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
        </div>
        <h2 class="text-xl font-semibold text-white mb-2">{{ t('goalDetail.allSet') }}</h2>
        <p class="text-white/60 mb-6">{{ t('goalDetail.goalReady', { title: goal.title }) }}</p>
        <button
          v-if="completionCta"
          @click="openCompletionTarget"
          class="glass-button rounded-lg px-6 py-3 font-medium"
        >
          {{ completionCta.label }}
        </button>
        <RouterLink v-else to="/dashboard/apps" class="glass-button rounded-lg px-6 py-3 font-medium">
          {{ t('goalDetail.viewMyServices') }}
        </RouterLink>
      </div>
    </template>

    <!-- Fund-wallet receive modal (on-chain address + QR, Zeus limits noted) -->
    <ReceiveBitcoinModal
      :show="showFundModal"
      note="Fund your Lightning channel: minimum 150,000 · maximum 1,500,000 on-chain sats required."
      auto-generate
      @close="showFundModal = false"
    />

    <!-- Action error toast -->
    <Transition name="fade">
      <div v-if="actionError" class="fixed bottom-20 left-1/2 -translate-x-1/2 z-50 max-w-md w-full px-4" role="alert" aria-live="assertive">
        <div class="alert-error backdrop-blur-sm rounded-lg px-4 py-3 text-sm flex items-center justify-between gap-3">
          <span>{{ actionError }}</span>
          <button @click="actionError = ''" class="text-red-300 hover:text-white shrink-0">&times;</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter, RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { useGoalStore } from '@/stores/goals'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { getGoalById, ZEUS_CHANNEL_MIN_SATS } from '@/data/goals'
import type { GoalStep } from '@/types/goals'
import { goalStepRouteOverride } from './goals/goalStepActions'
import BackButton from '@/components/BackButton.vue'
import ReceiveBitcoinModal from '@/components/ReceiveBitcoinModal.vue'
import { rpcClient } from '@/api/rpc-client'
import {
  acquireBitcoinSync,
  bitcoinSynced,
  bitcoinSyncLoaded,
  bitcoinSyncPercent,
  bitcoinBlockHeight,
  bitcoinSyncEtaText,
} from '@/composables/useBitcoinSync'

/** Map appId to its icon file path under /assets/img/app-icons/ */
const APP_ICON_MAP: Record<string, string> = {
  'bitcoin-knots': '/assets/img/app-icons/bitcoin-knots.webp',
  lnd: '/assets/img/app-icons/lnd.png',
  'btcpay-server': '/assets/img/app-icons/btcpay-server.png',
  immich: '/assets/img/app-icons/immich.png',
  nextcloud: '/assets/img/app-icons/nextcloud.webp',
  fedimint: '/assets/img/app-icons/fedimint.png',
  mempool: '/assets/img/app-icons/mempool.webp',
  electrs: '/assets/img/app-icons/electrumx.png',
  electrumx: '/assets/img/app-icons/electrumx.png',
}

function stepIconUrl(step: GoalStep): string | undefined {
  if (step.icon) return step.icon
  if (!step.appId) return undefined
  return APP_ICON_MAP[step.appId]
}

/**
 * Where the completion card sends the user: the app they just set up, not the
 * generic services list. `launchAppId` opens via the app launcher (iframe apps
 * overlay on top of the current screen; X-Frame-Options apps open a tab).
 */
const GOAL_COMPLETION_CTA: Record<string, { label: string; route?: string; launchAppId?: string }> = {
  'open-a-shop': { label: 'Go to my shop (BTCPay)', launchAppId: 'btcpay-server' },
  'accept-payments': { label: 'Go to Lightning (LND)', route: '/dashboard/apps/lnd' },
  'run-lightning-node': { label: 'View my channels', route: '/dashboard/apps/lnd/channels' },
  'setup-fedimint': { label: 'Open Fedimint', launchAppId: 'fedimint' },
  'file-browser': { label: 'Open File Browser', launchAppId: 'filebrowser' },
  'store-files': { label: 'Open my cloud (Nextcloud)', launchAppId: 'nextcloud' },
  'create-identity': { label: 'Go to my identity', route: '/dashboard/web5' },
  'back-up-everything': { label: 'Go to backups', route: '/dashboard/settings' },
}

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const appStore = useAppStore()
const goalStore = useGoalStore()

const goalId = computed(() => route.params.goalId as string)
const goal = computed(() => getGoalById(goalId.value))

const isInstalling = ref(false)
const actionError = ref('')
let errorTimer: ReturnType<typeof setTimeout> | undefined

function showActionError(msg: string) {
  actionError.value = msg
  if (errorTimer) clearTimeout(errorTimer)
  errorTimer = setTimeout(() => { actionError.value = '' }, 5000)
}

const overallStatus = computed(() => goalStore.getGoalStatus(goalId.value))

const completedSteps = computed(() => {
  if (!goal.value) return new Set<string>()
  const completed = new Set<string>()
  for (const step of goal.value.steps) {
    // Only install steps auto-tick from package state — manual steps (fund the
    // wallet, open a channel, configure) must be walked through.
    if (step.action === 'install' && step.appId && isAppInstalled(step.appId)) {
      completed.add(step.id)
    }
    if (goalStore.progress[goalId.value]?.completedSteps.includes(step.id)) {
      completed.add(step.id)
    }
  }
  return completed
})

const activeStepIndex = computed(() => {
  if (!goal.value) return 0
  const steps = goal.value.steps
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i]
    if (step && !completedSteps.value.has(step.id)) return i
  }
  return steps.length - 1
})

const currentStepDisplay = computed(() => Math.min(activeStepIndex.value + 1, goal.value?.steps.length || 1))

const progressPercent = computed(() => {
  if (!goal.value) return 0
  return Math.round((completedSteps.value.size / goal.value.steps.length) * 100)
})

const statusLabel = computed(() => {
  if (overallStatus.value === 'completed') return t('goalDetail.completed')
  if (overallStatus.value === 'in-progress') return t('goalDetail.inProgress')
  return t('goalDetail.notStarted')
})

const statusBadgeClass = computed(() => {
  if (overallStatus.value === 'completed') return 'goal-status-badge-completed'
  if (overallStatus.value === 'in-progress') return 'goal-status-badge-in-progress'
  return 'goal-status-badge-not-started'
})

const showSyncMessage = computed(() => {
  if (!goal.value) return false
  const hasBitcoin = goal.value.requiredApps.includes('bitcoin-knots')
  const bitcoinNotRunning = !isAppRunning('bitcoin-knots')
  return hasBitcoin && bitcoinNotRunning && overallStatus.value !== 'completed'
})

function isStepCompleted(step: GoalStep): boolean {
  return completedSteps.value.has(step.id)
}

/** App ID aliases — backend may register under variant names */
const APP_ALIASES: Record<string, string[]> = {
  immich: ['immich-server', 'immich-app', 'immich_server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
  'bitcoin-knots': ['bitcoin', 'bitcoin-core'],
}

function matchesAppId(pkgId: string, appId: string): boolean {
  if (pkgId === appId) return true
  const aliases = APP_ALIASES[appId]
  return aliases ? aliases.includes(pkgId) : false
}

function isAppInstalled(appId: string): boolean {
  return Object.keys(appStore.packages).some((pkgId) => matchesAppId(pkgId, appId))
}

function isAppRunning(appId: string): boolean {
  return Object.entries(appStore.packages).some(
    ([pkgId, pkg]) => matchesAppId(pkgId, appId) && pkg.state === 'running',
  )
}

async function installApp(step: GoalStep) {
  if (!step.appId) return
  isInstalling.value = true

  ensureGoalStarted()

  try {
    await appStore.installPackage(step.appId, '', 'latest')
    goalStore.completeStep(goalId.value, step.id)
  } catch (err) {
    showActionError(`Install failed: ${err instanceof Error ? err.message : 'Unknown error'}`)
  } finally {
    isInstalling.value = false
  }
}

function openConfigureStep(step: GoalStep) {
  ensureGoalStarted()
  goalStore.completeStep(goalId.value, step.id)
  const override = goalStepRouteOverride(step)
  if (override) {
    // Internal screens (channels, web5, settings) — tag where we came from so
    // their back button returns to this wizard.
    router.push({ path: override, query: { from: 'goal', goal: goalId.value } })
  } else if (step.appId) {
    // Launch the app itself: iframe apps overlay on top of the wizard,
    // tab-only apps open a tab (mobile: the in-app browser) — the app
    // launcher handles every case.
    useAppLauncherStore().openSession(step.appId)
  }
}

function completeVerifyStep(step: GoalStep) {
  ensureGoalStarted()
  goalStore.completeStep(goalId.value, step.id)
}

function completeInfoStep(step: GoalStep) {
  ensureGoalStarted()
  goalStore.completeStep(goalId.value, step.id)
}

function ensureGoalStarted() {
  if (!goalStore.progress[goalId.value]) {
    goalStore.startGoal(goalId.value)
  }
}

function goBack() {
  // The goal cards live on Home's Setup tab — return there, not the dashboard.
  router.push({ path: '/dashboard', query: { tab: 'setup' } })
}

// ── Fund-wallet step: live sync status + on-chain balance ───────────────────

const showFundModal = ref(false)
const walletOnchainSats = ref(0)

const hasFundStep = computed(() => goal.value?.steps.some((s) => s.action === 'fund') ?? false)
const fundStepActive = computed(() => {
  if (!goal.value || !hasFundStep.value) return false
  const active = goal.value.steps[activeStepIndex.value]
  return active?.action === 'fund' && overallStatus.value !== 'completed'
})

let releaseSync: (() => void) | null = null
let balanceTimer: ReturnType<typeof setInterval> | null = null

async function refreshWalletBalance() {
  try {
    const res = await rpcClient.call<{ balance_sats: number }>({ method: 'lnd.getinfo', timeout: 8000 })
    walletOnchainSats.value = res.balance_sats || 0
  } catch { /* LND not up yet — balance stays at last known value */ }
}

watch(fundStepActive, (active) => {
  if (active) {
    if (!releaseSync) releaseSync = acquireBitcoinSync()
    void refreshWalletBalance()
    if (!balanceTimer) balanceTimer = setInterval(() => void refreshWalletBalance(), 15000)
  } else {
    if (releaseSync) { releaseSync(); releaseSync = null }
    if (balanceTimer) { clearInterval(balanceTimer); balanceTimer = null }
  }
}, { immediate: true })

// Refresh the balance right after the receive modal closes — the user may
// have just sent funds.
watch(showFundModal, (open) => { if (!open) void refreshWalletBalance() })

onUnmounted(() => {
  if (releaseSync) { releaseSync(); releaseSync = null }
  if (balanceTimer) { clearInterval(balanceTimer); balanceTimer = null }
})

function completeFundStep(step: GoalStep) {
  ensureGoalStarted()
  goalStore.completeStep(goalId.value, step.id)
}

// ── Completion CTA: go to the app you just set up ────────────────────────────

const completionCta = computed(() => (goal.value ? GOAL_COMPLETION_CTA[goal.value.id] : undefined))

function openCompletionTarget() {
  const cta = completionCta.value
  if (!cta) return
  if (cta.launchAppId) {
    // Iframe apps overlay on top of the current screen; X-Frame-Options apps
    // (BTCPay, Nextcloud…) open in a new tab.
    useAppLauncherStore().openSession(cta.launchAppId)
  } else if (cta.route) {
    router.push(cta.route)
  }
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
