<template>
  <div class="relative flex-1 min-h-0 bg-black/40 overflow-hidden app-session-frame-safe">
    <Transition name="content-fade">
      <!-- Suppressed while the ElectrumX sync overlay below is showing — both
           conditions can be true at once during launch (generic loader fires
           first, then sync status arrives), and the sync screen is strictly
           more informative, so it takes precedence instead of the two
           rendering on top of each other. -->
      <AppLoadingScreen v-if="loading && !(electrsSync && !electrsSync.stale)" :icon="appIcon" :title="appTitle" :progress="loadProgress" />
    </Transition>

    <!-- ElectrumX sync screen — shown before the real UI while the on-chain
         index is still being built (the Electrum server can't serve clients
         until then). Mirrors the Fedimint Guardian "wait page" design. -->
    <Transition name="content-fade">
      <!-- Sync overlay only while ElectrumX is actively indexing. If the status
           goes stale (ElectrumX disconnected/unresponsive) we stop blocking and
           let the app's own UI load instead of a loader stuck on top (B7). -->
      <div v-if="electrsSync && !electrsSync.stale" class="absolute inset-0 z-10 flex flex-col items-center justify-center">
        <div class="text-center px-8 w-full max-w-md">
          <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-white/5 border border-white/10 flex items-center justify-center overflow-hidden animate-pulse">
            <img :src="appIcon" :alt="appTitle" class="w-full h-full object-cover" @error="handleImageError" />
          </div>
          <h3 class="text-lg font-semibold text-white mb-2">{{ appTitle }} is syncing</h3>
          <p class="text-white/50 text-sm mb-5">
            ElectrumX is building its index from the blockchain. The UI opens
            automatically once it's ready — you can keep using the rest of Archipelago.
          </p>
          <div class="w-full h-2 rounded-full bg-white/10 overflow-hidden mb-2">
            <div
              class="h-full bg-orange-400/80 transition-all duration-700"
              :style="{ width: `${Math.min(100, Math.max(2, electrsSync.progress_pct)).toFixed(1)}%` }"
            ></div>
          </div>
          <p class="text-white/70 text-sm font-medium mb-1">{{ electrsSync.progress_pct.toFixed(1) }}%</p>
          <p class="text-white/40 text-xs">
            Block {{ electrsSync.indexed_height.toLocaleString() }} of {{ electrsSync.network_height.toLocaleString() }}
            <template v-if="electrsSync.index_size"> · {{ electrsSync.index_size }} indexed</template>
          </p>
          <p v-if="electrsSync.stale" class="text-yellow-400/70 text-xs mt-2">Reconnecting to ElectrumX…</p>
        </div>
      </div>
    </Transition>

    <div
      v-if="appUrl && !iframeBlocked && (!electrsSync || electrsSync.stale)"
      class="absolute inset-0 app-session-frame-scroll-host"
      tabindex="-1"
      @pointerdown="focusIframe"
      @focusin="focusIframe"
    >
      <iframe
        ref="iframeRef"
        :key="refreshKey"
        :src="appUrl"
        class="w-full h-full border-0 iframe-scrollbar-hide"
        title="App content"
        tabindex="0"
        @load="handleIframeLoad"
        @error="$emit('iframeError')"
      />
    </div>

    <!-- Iframe blocked fallback. Suppressed while the ElectrumX sync screen
         (the "pre UI") is showing: a still-syncing Electrum server isn't
         reachable yet, so the "App not reachable / retry" overlay would just
         paint over the sync progress and read as a hard error. -->
    <Transition name="content-fade">
      <div v-if="iframeBlocked && !electrsSync" class="absolute inset-0 z-10 flex flex-col items-center justify-center">
        <div class="text-center px-8">
          <!-- Warm-up uses the app's own icon, pulsing, rather than the padlock:
               the padlock reads as "blocked/denied" and this state is neither. -->
          <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-white/5 border border-white/10 flex items-center justify-center overflow-hidden" :class="{ 'animate-pulse': warmingUp }">
            <img v-if="warmingUp" :src="appIcon" :alt="appTitle" class="w-full h-full object-cover" @error="handleImageError" />
            <svg v-else class="w-8 h-8 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
          </div>
          <h3 class="text-lg font-semibold text-white mb-2">{{ warmingUp ? `${appTitle} is starting…` : blockedReason ? blockedTitle : (mustOpenNewTab ? 'This app opens in a new tab' : 'App not reachable') }}</h3>
          <p class="text-white/50 text-sm mb-6">
            <template v-if="mustOpenNewTab">{{ appTitle }} sets security headers that prevent iframe embedding.<br>Open it in a new browser tab instead.</template>
            <template v-else-if="warmingUp">The container is running but hasn't finished warming up yet.<br>This screen opens on its own as soon as it answers.<span v-if="autoRetryCount > 0" class="block text-yellow-400/70">Checking again automatically ({{ autoRetryCount }})...</span></template>
            <template v-else-if="blockedReason">{{ blockedReason }}<br><span v-if="autoRetryCount > 0" class="text-yellow-400/70">Checking again automatically ({{ autoRetryCount }})...</span></template>
            <template v-else>{{ appTitle }} may still be starting up or the container is stopped.<br><span v-if="autoRetryCount > 0" class="text-yellow-400/70">Retrying automatically ({{ autoRetryCount }})...</span></template>
          </p>
          <div class="flex flex-wrap items-center justify-center gap-3">
            <button
              v-if="!mustOpenNewTab"
              @click="$emit('refresh')"
              class="glass-button px-6 py-3 rounded-lg text-sm font-semibold inline-flex items-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              Retry now
            </button>
            <button
              @click="$emit('openNewTabAndBack')"
              class="glass-button px-6 py-3 rounded-lg text-sm font-semibold inline-flex items-center gap-2"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
              Open in new tab
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <div v-if="!appUrl" class="absolute inset-0 flex items-center justify-center">
      <div class="text-center px-8">
        <h3 class="text-lg font-semibold text-white mb-2">App not configured</h3>
        <p class="text-white/50 text-sm">No URL found for {{ appId }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import type { ElectrsSyncStatus } from '@/composables/useElectrsSync'
import AppLoadingScreen from '@/components/AppLoadingScreen.vue'
import { handleImageError } from '@/views/apps/appsConfig'

const props = defineProps<{
  appUrl: string
  appId: string
  appTitle: string
  appIcon: string
  loading: boolean
  iframeBlocked: boolean
  mustOpenNewTab: boolean
  autoRetryCount: number
  refreshKey: number
  blockedReason?: string
  blockedTitle?: string
  // True while the container is up but its probe hasn't answered yet and the
  // auto-retries are still in flight — a warm-up, not a failure.
  warmingUp?: boolean
  // Non-null only for ElectrumX while its index is still building — shows the
  // sync screen and gates the iframe until status flips to "synced".
  electrsSync?: ElectrsSyncStatus | null
}>()

const emit = defineEmits<{
  iframeLoad: []
  iframeError: []
  refresh: []
  openNewTabAndBack: []
}>()

const iframeRef = ref<HTMLIFrameElement | null>(null)

// Faux load progress for the loading screen. Cross-origin iframes give no real
// progress events, so ease toward ~92% while loading and snap to 100% on load —
// far better UX than a black screen with a bare spinner.
const loadProgress = ref(0)
let progressTimer: ReturnType<typeof setInterval> | null = null

function stopProgress() {
  if (progressTimer) { clearInterval(progressTimer); progressTimer = null }
}

function startProgress() {
  stopProgress()
  loadProgress.value = 8
  progressTimer = setInterval(() => {
    // Decelerate as it approaches the cap so it never visually "finishes" early.
    const remaining = 92 - loadProgress.value
    loadProgress.value += Math.max(0.4, remaining * 0.08)
    if (loadProgress.value >= 92) { loadProgress.value = 92; stopProgress() }
  }, 180)
}

watch(() => props.loading, (isLoading) => {
  if (isLoading) {
    startProgress()
  } else {
    stopProgress()
    loadProgress.value = 100
  }
}, { immediate: true })

watch(() => props.refreshKey, () => { if (props.loading) startProgress() })

onBeforeUnmount(stopProgress)

function focusIframe() {
  iframeRef.value?.focus({ preventScroll: true })
}

async function handleIframeLoad() {
  emit('iframeLoad')
  await nextTick()
  requestAnimationFrame(focusIframe)
}

watch(() => [props.appUrl, props.refreshKey, props.iframeBlocked], async () => {
  if (!props.appUrl || props.iframeBlocked) return
  await nextTick()
  requestAnimationFrame(focusIframe)
}, { immediate: true })

defineExpose({ iframeRef })
</script>
