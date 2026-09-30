<template>
  <div
    data-controller-container
    :data-controller-launch="canLaunch(pkg) ? '' : undefined"
    tabindex="0"
    role="link"
    class="glass-card flex flex-col h-full p-6 transition-all hover:-translate-y-1 cursor-pointer relative min-w-0 overflow-hidden"
    :class="{ 'card-stagger': showStagger }"
    :style="{ '--stagger-index': index }"
    @click="$emit('goToApp', id)"
    @keydown.enter="handleEnter"
  >
    <!-- Installing indicator — no overlay, just replaces action buttons at bottom -->

    <!-- Uninstalling — handled in button area below, no overlay -->

    <!-- Uninstall Icon (not for web-only apps) -->
    <button
      v-if="!isWebOnly && !isUninstalling && !isInstalling && pkg.state !== 'installing'"
      @click.stop="$emit('showUninstall', id, pkg)"
      class="absolute top-4 right-4 p-2 rounded-lg text-white/60 hover:text-red-400 hover:bg-red-500/20 transition-colors z-10"
      :aria-label="`${t('common.uninstall')} ${pkg.manifest?.title || id}`"
      :title="t('common.uninstall')"
    >
      <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
      </svg>
    </button>

    <div class="flex items-start gap-4">
      <img
        :src="icon"
        :alt="title"
        class="app-card-icon archy-app-icon w-14 h-14"
        @error="handleImageError"
      />
      <div class="flex-1 min-w-0 overflow-hidden">
        <div class="flex items-center gap-2 mb-0.5">
          <h3 class="text-lg font-semibold text-white truncate" :title="title">
            {{ title }}
          </h3>
          <span
            v-if="tier && tier !== 'optional'"
            class="tier-badge"
            :class="tier === 'core' ? 'tier-badge-core' : 'tier-badge-recommended'"
          >{{ tier }}</span>
          <!-- Update state is the bottom-left button only: a second pill up
               here duplicated the signal and crowded the title. -->
        </div>
        <p class="text-sm text-white/50">{{ version ? $ver(version) : '' }}</p>
        <p v-if="author" class="text-xs text-white/40 mt-0.5">{{ author }}</p>
      </div>
    </div>

    <p class="text-white/70 text-sm mt-3 mb-3 line-clamp-2 min-h-[2.5rem]">
      {{ description }}
    </p>

    <div v-if="!isInstalling && !isUninstalling && pkg.state !== 'installing'" class="flex items-center gap-2 min-w-0">
      <span
        class="shrink-0 inline-flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium"
        :class="getStatusClass(pkg.state, pkg.health, pkg['exit-code'])"
      >
        <svg
          v-if="isTransitioning"
          class="animate-spin h-3 w-3"
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span v-if="pkg.state === 'running' && pkg.health === 'unhealthy'" class="w-1.5 h-1.5 rounded-full bg-orange-400 animate-pulse"></span>
        {{ getStatusLabel(pkg.state, pkg.health, pkg['exit-code']) }}
      </span>
      <p v-if="blockedReason" :title="blockedReason" class="min-w-0 truncate text-xs leading-snug text-yellow-200/80">
        {{ blockedReason }}
      </p>
    </div>

    <!-- Quick Actions — icon buttons in uniform dark containers -->
    <!-- Installing progress — replaces action buttons -->
    <div v-if="isInstalling || pkg.state === 'installing'" class="mt-auto pt-4">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs text-white/70 flex items-center gap-1.5">
          <svg class="animate-spin h-3 w-3" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          {{ installProgress?.message || 'Installing...' }}
        </span>
        <span class="text-xs text-white/50">{{ Math.round(installProgress?.progress || 0) }}%</span>
      </div>
      <div class="w-full h-1.5 bg-white/10 rounded-full overflow-hidden">
        <div
          class="install-progress-fill h-full bg-white/60 rounded-full transition-all duration-500"
          :style="{ width: `${Math.max(installProgress?.progress || 2, 2)}%` }"
        ></div>
      </div>
    </div>

    <!-- Uninstalling progress — truthful stage-driven bar (mirrors install) -->
    <div v-else-if="isUninstalling" class="mt-auto pt-4">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs text-white/70 flex items-center gap-1.5">
          <svg class="animate-spin h-3 w-3" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          {{ uninstallStageLabel }}
        </span>
        <span v-if="uninstallProgress !== null" class="text-xs text-white/50">{{ uninstallProgress }}%</span>
      </div>
      <div class="w-full h-1.5 bg-white/10 rounded-full overflow-hidden">
        <div
          class="install-progress-fill h-full bg-white/60 rounded-full transition-all duration-500"
          :style="{ width: `${Math.max(uninstallProgress ?? 8, 4)}%` }"
        ></div>
      </div>
    </div>

    <div v-else class="mt-auto pt-4 flex gap-2">
      <!-- Update available -->
      <button
        v-if="pkg['available-update'] && pkg.state !== 'updating'"
        @click.stop="$emit('update', id)"
        class="px-3 py-2 rounded-lg text-sm font-medium flex items-center justify-center gap-1.5 bg-orange-500/20 border border-orange-500/40 text-orange-200 hover:bg-orange-500/30 transition-colors"
        :title="`Update to ${$ver(pkg['available-update'])}`"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        {{ t('common.update') }}
      </button>
      <!-- Updating in progress -->
      <span
        v-if="pkg.state === 'updating'"
        class="px-3 py-2 rounded-lg text-sm font-medium flex items-center justify-center gap-1.5 bg-orange-500/20 border border-orange-500/40 text-orange-200"
      >
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        {{ t('common.updating') }}
      </span>
      <!-- Launch -->
      <button
        v-if="canLaunch(pkg)"
        data-controller-launch-btn
        @click.stop="$emit('launch', id)"
        class="flex-1 px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium flex items-center justify-center gap-1.5"
      >
        {{ t('common.launch') }}
        <svg v-if="opensInTab(id)" class="hidden md:block w-3.5 h-3.5 opacity-60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" /></svg>
      </button>
      <!-- Start (play icon) -->
      <button
        v-if="!isWebOnly && !isLoading && (pkg.state === 'stopped' || pkg.state === 'exited')"
        @click.stop="$emit('start', id)"
        class="px-3 py-2 glass-button glass-button-sm rounded-lg flex items-center justify-center"
        :title="pkg.state === 'exited' ? 'Restart' : t('common.start')"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24"><path d="M8 5v14l11-7z" /></svg>
      </button>
      <!-- Starting (spinner) -->
      <button
        v-if="!isWebOnly && isLoading && (pkg.state === 'stopped' || pkg.state === 'exited' || pkg.state === 'starting')"
        disabled
        class="px-3 py-2 glass-button glass-button-sm rounded-lg opacity-50 cursor-not-allowed flex items-center justify-center"
      >
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
      </button>
      <!-- Stop (square icon) -->
      <button
        v-if="!isWebOnly && !isLoading && (pkg.state === 'running' || pkg.state === 'starting')"
        @click.stop="$emit('stop', id)"
        class="px-3 py-2 glass-button glass-button-sm rounded-lg flex items-center justify-center"
        :title="t('common.stop')"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="1" /></svg>
      </button>
      <!-- Restart -->
      <button
        v-if="!isWebOnly && !isLoading && (pkg.state === 'running' || pkg.state === 'starting')"
        @click.stop="$emit('restart', id)"
        class="px-3 py-2 glass-button glass-button-sm rounded-lg flex items-center justify-center"
        :title="t('common.restart')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
      <!-- Stopping (spinner) -->
      <button
        v-if="!isWebOnly && isLoading && (pkg.state === 'running' || pkg.state === 'starting' || pkg.state === 'stopping')"
        disabled
        class="px-3 py-2 glass-button glass-button-sm rounded-lg opacity-50 cursor-not-allowed flex items-center justify-center"
      >
        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PackageDataEntry } from '@/types/api'
import {
  isWebOnlyApp, opensInTab, canLaunch, launchBlockedReason, resolveAppIcon, resolveAppTitle,
  getStatusClass, getStatusLabel, handleImageError,
} from './appsConfig'
import { getCuratedAppList } from '../discover/curatedApps'

const { t } = useI18n()

// Build a lookup map for enriching sparse backend data during install
const curatedMap = new Map(getCuratedAppList().map(a => [a.id, a]))

const props = defineProps<{
  id: string
  pkg: PackageDataEntry
  index: number
  showStagger: boolean
  isLoading: boolean
  isInstalling?: boolean
  installProgress?: { status: string; progress: number; message: string }
  isUninstalling: boolean
}>()

const emit = defineEmits<{
  goToApp: [id: string]
  launch: [id: string]
  start: [id: string]
  stop: [id: string]
  restart: [id: string]
  update: [id: string]
  showUninstall: [id: string, pkg: PackageDataEntry]
}>()

function handleEnter(e: KeyboardEvent) {
  // Controller nav already handled this Enter (preventDefault was called) — skip to avoid double navigation
  if (e.defaultPrevented) return
  emit('goToApp', props.id)
}

const isWebOnly = computed(() => isWebOnlyApp(props.id))

// Enrich from marketplace when backend data is sparse (e.g. during install)
const curated = computed(() => curatedMap.get(props.id))
const title = computed(() => resolveAppTitle(props.id, props.pkg, curated.value?.title))
const description = computed(() => {
  const d = props.pkg.manifest?.description?.short
  return (d && d !== 'Installing...') ? d : (curated.value?.description || d || '')
})
const icon = computed(() => resolveAppIcon(props.id, props.pkg, curated.value?.icon))
const version = computed(() => {
  const v = props.pkg.manifest?.version
  return v || curated.value?.version || ''
})
const author = computed(() => props.pkg.manifest?.author || curated.value?.author || '')
const tier = computed(() => {
  const t = props.pkg.manifest?.tier
  if (t && t !== '') return t
  const core = ['bitcoin-knots', 'bitcoin', 'lnd', 'mempool', 'btcpay-server', 'dwn', 'filebrowser']
  const recommended = ['fedimint', 'vaultwarden', 'uptime-kuma', 'grafana', 'searxng', 'tailscale', 'netbird', 'portainer']
  if (core.includes(props.id)) return 'core'
  if (recommended.includes(props.id)) return 'recommended'
  return 'optional'
})

// Live uninstall stage from backend, with a sensible fallback so the
// label is never blank between WS pushes.
const uninstallStageLabel = computed(() => {
  const raw = props.pkg['uninstall-stage']
  return raw ? raw : `${t('common.uninstalling')}…`
})

// Map the backend's uninstall-stage label to a truthful percentage so the bar
// progresses through the teardown instead of sitting at a solid full(-red)
// block. Backend stages (set_uninstall_stage):
//   "Stopping containers (X/N)" → 10–50%   (linear over the stack)
//   "Cleaning up volumes"       → 70%
//   "Removing app data"         → 90%
// Unknown/between pushes → null → the bar parks low and the shimmer overlay
// (install-progress-fill) carries the motion, exactly like a fixed install phase.
const uninstallProgress = computed<number | null>(() => {
  const raw = props.pkg['uninstall-stage'] || ''
  const m = raw.match(/\((\d+)\s*\/\s*(\d+)\)/)
  if (m) {
    const done = Number(m[1])
    const total = Number(m[2])
    if (total > 0) {
      return Math.round(10 + Math.min(done / total, 1) * 40)
    }
  }
  if (/volume/i.test(raw)) return 70
  if (/data/i.test(raw)) return 90
  return null
})

const isTransitioning = computed(() => {
  const s = props.pkg.state
  const h = props.pkg.health
  return s === 'starting' || s === 'installing' || s === 'stopping' || s === 'restarting' || s === 'updating' || (s === 'running' && h === 'starting')
})
const blockedReason = computed(() => launchBlockedReason(props.id, props.pkg))
</script>

<style scoped>
/* Shimmer overlay on the install progress bar so users see motion even
 * when the bar is parked at a fixed phase percentage (pulling-image can
 * take minutes, and podman doesn't give us byte-level progress). */
.install-progress-fill {
  background-image: linear-gradient(
    90deg,
    rgba(255, 255, 255, 0.55) 0%,
    rgba(255, 255, 255, 0.9) 50%,
    rgba(255, 255, 255, 0.55) 100%
  );
  background-size: 200% 100%;
  animation: install-shimmer 1.8s ease-in-out infinite;
}

@keyframes install-shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* Respect user motion preferences */
@media (prefers-reduced-motion: reduce) {
  .install-progress-fill {
    animation: none;
    background-image: none;
  }
}
</style>
