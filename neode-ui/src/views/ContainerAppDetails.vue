<template>
  <div class="p-6">
    <div class="mb-6">
      <BackButton :label="t('containerDetails.back')" @click="$router.back()" />
      
      <div class="flex items-start justify-between">
        <div>
          <h1 class="text-3xl font-bold text-white mb-2">{{ appName }}</h1>
          <p class="text-white/70">{{ t('containerDetails.subtitle') }}</p>
        </div>
        <ContainerStatus 
          v-if="container"
          :state="container.state as ContainerStateValue"
          :health="healthStatus as HealthStatusValue"
        />
      </div>
    </div>

    <Transition name="content-fade" mode="out-in">
    <div v-if="loading" key="loading" class="flex items-center justify-center py-12">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-white/60"></div>
    </div>

    <div v-else-if="error" key="error" class="glass-card p-6">
      <div class="flex items-center gap-3 text-red-400">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <span>{{ error }}</span>
      </div>
    </div>

    <div v-else-if="container" key="content" class="space-y-6">
      <!-- Container Info Card -->
      <div class="glass-card p-6">
        <h2 class="text-xl font-semibold text-white mb-4">{{ t('containerDetails.containerInfo') }}</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <span class="text-sm text-white/60">{{ t('containerDetails.containerId') }}</span>
            <p class="text-white/90 font-mono text-sm mt-1">{{ container.id }}</p>
          </div>
          <div>
            <span class="text-sm text-white/60">{{ t('containerDetails.image') }}</span>
            <p class="text-white/90 text-sm mt-1">{{ container.image }}</p>
          </div>
          <div>
            <span class="text-sm text-white/60">{{ t('containerDetails.state') }}</span>
            <p class="text-white/90 text-sm mt-1 capitalize">{{ container.state }}</p>
          </div>
          <div>
            <span class="text-sm text-white/60">{{ t('containerDetails.created') }}</span>
            <p class="text-white/90 text-sm mt-1">{{ formatDate(container.created) }}</p>
          </div>
        </div>
      </div>

      <!-- Actions Card -->
      <div class="glass-card p-6">
        <h2 class="text-xl font-semibold text-white mb-4">{{ t('containerDetails.actions') }}</h2>
        <div class="flex gap-4">
          <!-- Single primary button: Start when stopped, Stop when running,
               transitional label + spinner while stopping/starting/restarting. -->
          <button
            :disabled="isPrimaryDisabled"
            @click="handlePrimary"
            :class="primaryButtonClass"
            class="px-6 py-3 rounded-lg font-medium text-white transition-colors disabled:opacity-60 disabled:cursor-not-allowed flex items-center gap-2"
          >
            <svg v-if="isTransitional" class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            <span>{{ primaryButtonLabel }}</span>
          </button>
          <button
            @click="handleRestart"
            :disabled="actionLoading || isTransitional || container.state !== 'running'"
            class="px-6 py-3 glass-button rounded-lg font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
          >
            {{ t('common.restart') }}
          </button>
          <button
            @click="handleRemove"
            :disabled="actionLoading || isTransitional"
            class="px-6 py-3 glass-button rounded-lg font-medium text-red-400/90 hover:text-red-400 transition-colors disabled:opacity-50"
          >
            {{ t('common.remove') }}
          </button>
        </div>
      </div>

      <!-- Logs Card -->
      <div class="glass-card p-6">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-xl font-semibold text-white">{{ t('containerDetails.logs') }}</h2>
          <button
            @click="refreshLogs"
            :disabled="logsLoading"
            class="px-4 py-2 glass-button rounded text-sm font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
          >
            {{ t('common.refresh') }}
          </button>
        </div>
        <div class="bg-black/40 rounded-lg p-4 font-mono text-sm text-white/80 max-h-96 overflow-y-auto">
          <div v-if="logsLoading" class="text-center py-4 text-white/60">
            {{ t('containerDetails.loadingLogs') }}
          </div>
          <div v-else-if="logs.length === 0" class="text-center py-4 text-white/60">
            {{ t('containerDetails.noLogs') }}
          </div>
          <div v-else>
            <div v-for="(log, index) in logs" :key="index" class="mb-1">
              {{ log }}
            </div>
          </div>
        </div>
      </div>
    </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useContainerStore } from '@/stores/container'
import { type ContainerStatus as ContainerStatusData } from '@/api/container-client'
import ContainerStatus from '@/components/ContainerStatus.vue'
import BackButton from '@/components/BackButton.vue'

type ContainerStateValue =
  | 'created'
  | 'running'
  | 'stopped'
  | 'exited'
  | 'paused'
  | 'unknown'
  | 'stopping'
  | 'starting'
  | 'restarting'
  | 'installing'
  | 'updating'
  | 'removing'
  | 'installed'
type HealthStatusValue = 'healthy' | 'unhealthy' | 'unknown' | 'starting'

const route = useRoute()
const router = useRouter()
const store = useContainerStore()
const { t } = useI18n()

const appId = computed(() => route.params.id as string)
const appName = computed(() => {
  return appId.value
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
})

const container = ref<ContainerStatusData | null>(null)
const logs = ref<string[]>([])
const loading = ref(false)
const logsLoading = ref(false)
const actionLoading = ref(false)
const error = ref<string | null>(null)
const healthStatus = ref<string>('unknown')

onMounted(async () => {
  await loadContainer()
  await loadLogs()
  await loadHealthStatus()
})

async function loadContainer() {
  loading.value = true
  error.value = null
  try {
    const status = await store.getContainerStatus(appId.value)
    container.value = status
  } catch (e) {
    error.value = e instanceof Error ? e.message : t('common.error')
  } finally {
    loading.value = false
  }
}

async function loadLogs() {
  logsLoading.value = true
  try {
    logs.value = await store.getContainerLogs(appId.value, 100)
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to load logs:', e)
  } finally {
    logsLoading.value = false
  }
}

async function loadHealthStatus() {
  await store.fetchHealthStatus()
  healthStatus.value = store.getHealthStatus(appId.value)
}

async function refreshLogs() {
  await loadLogs()
}

async function handleStart() {
  actionLoading.value = true
  try {
    await store.startContainer(appId.value)
    await loadContainer()
    await loadHealthStatus()
  } catch (e) {
    error.value = e instanceof Error ? e.message : t('common.error')
  } finally {
    actionLoading.value = false
  }
}

async function handleStop() {
  actionLoading.value = true
  try {
    await store.stopContainer(appId.value)
    await loadContainer()
  } catch (e) {
    error.value = e instanceof Error ? e.message : t('common.error')
  } finally {
    actionLoading.value = false
  }
}

async function handleRestart() {
  actionLoading.value = true
  try {
    // Use the async container-restart RPC (returns immediately, backend
    // flips state to Restarting and spawns the stop+start sequence).
    await store.restartContainer(appId.value)
    await loadContainer()
    await loadHealthStatus()
  } catch (e) {
    error.value = e instanceof Error ? e.message : t('common.error')
  } finally {
    actionLoading.value = false
  }
}

// Single-button state helpers mirroring ContainerApps.vue — driven off the
// backend state so transitional labels stay accurate across the 5–600s
// graceful-stop window.
const isTransitional = computed(() => {
  const s = container.value?.state
  return (
    s === 'stopping' ||
    s === 'starting' ||
    s === 'restarting' ||
    s === 'installing' ||
    s === 'updating' ||
    s === 'removing'
  )
})

const isPrimaryDisabled = computed(() => actionLoading.value || isTransitional.value)

const primaryButtonLabel = computed(() => {
  const s = container.value?.state
  switch (s) {
    case 'running':
      return t('containerDetails.stopContainer')
    case 'stopping':
      return 'Stopping…'
    case 'starting':
      return 'Starting…'
    case 'restarting':
      return 'Restarting…'
    case 'installing':
      return 'Installing…'
    case 'updating':
      return 'Updating…'
    case 'removing':
      return 'Removing…'
    default:
      return t('containerDetails.startContainer')
  }
})

const primaryButtonClass = computed(() => {
  const s = container.value?.state
  if (s === 'running') {
    return 'glass-button hover:text-white text-white/90'
  }
  if (isTransitional.value) {
    return 'bg-yellow-700/40 text-yellow-200'
  }
  return 'bg-green-600 hover:bg-green-500'
})

async function handlePrimary() {
  const s = container.value?.state
  if (s === 'running') {
    return handleStop()
  }
  if (!isTransitional.value) {
    return handleStart()
  }
}

// Poll every 2s while a transition is in flight so the label updates
// without needing a manual refresh.
let transitionalPollInterval: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  transitionalPollInterval = setInterval(async () => {
    if (isTransitional.value) {
      try {
        await loadContainer()
      } catch {
        // ignore transient poll errors
      }
    }
  }, 2000)
})

onUnmounted(() => {
  if (transitionalPollInterval) clearInterval(transitionalPollInterval)
})

async function handleRemove() {
  if (!confirm(t('apps.uninstallConfirm', { name: appName.value }))) {
    return
  }
  
  actionLoading.value = true
  try {
    await store.removeContainer(appId.value)
    router.push('/dashboard/apps')
  } catch (e) {
    error.value = e instanceof Error ? e.message : t('common.error')
  } finally {
    actionLoading.value = false
  }
}

function formatDate(dateString: string): string {
  try {
    const date = new Date(dateString)
    return date.toLocaleString()
  } catch {
    return dateString
  }
}
</script>
