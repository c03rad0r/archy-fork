<template>
  <div class="p-6">
    <div class="mb-8">
      <h1 class="text-3xl font-bold text-white mb-2">My Apps</h1>
      <p class="text-white/70">Manage your Archipelago applications</p>
    </div>

    <!-- Loading State (initial load) -->
    <div v-if="store.loading && !hasAnyApps" class="flex items-center justify-center py-12">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-white/60"></div>
    </div>

    <!-- Error State -->
    <div v-if="store.error" class="glass-card p-6 mb-6">
      <div class="flex items-center gap-3 text-red-400">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span>{{ store.error }}</span>
      </div>
    </div>

    <!-- Apps Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
      <!-- Bundled Apps -->
      <div
        v-for="app in bundledApps"
        :key="app.id"
        data-controller-container
        :data-controller-launch="store.getAppState(app.id) === 'running' ? '' : undefined"
        tabindex="0"
        class="glass-card p-6 hover:bg-white/5 transition-colors"
      >
        <div class="flex items-start justify-between mb-4">
          <div class="flex items-center gap-3">
            <span class="text-3xl">{{ app.icon }}</span>
            <div>
              <h3 class="text-lg font-semibold text-white">{{ app.name }}</h3>
              <p class="text-sm text-white/60">{{ app.description }}</p>
            </div>
          </div>
        </div>

        <!-- Status Badge -->
        <div class="mb-4">
          <span 
            :class="getStatusBadgeClass(app.id)"
            class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium"
          >
            <!-- Loading spinner -->
            <svg 
              v-if="store.isAppLoading(app.id)" 
              class="w-3 h-3 animate-spin" 
              fill="none" 
              viewBox="0 0 24 24"
            >
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            <!-- Status dot -->
            <span 
              v-else
              :class="getStatusDotClass(app.id)"
              class="w-2 h-2 rounded-full"
            ></span>
            {{ getStatusText(app.id) }}
          </span>
        </div>

        <!-- Port info -->
        <div class="text-sm text-white/50 mb-4">
          <span v-if="store.getAppState(app.id) === 'running'">
            Port{{ app.ports.length > 1 ? 's' : '' }}: 
            {{ app.ports.map(p => p.host).join(', ') }}
          </span>
          <span v-else>
            {{ app.image.split('/').pop() }}
          </span>
        </div>

        <!-- Action Buttons -->
        <div class="flex gap-2">
          <!-- Single primary button whose label + action depend on visual state.
               Transitional states (stopping/starting/restarting/installing/
               updating/removing) show a spinner and are disabled. -->
          <button
            :disabled="isPrimaryDisabled(app.id)"
            @click="handlePrimary(app)"
            :class="primaryButtonClass(app.id)"
            class="flex-1 px-4 py-2 rounded text-sm font-medium text-white transition-colors flex items-center justify-center gap-2 disabled:cursor-not-allowed"
          >
            <svg v-if="isTransitional(app.id)" class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            <span>{{ primaryButtonLabel(app.id) }}</span>
          </button>

          <!-- Restart button: only visible when running, hidden during transitions -->
          <button
            v-if="store.getAppVisualState(app.id) === 'running'"
            @click="handleRestartApp(app.id)"
            :disabled="store.isAppLoading(app.id)"
            class="px-4 py-2 glass-button rounded text-sm font-medium text-white/90 hover:text-white disabled:cursor-not-allowed transition-colors"
            :title="'Restart ' + app.name"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>

          <!-- Launch button: only when running -->
          <button
            v-if="store.getAppVisualState(app.id) === 'running'"
            type="button"
            data-controller-launch-btn
            class="px-4 py-2 glass-button glass-button-warning rounded text-sm font-medium flex items-center gap-2"
            @click="launchApp(app)"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
            Launch
          </button>
        </div>
      </div>

      <!-- Other containers (not bundled) -->
      <div
        v-for="container in otherContainers"
        :key="container.id"
        data-controller-container
        tabindex="0"
        class="glass-card p-6 hover:bg-white/5 transition-colors"
      >
        <div class="flex items-start justify-between mb-4">
          <div class="flex items-center gap-3">
            <span class="text-3xl">📦</span>
            <div>
              <h3 class="text-lg font-semibold text-white">{{ extractAppName(container.name) }}</h3>
              <p class="text-sm text-white/60">{{ container.image }}</p>
            </div>
          </div>
        </div>

        <div class="mb-4">
          <ContainerStatus :state="container.state" />
        </div>

        <div class="flex gap-2">
          <button
            v-if="container.state !== 'running'"
            @click="handleStartContainer(container.name)"
            class="flex-1 px-4 py-2 glass-button rounded text-sm font-medium text-white/90 hover:text-white transition-colors"
          >
            Start
          </button>
          <button
            v-else
            @click="handleStopContainer(container.name)"
            class="flex-1 px-4 py-2 glass-button rounded text-sm font-medium text-white/90 hover:text-white transition-colors"
          >
            Stop
          </button>
        </div>
      </div>
    </div>

    <!-- Empty state (when no bundled apps - shouldn't happen) -->
    <div v-if="!hasAnyApps && !store.loading" class="glass-card p-12 text-center">
      <svg class="w-16 h-16 mx-auto mb-4 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
      </svg>
      <h3 class="text-xl font-semibold text-white mb-2">No apps available</h3>
      <p class="text-white/60">Check your Archipelago installation</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, computed } from 'vue'
import { useContainerStore, type BundledApp } from '@/stores/container'
import { useAppLauncherStore } from '@/stores/appLauncher'
import ContainerStatus from '@/components/ContainerStatus.vue'

const store = useContainerStore()
const appLauncherStore = useAppLauncherStore()

// Use enriched bundled apps with runtime data (like lan_address)
// Only show apps that actually have a container (hides pre-defined apps on unbundled installs)
const bundledApps = computed(() => store.enrichedBundledApps.filter(
  app => store.getAppState(app.id) !== 'not-installed'
))

// Get current host for launch URLs
const currentHost = computed(() => window.location.hostname)

let startingPollInterval: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await store.fetchContainers()
  await store.fetchHealthStatus()

  // Refresh every 10 seconds
  setInterval(async () => {
    try {
      await store.fetchContainers()
      await store.fetchHealthStatus()
    } catch {
      // Background poll — ignore transient errors
    }
  }, 10000)

  // When any bundled app is transitional (stopping/starting/restarting/etc),
  // poll every 2s so state updates flow through quickly.
  startingPollInterval = setInterval(async () => {
    const anyTransitional = bundledApps.value.some((app) => isTransitional(app.id))
    if (anyTransitional) {
      try {
        await store.fetchContainers()
        await store.fetchHealthStatus()
      } catch {
        // Background poll — ignore transient errors
      }
    }
  }, 2000)
})

onUnmounted(() => {
  if (startingPollInterval) clearInterval(startingPollInterval)
})

// Containers that aren't bundled apps
const otherContainers = computed(() => {
  const bundledIds = bundledApps.value.map(a => a.id)
  return store.containers.filter(c => {
    const name = c.name.toLowerCase()
    return !bundledIds.some(id => name.includes(id))
  })
})

const hasAnyApps = computed(() => bundledApps.value.length > 0 || store.containers.length > 0)

function extractAppName(containerName: string): string {
  return containerName
    .replace('archipelago-', '')
    .replace('-dev', '')
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

function getStatusBadgeClass(appId: string): string {
  const vs = store.getAppVisualState(appId)
  switch (vs) {
    case 'running':
      return 'bg-green-500/20 text-green-400'
    case 'stopped':
      return 'bg-gray-500/20 text-gray-400'
    case 'starting':
    case 'stopping':
    case 'restarting':
    case 'installing':
    case 'updating':
    case 'removing':
      return 'bg-yellow-500/20 text-yellow-400'
    case 'not-installed':
    default:
      return 'bg-blue-500/20 text-blue-400'
  }
}

function getStatusDotClass(appId: string): string {
  const vs = store.getAppVisualState(appId)
  switch (vs) {
    case 'running':
      return 'bg-green-400'
    case 'stopped':
      return 'bg-gray-400'
    case 'not-installed':
      return 'bg-blue-400'
    default:
      return 'bg-yellow-400'
  }
}

function getStatusText(appId: string): string {
  const vs = store.getAppVisualState(appId)
  switch (vs) {
    case 'running':
      return 'Running'
    case 'stopped':
      return 'Stopped'
    case 'not-installed':
      return 'Ready to Start'
    case 'starting':
      return 'Starting'
    case 'stopping':
      return 'Stopping'
    case 'restarting':
      return 'Restarting'
    case 'installing':
      return 'Installing'
    case 'updating':
      return 'Updating'
    case 'removing':
      return 'Removing'
    default:
      return 'Unknown'
  }
}

function isTransitional(appId: string): boolean {
  const vs = store.getAppVisualState(appId)
  return (
    vs === 'starting' ||
    vs === 'stopping' ||
    vs === 'restarting' ||
    vs === 'installing' ||
    vs === 'updating' ||
    vs === 'removing'
  )
}

function isPrimaryDisabled(appId: string): boolean {
  return isTransitional(appId) || store.isAppLoading(appId)
}

function primaryButtonLabel(appId: string): string {
  const vs = store.getAppVisualState(appId)
  switch (vs) {
    case 'running':
      return 'Stop'
    case 'stopped':
    case 'not-installed':
      return 'Start'
    case 'starting':
      return 'Starting…'
    case 'stopping':
      return 'Stopping…'
    case 'restarting':
      return 'Restarting…'
    case 'installing':
      return 'Installing…'
    case 'updating':
      return 'Updating…'
    case 'removing':
      return 'Removing…'
    default:
      return '—'
  }
}

function primaryButtonClass(appId: string): string {
  const vs = store.getAppVisualState(appId)
  if (vs === 'running') {
    return 'glass-button hover:text-white text-white/90 disabled:opacity-60'
  }
  if (vs === 'stopped' || vs === 'not-installed') {
    return 'bg-green-600 hover:bg-green-500 disabled:bg-green-800'
  }
  // Transitional: muted appearance
  return 'bg-yellow-700/40 text-yellow-200 disabled:opacity-80'
}

const backendPort = 5678

function getLaunchUrl(app: BundledApp): string {
  // Prefer lan_address from backend (for apps with custom UIs)
  if (app.lan_address) {
    // Replace localhost so Launch works when browsing from another machine (e.g. a LAN address)
    let url = app.lan_address.replace(/localhost/i, currentHost.value)
    // LND UI (and other app UIs) need backend URL for live data (logs, getinfo proxy)
    if (app.id === 'lnd') {
      const backend = `http://${currentHost.value}:${backendPort}`
      url += (url.includes('?') ? '&' : '?') + 'backend=' + encodeURIComponent(backend)
    }
    return url
  }

  // Fallback to first configured port
  const port = app.ports[0]?.host
  if (!port) return '#'
  return `http://${currentHost.value}:${port}`
}

function launchApp(app: BundledApp) {
  const url = getLaunchUrl(app)
  if (url === '#') return
  appLauncherStore.open({ url, title: app.name })
}

async function handlePrimary(app: BundledApp) {
  const vs = store.getAppVisualState(app.id)
  if (vs === 'running') {
    return handleStopApp(app.id)
  }
  if (vs === 'stopped' || vs === 'not-installed') {
    return handleStartApp(app)
  }
  // Transitional — button should be disabled; ignore.
}

async function handleStartApp(app: BundledApp) {
  try {
    // Route through container-start (async) rather than the legacy synchronous
    // bundled-app-start RPC so Stop/Start exercise the spawn_transitional path
    // and the UI sees Starting → Running transitions.
    await store.startContainer(app.id)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to start app:', e)
  }
}

async function handleStopApp(appId: string) {
  try {
    await store.stopContainer(appId)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to stop app:', e)
  }
}

async function handleRestartApp(appId: string) {
  try {
    await store.restartContainer(appId)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to restart app:', e)
  }
}

async function handleStartContainer(name: string) {
  try {
    const appId = name.replace('archipelago-', '').replace('-dev', '')
    await store.startContainer(appId)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to start container:', e)
  }
}

async function handleStopContainer(name: string) {
  try {
    const appId = name.replace('archipelago-', '').replace('-dev', '')
    await store.stopContainer(appId)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to stop container:', e)
  }
}
</script>
