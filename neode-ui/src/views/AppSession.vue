<template>
  <!-- The root stays in the layout as a rect placeholder; the session itself
       ALWAYS lives under <body>. Toggling Teleport's disabled re-parented the
       subtree on panel<->overlay switches, and moving an iframe node reloads
       it — inline mode is now emulated with a fixed-position rect synced from
       this placeholder, so the iframe never moves in the DOM. -->
  <div class="app-session-root" ref="rootRef">
  <Teleport to="body">
  <div
    :class="backdropClasses"
    :style="inlineRectStyle"
    @click.self="handleBackdropClick"
  >
    <div
      ref="sessionRef"
      :class="panelClasses"
      @click.stop
    >
      <AppSessionHeader
        :app-title="appTitle"
        :is-refreshing="isRefreshing"
        :display-mode="displayMode"
        @go-back="iframeGoBack"
        @go-forward="iframeGoForward"
        @refresh="refresh"
        @open-new-tab="openNewTab"
        @close="closeSession"
        @set-mode="setMode"
      />

      <AppSessionFrame
        ref="frameRef"
        :app-url="appUrl"
        :app-id="appId"
        :app-title="appTitle"
        :app-icon="appIcon"
        :loading="loading"
        :iframe-blocked="iframeBlocked"
        :must-open-new-tab="mustOpenNewTab"
        :auto-retry-count="autoRetryCount"
        :refresh-key="refreshKey"
        :ui-ready-blocked="packageEntry?.['ui-ready'] === false"
        :blocked-reason="blockedReason"
        :blocked-title="blockedTitle"
        :warming-up="warmingUp"
        :electrs-sync="electrsSync"
        @iframe-load="onLoad"
        @iframe-error="onError"
        @refresh="refresh"
        @open-new-tab-and-back="openNewTabAndBack"
      />

      <!-- Mobile: gamepad for botfights (with utility buttons), browser bar for everything else -->
      <MobileGamepad
        v-if="isMobile && appId === 'botfights'"
        :iframe-ref="iframeRef ?? null"
        :player="1"
        @refresh="refresh"
        @openBrowser="openNewTab"
        @close="closeSession"
      />
      <div v-else class="md:hidden app-session-mobile-bar">
        <button class="app-session-bar-btn" aria-label="Back" @click="iframeGoBack">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <button class="app-session-bar-btn" aria-label="Forward" @click="iframeGoForward">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
        <button class="app-session-bar-btn" aria-label="Refresh" @click="refresh">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" :class="{ 'animate-spin': isRefreshing }">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v6h6M20 20v-6h-6M5.64 15.36A8 8 0 0018.36 18M18.36 8.64A8 8 0 005.64 6" />
          </svg>
        </button>
        <button class="app-session-bar-btn" aria-label="Open in new tab" @click="openNewTab">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
        </button>
        <button class="app-session-bar-btn" aria-label="Close" @click="closeSession">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Host-owned signer stays inside the active app surface. This keeps
           the context visible and works unchanged in the companion WebView. -->
      <NostrSignConsent
        :show="nostrBridge.showConsent.value"
        :app-name="nostrBridge.consentRequest.value?.appName ?? appTitle"
        :method="nostrBridge.consentRequest.value?.method ?? ''"
        :identity-label="nostrBridge.consentRequest.value?.identityLabel"
        :event-kind="nostrBridge.consentRequest.value?.eventKind"
        :content="nostrBridge.consentRequest.value?.content"
        :phase="nostrBridge.consentPhase.value"
        :error="nostrBridge.consentError.value"
        @approve="nostrBridge.approveConsent"
        @deny="nostrBridge.denyConsent"
      />
      <NostrIdentityPicker
        :show="showIdentityPicker"
        :app-name="appTitle"
        @select="identity.onIdentitySelected"
        @cancel="identity.cancelIdentitySelection"
      />
    </div>
  </div>
  </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onBeforeUnmount, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useAppStore } from '@/stores/app'
import { useScreensaverStore } from '@/stores/screensaver'
import NostrIdentityPicker from '@/components/NostrIdentityPicker.vue'
import NostrSignConsent from '@/components/NostrSignConsent.vue'
import { isAutoTabApp, rememberAutoTabApp, forgetAutoTabApp } from '@/utils/autoTabApps'
import AppSessionHeader from './appSession/AppSessionHeader.vue'
import AppSessionFrame from './appSession/AppSessionFrame.vue'
import MobileGamepad from './appSession/MobileGamepad.vue'
import {
  type DisplayMode, DISPLAY_MODE_KEY, NEW_TAB_APPS, IFRAME_BLOCKED_APPS,
  initialDisplayMode, resolveAppUrl, resolveAppTitle, canonicalAppUrl,
} from './appSession/appSessionConfig'
import { launchBlockedReason, resolveAppIcon } from './apps/appsConfig'
import { PackageState } from '@/types/api'
import { useAppMediaBridge } from '@/composables/useAppMediaBridge'
import { useAppIdentity } from './appSession/useAppIdentity'
import { useNostrBridge } from './appSession/useNostrBridge'
import { openExternalUrl, openInAppOrNewTab } from '@/utils/openExternal'
import { useElectrsSync } from '@/composables/useElectrsSync'
import { IS_DEMO, isDemoApp, isDemoExternal } from '@/composables/useDemoIntro'

const props = defineProps<{
  appIdProp?: string
  /** Deep-link path inside the app (store-driven sessions), e.g. /tx/<hash> */
  pathProp?: string
  suspended?: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

/** True when rendered inline via store (panel mode), false when route-based */
const isInlinePanel = computed(() => !!props.appIdProp)

const route = useRoute()
const router = useRouter()
const store = useAppStore()
const screensaverStore = useScreensaverStore()

const sessionRef = ref<HTMLElement | null>(null)
const frameRef = ref<InstanceType<typeof AppSessionFrame> | null>(null)
const loading = ref(true)
const isRefreshing = ref(false)
const iframeBlocked = ref(false)
const refreshKey = ref(0)
const showIdentityPicker = ref(false)
const autoRetryCount = ref(0)
let loadTimeoutId: ReturnType<typeof setTimeout> | null = null
let autoRetryId: ReturnType<typeof setTimeout> | null = null
let iframeCheckId: ReturnType<typeof setTimeout> | null = null

const appId = computed(() => {
  const id = props.appIdProp || (route.params.appId as string)
  if (typeof id !== 'string' || !/^[a-z0-9][a-z0-9._-]*$/.test(id) || id.length > 64) {
    router.replace('/dashboard/apps')
    return ''
  }
  return id
})

// Display mode -- per-app user choice → per-app default → last global → panel
const displayMode = ref<DisplayMode>(initialDisplayMode(appId.value))

const packageEntry = computed(() => store.data?.['package-data']?.[appId.value] || null)
const appTitle = computed(() => packageEntry.value?.manifest?.title || resolveAppTitle(appId.value))
const appIcon = computed(() =>
  packageEntry.value
    ? resolveAppIcon(appId.value, packageEntry.value)
    : `/assets/img/app-icons/${appId.value}.png`
)
const blockedReason = computed(() => launchBlockedReason(appId.value, packageEntry.value))

// A container that is up but not yet answering its probe is STARTING, not
// broken — bitcoind serves RPC error -28 for its whole warm-up and lnd is
// unreachable until the wallet unlocks, so both spent that window reading as
// a hard "App not reachable" failure. The retry machinery below already
// tolerates it (6 × 10s); this only makes the headline tell the truth while
// those retries are still in flight. Once they are exhausted, the failure is
// real again and the copy reverts.
const MAX_AUTO_RETRIES = 6
const warmingUp = computed(() =>
  iframeBlocked.value &&
  !mustOpenNewTab.value &&
  !blockedReason.value &&
  autoRetryCount.value < MAX_AUTO_RETRIES &&
  (packageEntry.value?.state === PackageState.Running ||
   packageEntry.value?.state === PackageState.Starting ||
   packageEntry.value?.state === PackageState.Restarting ||
   packageEntry.value?.health === 'starting')
)
const blockedTitle = computed(() => appId.value === 'fedimint' || appId.value === 'fedimintd' ? 'Waiting for Bitcoin sync' : 'App not ready')
// Reactive so the overlay/teleport/footer/animation decisions track the live
// viewport (and match the CSS `md` breakpoint) instead of a stale one-shot read.
const isMobile = ref(typeof window !== 'undefined' && window.innerWidth < 768)
function updateIsMobile() { isMobile.value = window.innerWidth < 768 }
// In the demo, apps backed by a real external site that blocks iframing open
// in a new tab rather than the in-app session frame. Demoable apps are served
// same-origin by the mock backend, so the prod new-tab list doesn't apply.
const mustOpenNewTab = computed(() =>
  (NEW_TAB_APPS.has(appId.value) && !(IS_DEMO && isDemoApp(appId.value))) ||
  // Remembered embed-refusers (frame-busting, top-level-origin apps): the
  // first blocked encounter records them, every later launch is a tab app.
  isAutoTabApp(appId.value) ||
  (IS_DEMO && isDemoExternal(appId.value))
)

// The auto-tab detector: the load-timeout marked the frame blocked, the
// warming-up retry loop has given up, and the backend says the app is
// actually RUNNING — that combination is the embed-refusal signature (a
// down app is "warming up" or shows a blocked reason instead). Remember it
// so this app never shows the dead pane again; a later successful iframe
// load (onLoad) clears the memory, and entries expire on their own.
watch([iframeBlocked, warmingUp], ([blocked, warming]) => {
  if (
    blocked && !warming && !blockedReason.value &&
    (packageEntry.value?.state === PackageState.Running)
  ) {
    rememberAutoTabApp(appId.value)
  }
})

// ElectrumX shows a sync screen before its real UI (the Electrum server only
// serves clients once its index is built). Poll /electrs-status while this is
// the Electrum app; pass the status to the frame only while still syncing.
const isElectrsApp = computed(() =>
  ['electrumx', 'electrs-ui', 'archy-electrs-ui'].includes(appId.value)
)
const { status: electrsStatus, syncing: electrsSyncing, start: startElectrsPoll, stop: stopElectrsPoll } =
  useElectrsSync()
const electrsSync = computed(() =>
  isElectrsApp.value && electrsSyncing.value ? electrsStatus.value : null
)
watch(
  isElectrsApp,
  (on) => { if (on) startElectrsPoll(); else stopElectrsPoll() },
  { immediate: true }
)
const screensaverReason = computed(() => `app-session:${appId.value}`)
const screensaverSuppressedApps = new Set([
  'indeedhub',
  'jellyfin',
  'immich',
  'photoprism',
  'filebrowser',
])

const appUrl = computed(() => {
  const runtimeUrl = store.data?.['package-data']?.[appId.value]?.installed?.['interface-addresses']?.main?.['lan-address'] || undefined
  const deepPath = props.pathProp ?? (route.query.path as string | undefined)
  return canonicalAppUrl(resolveAppUrl(appId.value, deepPath, runtimeUrl))
})

function closeRouteSession() {
  const fallback = route.query.returnTo
  const fallbackPath = typeof fallback === 'string' && fallback.startsWith('/dashboard')
    ? fallback
    : '/dashboard/apps'
  router.replace(fallbackPath).catch(() => {})
}

// --- Identity & Nostr bridge ---

const iframeRef = computed(() => frameRef.value?.iframeRef ?? null)
const mediaBridge = useAppMediaBridge(appId, appUrl, iframeRef, computed(() => !props.suspended))

const identity = useAppIdentity(appId, iframeRef, showIdentityPicker)
const nostrBridge = useNostrBridge(identity.getStoredIdentity, {
  appId: () => appId.value,
  appName: () => appTitle.value,
  appUrl: () => appUrl.value,
  frameWindow: () => iframeRef.value?.contentWindow ?? null,
})

// An actual destination change invalidates consent queued for the previous app page.
watch(appUrl, () => nostrBridge.cancelPending())

// --- Display mode ---

function setMode(mode: DisplayMode) {
  if (displayMode.value === 'fullscreen' && document.fullscreenElement) {
    document.exitFullscreen().catch(() => {})
  }
  displayMode.value = mode
  // Strictly per-app: the pick is remembered for THIS app only (no global
  // key — one app's mode must never change how another opens).
  if (appId.value) localStorage.setItem(`${DISPLAY_MODE_KEY}:${appId.value}`, mode)

  // Route-based sessions (deep links) hand off to the store-driven session so
  // the app keeps floating above the dashboard instead of owning the route.
  if (!isInlinePanel.value && mode === 'panel') {
    const id = appId.value
    const launcher = useAppLauncherStore()
    const fallback = route.query.returnTo
    const fallbackPath = typeof fallback === 'string' && fallback.startsWith('/dashboard')
      ? fallback
      : '/dashboard/apps'
    router.push(fallbackPath).then(() => {
      launcher.panelAppId = id
    })
    return
  }

}

// Reactive classes based on display mode. The store-driven session honors the
// selected display mode in place: panel renders inline beside the page,
// overlay/fullscreen render above it — the underlying route never changes.
// Mobile always uses the full overlay.
const inlinePanelMode = computed(() =>
  isInlinePanel.value && !isMobile.value && displayMode.value === 'panel'
)

// Inline-mode rect emulation: the always-body-teleported backdrop pins itself
// to the placeholder's box so "inline" looks identical to the old in-place
// render while the iframe stays put in the DOM across mode switches.
const rootRef = ref<HTMLElement | null>(null)
const inlineRect = ref<{ top: number; left: number; width: number; height: number } | null>(null)
let rectObserver: ResizeObserver | null = null

function syncInlineRect() {
  const el = rootRef.value
  if (!el) return
  const r = el.getBoundingClientRect()
  // A hidden/unmounted placeholder measures 0x0 — keep the last good rect.
  if (r.width > 0 && r.height > 0) {
    inlineRect.value = { top: r.top, left: r.left, width: r.width, height: r.height }
  }
}

const inlineRectStyle = computed<Record<string, string> | undefined>(() => {
  if (props.suspended) return { display: 'none' }
  if (!inlinePanelMode.value) return undefined
  const r = inlineRect.value
  // Never paint the inline backdrop over the whole viewport while unmeasured.
  if (!r) {
    const hidden: Record<string, string> = { visibility: 'hidden' }
    return hidden
  }
  const style: Record<string, string> = {
    position: 'fixed',
    top: `${r.top}px`,
    left: `${r.left}px`,
    width: `${r.width}px`,
    height: `${r.height}px`,
    zIndex: '100',
  }
  return style
})

watch(inlinePanelMode, (on) => {
  if (on) void nextTick(syncInlineRect)
})

const backdropClasses = computed(() => {
  if (inlinePanelMode.value) return 'app-session-backdrop-inline'
  return 'app-session-backdrop-overlay'
})

const panelClasses = computed(() => {
  const base = 'app-session-panel glass-card relative overflow-hidden'
  if (inlinePanelMode.value) return `${base} app-session-inline`
  if (displayMode.value === 'fullscreen' && !isMobile.value) return `${base} app-session-fullscreen`
  return `${base} app-session-overlay`
})

// A cold/restarting upstream is held outside the iframe. Start one fresh
// load when the scanner observes HTTP readiness; no manual refresh required.
watch(() => packageEntry.value?.['ui-ready'], (ready, previous) => {
  if (ready === false) {
    if (loadTimeoutId) clearTimeout(loadTimeoutId)
    if (autoRetryId) clearTimeout(autoRetryId)
    if (iframeCheckId) clearTimeout(iframeCheckId)
    loading.value = false
  } else if (previous === false && ready === true) {
    autoRetryCount.value = 0
    refresh()
  }
})

// --- Lifecycle handlers ---

function onLoad() {
  mediaBridge.connect()
  if (loadTimeoutId) { clearTimeout(loadTimeoutId); loadTimeoutId = null }
  if (autoRetryId) { clearTimeout(autoRetryId); autoRetryId = null }
  loading.value = false
  isRefreshing.value = false
  autoRetryCount.value = 0
  // A frame that loads embeds fine — heal any stale auto-tab memory (for
  // example an app remembered as blocked before the gate learned to strip
  // frame headers).
  forgetAutoTabApp(appId.value)
  // TV/keyboard: hand focus to the app so keys (incl. the gamepad bridge's
  // virtual keyboard) flow into the iframe without needing a pointer click.
  try { if (!props.suspended) frameRef.value?.iframeRef?.focus() } catch { /* cross-origin is fine */ }
  // Check if iframe actually loaded content (same-origin only)
  iframeCheckId = setTimeout(() => {
    try {
      const iframe = frameRef.value?.iframeRef
      const doc = iframe?.contentDocument
      if (doc) {
        const body = doc.body
        if (!body || (body.children.length === 0 && body.innerText.trim() === '')) {
          iframeBlocked.value = true
        }
      }
    } catch {
      // Cross-origin -- can't check, assume OK
    }
  }, 1000)
  identity.onIframeLoadIdentity()
}

function onError() {
  if (loadTimeoutId) { clearTimeout(loadTimeoutId); loadTimeoutId = null }
  loading.value = false
  isRefreshing.value = false
  iframeBlocked.value = true
  // Auto-retry up to 6 times (60s total) for apps that are still starting
  if (!mustOpenNewTab.value && autoRetryCount.value < MAX_AUTO_RETRIES) {
    autoRetryId = setTimeout(() => {
      autoRetryCount.value++
      refresh()
    }, 10000)
  }
}

function refresh() {
  if (autoRetryId) { clearTimeout(autoRetryId); autoRetryId = null }
  isRefreshing.value = true
  loading.value = true
  iframeBlocked.value = false
  refreshKey.value++
  startLoadTimeout()
}

function startLoadTimeout() {
  if (loadTimeoutId) clearTimeout(loadTimeoutId)
  if (packageEntry.value?.['ui-ready'] === false) return
  loadTimeoutId = setTimeout(() => {
    if (loading.value) {
      loading.value = false
      iframeBlocked.value = true
    }
  }, 12000)
}

function openNewTabAndBack() {
  if (packageEntry.value?.['ui-ready'] === false) return
  if (appUrl.value) openExternalUrl(appUrl.value)
  closeSession()
}

function openNewTab() {
  if (packageEntry.value?.['ui-ready'] === false) return
  if (appUrl.value) openExternalUrl(appUrl.value)
}

function iframeGoBack() {
  try { frameRef.value?.iframeRef?.contentWindow?.history.back() } catch {}
}

function iframeGoForward() {
  try { frameRef.value?.iframeRef?.contentWindow?.history.forward() } catch {}
}

function handleBackdropClick() {
  closeSession()
}

function closeSession() {
  nostrBridge.cancelPending()
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {})
  if (isInlinePanel.value) emit('close')
  else closeRouteSession()
}

function onKeyDown(e: KeyboardEvent) {
  if (props.suspended) return
  if (e.key === 'Escape') {
    if (nostrBridge.showConsent.value) {
      nostrBridge.denyConsent()
      e.preventDefault()
      return
    }
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {})
    else closeSession()
    e.preventDefault()
  }
}

function onFullscreenChange() {
  if (!document.fullscreenElement && displayMode.value === 'fullscreen') {
    displayMode.value = 'overlay'
    if (appId.value) localStorage.setItem(`${DISPLAY_MODE_KEY}:${appId.value}`, 'overlay')
  }
}

function onMessage(e: MessageEvent) {
  if (e.source !== iframeRef.value?.contentWindow) return
  mediaBridge.handle(e)
  if (e.data?.type === 'nostr-request') nostrBridge.handleNostrRequest(e)
  if (e.data?.type === 'archipelago:identity:request') identity.handleIdentityRequest(e.data?.force === true)
  if (e.data?.type === 'archipelago:media:playing') screensaverStore.suppress(screensaverReason.value)
  if (e.data?.type === 'archipelago:media:idle') screensaverStore.resume(screensaverReason.value)
}

// Enter fullscreen on mount if mode is fullscreen
watch(displayMode, (mode) => {
  if (mode !== 'fullscreen') return
  // The panel may teleport to <body> on this mode change — request fullscreen
  // after the DOM settles so we grab the element at its new location.
  void nextTick(() => {
    if (displayMode.value === 'fullscreen' && sessionRef.value && !document.fullscreenElement) {
      sessionRef.value.requestFullscreen().catch(() => {})
    }
  })
})

onMounted(() => {
  // Apps that block iframes (X-Frame-Options) can't be shown in the session.
  // Open them directly instead of showing a "this app opens in a tab"
  // interstitial: desktop → new browser tab; mobile → in-app WebView (companion)
  // or new tab (PWA). Then dismiss the (empty) session surface.
  if (mustOpenNewTab.value && appUrl.value) {
    if (isMobile.value) openInAppOrNewTab(appUrl.value)
    else window.open(appUrl.value, '_blank', 'noopener,noreferrer')
    if (isInlinePanel.value) emit('close')
    else closeRouteSession()
    return
  }

  window.addEventListener('keydown', onKeyDown, true)
  window.addEventListener('message', onMessage)
  window.addEventListener('resize', updateIsMobile)
  window.addEventListener('resize', syncInlineRect)
  document.addEventListener('fullscreenchange', onFullscreenChange)
  // Track the placeholder's box (sidebar collapse, layout shifts) for the
  // inline-mode fixed-position emulation. Sync before first paint so the
  // inline backdrop never flashes at the wrong rect.
  syncInlineRect()
  if (rootRef.value && typeof ResizeObserver !== 'undefined') {
    rectObserver = new ResizeObserver(syncInlineRect)
    rectObserver.observe(rootRef.value)
  }
  if (IFRAME_BLOCKED_APPS.has(appId.value)) {
    loading.value = false
    iframeBlocked.value = true
  } else {
    startLoadTimeout()
  }
  if (displayMode.value === 'fullscreen') {
    requestAnimationFrame(() => {
      sessionRef.value?.requestFullscreen().catch(() => {})
    })
  }
  if (screensaverSuppressedApps.has(appId.value)) {
    screensaverStore.suppress(screensaverReason.value)
  }
})

onBeforeUnmount(() => {
  nostrBridge.dispose()
  if (loadTimeoutId) clearTimeout(loadTimeoutId)
  if (autoRetryId) clearTimeout(autoRetryId)
  if (iframeCheckId) clearTimeout(iframeCheckId)
  window.removeEventListener('keydown', onKeyDown, true)
  window.removeEventListener('message', onMessage)
  window.removeEventListener('resize', updateIsMobile)
  window.removeEventListener('resize', syncInlineRect)
  document.removeEventListener('fullscreenchange', onFullscreenChange)
  rectObserver?.disconnect()
  rectObserver = null
  screensaverStore.resume(screensaverReason.value)
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {})
})
</script>

<style>
/* Unscoped so children can use these classes */
.app-session-root {
  width: 100%;
  height: 100%;
}
/* Inline panel mode -- fills content area, no blur, original layout */
.app-session-backdrop-inline {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.4);
  padding: 0;
}

.app-session-inline {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  width: 100%;
  height: 100%;
  border-radius: 0;
  border: none;
}

@media (min-width: 768px) {
  .app-session-backdrop-inline {
    padding: 1.5rem;
  }
  .app-session-inline {
    border-radius: 1rem;
    max-width: calc(100% - 1rem);
    max-height: calc(100vh - 6rem);
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  }
}

/* Overlay mode -- covers entire viewport including sidebar */
.app-session-backdrop-overlay {
  position: fixed;
  inset: 0;
  z-index: 2400;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(12px);
}

@media (min-width: 768px) {
  .app-session-backdrop-overlay {
    padding: 2.5rem;
  }
}

.app-session-overlay {
  position: relative;
  z-index: 10;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  width: 100%;
  height: 100%;
  border-radius: 0;
  border: none;
  box-shadow: none;
}

@media (min-width: 768px) {
  .app-session-overlay {
    max-width: calc(100vw - 5rem);
    max-height: calc(100vh - 5rem);
    border-radius: 1rem;
  }
}

/* Fullscreen mode */
.app-session-fullscreen {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  width: 100%;
  height: 100%;
  border-radius: 0 !important;
  max-width: none !important;
  max-height: none !important;
}

/* Shared */
.app-session-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.7);
  transition: all 0.15s ease;
  flex-shrink: 0;
}
.app-session-btn:hover {
  background: rgba(255, 255, 255, 0.15);
  color: white;
}
.app-session-btn:disabled {
  opacity: 0.5;
}

/* Active display-mode button in the header bar */
.app-session-btn-active {
  color: #fb923c;
  background: rgba(251, 146, 60, 0.12);
}
.app-session-btn-active:hover {
  color: #fb923c;
  background: rgba(251, 146, 60, 0.18);
}

.content-fade-enter-active,
.content-fade-leave-active {
  transition: opacity 0.2s ease;
}
.content-fade-enter-from,
.content-fade-leave-to {
  opacity: 0;
}

.app-session-frame-scroll-host {
  overflow: auto;
  overscroll-behavior: contain;
  -webkit-overflow-scrolling: touch;
}

/* Mobile: full-bleed app sessions — no border, no radius, no shadow */
@media (max-width: 767px) {
  .app-session-root {
    height: 100%;
  }
  .app-session-inline {
    height: 100%;
  }
  .app-session-overlay,
  .app-session-fullscreen {
    height: 100vh;
    height: 100dvh;
    height: var(--visual-viewport-height, 100dvh);
    padding-top: var(--safe-area-top, env(safe-area-inset-top, 0px));
    background: black;
  }
  .app-session-panel.glass-card {
    border: none !important;
    border-radius: 0 !important;
    box-shadow: none !important;
  }
  .app-session-backdrop-overlay {
    padding: 0;
    backdrop-filter: none;
    background: black;
  }
  .app-session-frame-safe {
    flex: none !important;
    height: calc(100vh - var(--app-session-mobile-bar-height, 84px) - var(--safe-area-top, env(safe-area-inset-top, 0px)));
    height: calc(100dvh - var(--app-session-mobile-bar-height, 84px) - var(--safe-area-top, env(safe-area-inset-top, 0px)));
    height: calc(var(--visual-viewport-height, 100dvh) - var(--app-session-mobile-bar-height, 84px) - var(--safe-area-top, env(safe-area-inset-top, 0px)));
    padding-bottom: 0;
  }
}

/* Mobile bottom browser bar — sized like the main tab bar.
   Uses !important-free display so Tailwind md:hidden can override. */
@media (min-width: 768px) {
  .app-session-mobile-bar { display: none !important; }
}
.app-session-mobile-bar {
  display: flex;
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 2600;
  justify-content: space-around;
  align-items: center;
  flex-shrink: 0;
  min-height: var(--app-session-mobile-bar-height, 84px);
  padding: 10px 16px;
  padding-bottom: calc(10px + max(var(--safe-area-bottom, 0px), env(safe-area-inset-bottom, 0px), 10px));
  /* Solid black, not translucent: the app iframe's theme colour bled
     through the bar and its safe-area strip on phones. */
  background: #000;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  transform: translateZ(0);
}

.app-session-inline .app-session-mobile-bar {
  position: absolute;
  z-index: 20;
}


.app-session-bar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  min-height: 52px;
  border-radius: 13px;
  color: rgba(255, 255, 255, 0.65);
  transition: color 0.15s ease, background 0.15s ease;
}
.app-session-bar-btn svg {
  width: 24px;
  height: 24px;
}
.app-session-bar-btn:active {
  color: white;
  background: rgba(255, 255, 255, 0.12);
}
</style>
