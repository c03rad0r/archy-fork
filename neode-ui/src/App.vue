<template>
  <div id="app">
    <!-- Splash Screen (only on first visit) -->
    <SplashScreen v-if="showSplash" @complete="handleSplashComplete" />

    <!-- Main App Content - only show after splash and routing is complete -->
    <div v-if="!showSplash && !isReady" class="min-h-screen bg-black" />
    <RouterView v-else-if="!showSplash && isReady" />
    
    <!-- Spotlight command palette (Cmd+K / Ctrl+K) -->
    <SpotlightSearch />

    <!-- CLI popup (F key) -->
    <CLIPopup />

    <!-- App launcher overlay (iframe popup) -->
    <AppLauncherOverlay />

    <!-- Global toast notifications -->
    <ToastStack />

    <!-- Screensaver -->
    <Screensaver />

    <!-- Help guide modal (from spotlight) -->
    <HelpGuideModal
      :show="spotlightStore.helpModal.show"
      :title="spotlightStore.helpModal.title"
      :content="spotlightStore.helpModal.content"
      :related-path="spotlightStore.helpModal.relatedPath"
      @close="spotlightStore.closeHelpModal()"
    />
    
    <!-- PWA Update Prompt -->
    <PWAUpdatePrompt />

    <!-- PWA Install Prompt (Install app, not just Add to Home Screen) -->
    <PWAInstallPrompt />

    <!-- Global "mesh device detected" setup flow (fires on any page) -->
    <MeshDeviceSetupModal />
    <ExternalExplorerModal />

    <!-- In-app confirm() replacement — native browser dialogs block the JS
         event loop and freeze companion remote input (see useAppConfirm) -->
    <AppConfirmModal />

    <!-- Nudge to back up the Lightning seed once a wallet exists (any page) -->
    <LndSeedBackupPrompt />

    <!-- "You need a Lightning node" install prompt. Global because it is
         raised from inside other modals (wallet Receive, the Web5 sheet, the
         app launcher's paywall) — one instance, shared state. -->
    <LightningRequiredModal />

    <!-- Global persistent audio player (bottom bar) -->
    <GlobalAudioPlayer />

    <!-- Toast notifications - top right, glass style, any page -->
    <Teleport to="body">
      <Transition name="toast">
        <div
          v-if="toastMessage.show"
          @click="messageToast.dismissToastAndOpenMessages"
          class="fixed top-20 right-4 left-4 z-[100] w-auto max-w-md cursor-pointer rounded-xl p-4 transition-all hover:border-white/30 hover:shadow-2xl md:top-6 md:right-6 md:left-auto md:max-w-md toast-glass"
        >
          <div class="flex items-start gap-3">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-orange-500/20">
              <svg class="h-5 w-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
              </svg>
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium text-white">New message</p>
              <p class="mt-0.5 text-sm text-white/70 line-clamp-2">{{ toastMessage.text }}</p>
              <p class="mt-1 text-xs text-orange-400">Click to view</p>
            </div>
            <button
              @click.stop="messageToast.closeToast"
              aria-label="Dismiss notification"
              class="-mt-1 -mr-1 shrink-0 rounded-full p-1 text-white/40 transition-colors hover:bg-white/10 hover:text-white/80"
            >
              <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import SplashScreen from './components/SplashScreen.vue'
import PWAUpdatePrompt from './components/PWAUpdatePrompt.vue'
import PWAInstallPrompt from './components/PWAInstallPrompt.vue'
import SpotlightSearch from './components/SpotlightSearch.vue'
import CLIPopup from './components/CLIPopup.vue'
import AppLauncherOverlay from './components/AppLauncherOverlay.vue'
import ToastStack from './components/ToastStack.vue'
import Screensaver from './components/Screensaver.vue'
import HelpGuideModal from './components/HelpGuideModal.vue'
import GlobalAudioPlayer from './components/GlobalAudioPlayer.vue'
import MeshDeviceSetupModal from './components/mesh/MeshDeviceSetupModal.vue'
import ExternalExplorerModal from './components/ExternalExplorerModal.vue'
import AppConfirmModal from './components/AppConfirmModal.vue'
import LndSeedBackupPrompt from './components/LndSeedBackupPrompt.vue'
import LightningRequiredModal from './components/LightningRequiredModal.vue'
import { useMeshStore } from './stores/mesh'

import { useControllerNav } from '@/composables/useControllerNav'
import { playKeyboardTypingSound } from '@/composables/useLoginSounds'
import { useSpotlightStore } from '@/stores/spotlight'
import { useCLIStore } from '@/stores/cli'
import { useMessageToast } from '@/composables/useMessageToast'
import { useAppStore } from '@/stores/app'
import { useScreensaverStore } from '@/stores/screensaver'
import { useUIModeStore } from '@/stores/uiMode'
import { startRemoteRelay, stopRemoteRelay } from '@/api/remote-relay'
import { shouldShowIntroSplash } from '@/utils/introSplash'
import { isCompanionApp } from '@/utils/openExternal'

const router = useRouter()
const screensaverStore = useScreensaverStore()
const spotlightStore = useSpotlightStore()
const cliStore = useCLIStore()
const appStore = useAppStore()
const uiModeStore = useUIModeStore()
const messageToast = useMessageToast()
const toastMessage = messageToast.toastMessage

useControllerNav()

function syncKioskSafeArea() {
  if (typeof document === 'undefined') return
  const isKiosk = localStorage.getItem('kiosk') === 'true'
    || new URLSearchParams(window.location.search).has('kiosk')
    // Very first kiosk boot: the launcher opens /kiosk before the route
    // guard has had a chance to persist localStorage.kiosk.
    || window.location.pathname === '/kiosk'
  // Global styling hook: kiosk chromium runs software-composited
  // (--in-process-gpu / --disable-gpu), where heavy compositing effects
  // (3D bg layers, animated mix-blend overlays) fail and paint black.
  document.documentElement.classList.toggle('kiosk-mode', isKiosk)
  if (isKiosk) {
    // AIUI (same-origin iframe) themes from localStorage['aiui-theme'], and a
    // stored value beats its prefers-color-scheme fallback. Kiosk profiles
    // that ran before the launcher forced a dark color-scheme have "light"
    // baked in — white panels on an otherwise dark TV UI — so pin it dark.
    try { localStorage.setItem('aiui-theme', 'dark') } catch { /* storage full/denied */ }
  }
  const rawSafeArea = localStorage.getItem('archipelago_kiosk_safe_area_px') || '0'
  const safeArea = /^\d{1,3}$/.test(rawSafeArea) ? Number(rawSafeArea) : 0
  const rawSafeAreaX = localStorage.getItem('archipelago_kiosk_safe_area_x_px') || rawSafeArea
  const rawSafeAreaY = localStorage.getItem('archipelago_kiosk_safe_area_y_px') || rawSafeArea
  const safeAreaX = /^\d{1,3}$/.test(rawSafeAreaX) ? Number(rawSafeAreaX) : safeArea
  const safeAreaY = /^\d{1,3}$/.test(rawSafeAreaY) ? Number(rawSafeAreaY) : safeArea
  document.documentElement.classList.toggle('kiosk-safe-area', isKiosk && (safeAreaX > 0 || safeAreaY > 0))
  if (isKiosk && (safeAreaX > 0 || safeAreaY > 0)) {
    document.documentElement.style.setProperty('--kiosk-safe-area-x', `${safeAreaX}px`)
    document.documentElement.style.setProperty('--kiosk-safe-area-y', `${safeAreaY}px`)
  } else {
    document.documentElement.style.removeProperty('--kiosk-safe-area-x')
    document.documentElement.style.removeProperty('--kiosk-safe-area-y')
  }
}

// Start/stop message polling and remote relay when auth state changes
watch(() => appStore.isAuthenticated, (authenticated) => {
  if (authenticated) {
    messageToast.startPolling()
    screensaverStore.resetInactivityTimer()
    // Kiosk included: the backend's remote-input path is relay-only (it
    // validates and broadcasts; it never runs xdotool), so the browser-side
    // relay is the ONLY consumer of companion input. Skipping it on kiosk —
    // an assumption from a long-gone system-level xdotool injector — left
    // TVs deaf to the companion remote.
    startRemoteRelay()
  } else {
    messageToast.stopPolling()
    toastMessage.value = { show: false, text: '', fromPubkey: '' }
    screensaverStore.clearInactivityTimer()
    screensaverStore.deactivate()
    stopRemoteRelay()
  }
}, { immediate: true })

// Reset screensaver inactivity on user activity (when authenticated)
function onUserActivity() {
  if (appStore.isAuthenticated && !screensaverStore.isActive) {
    screensaverStore.resetInactivityTimer()
  }
}

function onKeyDown(e: KeyboardEvent) {
  const isMac = navigator.platform.toUpperCase().includes('MAC')
  const mod = isMac ? e.metaKey : e.ctrlKey
  // Cmd+K / Ctrl+K only (modifier required - avoids accidental trigger when typing)
  const target = e.target as HTMLElement
  const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
  if (mod && e.key === 'k') {
    e.preventDefault()
    spotlightStore.toggle()
    return
  }
  // F key - CLI popup (skip when in input or modifier held)
  if ((e.key === 'f' || e.key === 'F') && !isInput && !mod && !e.altKey) {
    e.preventDefault()
    cliStore.toggle()
    return
  }
  // Cmd+1/2/3 - switch UI mode (skip when in input)
  if (mod && !isInput && appStore.isAuthenticated) {
    if (e.key === '1') { e.preventDefault(); uiModeStore.setMode('easy'); router.push('/dashboard'); return }
    if (e.key === '2') { e.preventDefault(); uiModeStore.setMode('gamer'); router.push('/dashboard'); return }
    if (e.key === '3') { e.preventDefault(); router.push('/dashboard/chat'); return }
  }
  // Cmd+M / Ctrl+M - cycle UI mode (skip when in input)
  if (mod && (e.key === 'm' || e.key === 'M') && !isInput && appStore.isAuthenticated) {
    e.preventDefault()
    uiModeStore.cycleMode()
    router.push('/dashboard')
    return
  }
  // 's' key activates screensaver when authenticated (skip if typing in input)
  if (e.key === 's' || e.key === 'S') {
    if (!isInput && appStore.isAuthenticated && !screensaverStore.isActive && !screensaverStore.isSuppressed) {
      e.preventDefault()
      screensaverStore.activate()
    }
  }
  // Keyboard typing sound - plays on any character typed in inputs (global)
  if (isInput && e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
    playKeyboardTypingSound()
  }
}

const route = useRoute()
// Start with splash hidden — onMounted decides whether to show it
const showSplash = ref(false)
const isReady = ref(false)
let modalOverlayObserver: MutationObserver | null = null
let lockedScrollY = 0
let previousBodyStyles: Partial<CSSStyleDeclaration> = {}
let bodyLockedForModal = false
let modalTouchY: number | null = null

function hasBlockingOverlay() {
  if (typeof document === 'undefined') return false
  return Array.from(document.querySelectorAll<HTMLElement>('.fixed.inset-0'))
    .some((el) => {
      const style = window.getComputedStyle(el)
      const rect = el.getBoundingClientRect()
      return style.display !== 'none'
        && style.visibility !== 'hidden'
        && style.pointerEvents !== 'none'
        && rect.width > 0
        && rect.height > 0
    })
}

function visibleBlockingOverlays() {
  if (typeof document === 'undefined') return []
  return Array.from(document.querySelectorAll<HTMLElement>('.fixed.inset-0'))
    .filter((el) => {
      const style = window.getComputedStyle(el)
      const rect = el.getBoundingClientRect()
      return style.display !== 'none'
        && style.visibility !== 'hidden'
        && style.pointerEvents !== 'none'
        && rect.width > 0
        && rect.height > 0
    })
}

function closestOverlay(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return null
  return visibleBlockingOverlays().find((overlay) => overlay.contains(target)) || null
}

function canScrollInsideOverlay(target: EventTarget | null, overlay: HTMLElement, deltaY: number) {
  if (!(target instanceof HTMLElement)) return false
  let el: HTMLElement | null = target
  while (el && overlay.contains(el)) {
    const style = window.getComputedStyle(el)
    const canScrollY = /(auto|scroll)/.test(style.overflowY)
      && el.scrollHeight > el.clientHeight
    if (canScrollY) {
      if (deltaY < 0 && el.scrollTop > 0) return true
      if (deltaY > 0 && el.scrollTop + el.clientHeight < el.scrollHeight - 1) return true
    }
    if (el === overlay) break
    el = el.parentElement
  }
  return false
}

function containModalWheel(ev: WheelEvent) {
  if (!bodyLockedForModal) return
  const overlay = closestOverlay(ev.target)
  if (!overlay || !canScrollInsideOverlay(ev.target, overlay, ev.deltaY)) {
    ev.preventDefault()
  }
}

function containModalTouchStart(ev: TouchEvent) {
  modalTouchY = ev.touches[0]?.clientY ?? null
}

function containModalTouchMove(ev: TouchEvent) {
  if (!bodyLockedForModal) return
  const currentY = ev.touches[0]?.clientY
  if (currentY === undefined || modalTouchY === null) {
    ev.preventDefault()
    return
  }
  const deltaY = modalTouchY - currentY
  modalTouchY = currentY
  const overlay = closestOverlay(ev.target)
  if (!overlay || !canScrollInsideOverlay(ev.target, overlay, deltaY)) {
    ev.preventDefault()
  }
}

function lockBodyForModal() {
  if (bodyLockedForModal || typeof document === 'undefined') return
  lockedScrollY = window.scrollY || document.documentElement.scrollTop || 0
  previousBodyStyles = {
    position: document.body.style.position,
    top: document.body.style.top,
    left: document.body.style.left,
    right: document.body.style.right,
    width: document.body.style.width,
    overflow: document.body.style.overflow,
  }
  document.body.style.position = 'fixed'
  document.body.style.top = `-${lockedScrollY}px`
  document.body.style.left = '0'
  document.body.style.right = '0'
  document.body.style.width = '100%'
  document.body.style.overflow = 'hidden'
  document.documentElement.classList.add('modal-scroll-locked')
  bodyLockedForModal = true
}

function unlockBodyForModal() {
  if (!bodyLockedForModal || typeof document === 'undefined') return
  document.body.style.position = previousBodyStyles.position || ''
  document.body.style.top = previousBodyStyles.top || ''
  document.body.style.left = previousBodyStyles.left || ''
  document.body.style.right = previousBodyStyles.right || ''
  document.body.style.width = previousBodyStyles.width || ''
  document.body.style.overflow = previousBodyStyles.overflow || ''
  document.documentElement.classList.remove('modal-scroll-locked')
  window.scrollTo(0, lockedScrollY)
  previousBodyStyles = {}
  bodyLockedForModal = false
  modalTouchY = null
}

function syncModalBodyLock() {
  if (hasBlockingOverlay()) lockBodyForModal()
  else unlockBodyForModal()
}

/**
 * Determine if splash screen should be shown
 * Splash is skipped if:
 * - User has already seen the intro
 * - User is on a direct route (refresh/bookmark)
 */
// Fix Chromium backdrop-filter rendering bug: when tab loses/regains focus,
// the compositor fails to repaint backdrop-filter layers over animated
// fixed-position overlays (body::before/after with mix-blend-mode).
// On return: strip backdrop-filter via class, wait a frame, then restore.
function onVisibilityChange() {
  if (document.hidden) {
    document.documentElement.classList.add('tab-hidden')
  } else {
    // Step 1: strip backdrop-filter while animations stay paused (tab-hidden)
    document.documentElement.classList.add('no-backdrop')
    // Step 2: restore backdrop-filter over static content (clean compositor rebuild)
    // Use setTimeout — Chromium batches rAFs on tab return
    setTimeout(() => {
      document.documentElement.classList.remove('no-backdrop')
      // Step 3: resume animations after backdrop-filter layers are established
      requestAnimationFrame(() => {
        document.documentElement.classList.remove('tab-hidden')
      })
    }, 50)
  }
}

onMounted(async () => {
  syncKioskSafeArea()
  // Light app-wide mesh poll so a freshly plugged-in radio surfaces the
  // setup modal on any page (the Mesh view's own poll takes over there).
  useMeshStore().startGlobalDetection()
  document.addEventListener('visibilitychange', onVisibilityChange)
  window.addEventListener('keydown', onKeyDown, true)
  window.addEventListener('mousemove', onUserActivity)
  window.addEventListener('mousedown', onUserActivity)
  window.addEventListener('keydown', onUserActivity)
  window.addEventListener('touchstart', onUserActivity)
  window.addEventListener('message', onShareToMeshMessage)
  document.addEventListener('wheel', containModalWheel, { capture: true, passive: false })
  document.addEventListener('touchstart', containModalTouchStart, { capture: true, passive: true })
  document.addEventListener('touchmove', containModalTouchMove, { capture: true, passive: false })
  modalOverlayObserver = new MutationObserver(() => {
    requestAnimationFrame(syncModalBodyLock)
  })
  modalOverlayObserver.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['class', 'style'],
  })
  syncModalBodyLock()
  let seenIntro = localStorage.getItem('neode_intro_seen') === '1'
  const fromBoot = sessionStorage.getItem('archipelago_from_boot') === '1'
  if (fromBoot) sessionStorage.removeItem('archipelago_from_boot')
  // One-shot "Replay intro" request from the login screen — must survive the
  // auto-re-mark below (which otherwise instantly suppresses the replay on any
  // onboarded node).
  let replayRequested = sessionStorage.getItem('archipelago_replay_intro') === '1'
  if (replayRequested) sessionStorage.removeItem('archipelago_replay_intro')
  // The route object still holds the router's START_LOCATION ('/') here —
  // the initial navigation resolves asynchronously — so a deep-route load
  // (e.g. a direct /login visit) would masquerade as a root boot. The URL
  // bar is the only truthful source this early.
  //
  // The kiosk chromium opens /kiosk — a marker route whose beforeEnter
  // stamps localStorage.kiosk and immediately redirects to '/'. That is a
  // root boot, not a deep route: without normalizing it, the fresh-install
  // kiosk never plays the typing intro (deep-route suppression regression).
  const rawBootPath = window.location.pathname
  const bootPath = rawBootPath === '/kiosk' ? '/' : rawBootPath
  // Public demo: every fresh boot at the root (first visit or a browser
  // refresh) starts with the typing splash for the full effect. In-session SPA
  // navigation never remounts App, and deep-route refreshes keep their place.
  const { IS_DEMO } = await import('@/composables/useDemoIntro')
  // Companion in-app demo never requests the splash replay; browser demo unaffected.
  if (IS_DEMO && bootPath === '/' && !isCompanionApp()) replayRequested = true
  let onboardingComplete: boolean | null = localStorage.getItem('neode_onboarding_complete') === '1' ? true : null
  // Root boots always ask the backend — even when this browser thinks it has
  // seen the intro. Both `neode_intro_seen` and `neode_onboarding_complete`
  // are per-origin browser state: after a reinstall (or another node coming
  // up on a DHCP-recycled IP) they describe the PREVIOUS node and would mute
  // a fresh install's intro / misroute it to login.
  const splashCandidate = fromBoot || (bootPath === '/' && import.meta.env.VITE_DEV_MODE !== 'boot')

  if (splashCandidate) {
    try {
      const { checkOnboardingStatus } = await import('@/composables/useOnboarding')
      // Bound the pre-splash status check: its retry ladder can spend ~30s
      // against a still-booting backend, and this await holds the black
      // "!isReady" screen the whole time — exactly where the typing intro
      // should be playing on a fresh kiosk boot. Unknown within 2.5s → let
      // the splash play (a fresh install IS the slow-backend case; onboarded
      // nodes answer in milliseconds, so their suppression path is intact).
      // handleSplashComplete re-checks with full retries after the intro.
      const live = await Promise.race([
        checkOnboardingStatus(),
        new Promise<null>((resolve) => setTimeout(() => resolve(null), 2500)),
      ])
      if (live !== null) onboardingComplete = live
      if (live === false && seenIntro) {
        // Backend-confirmed fresh node behind a browser with a stale flag —
        // drop it so this boot (and every later one) plays the intro.
        try { localStorage.removeItem('neode_intro_seen') } catch { /* noop */ }
        seenIntro = false
      }
    } catch {
      onboardingComplete = localStorage.getItem('neode_onboarding_complete') === '1' ? true : null
    }
  }

  if (!replayRequested && !seenIntro && onboardingComplete === true) {
    try { localStorage.setItem('neode_intro_seen', '1') } catch { /* noop */ }
    seenIntro = true
  }

  if (import.meta.env.DEV) console.log('[App] onMounted — seenIntro:', seenIntro, 'fromBoot:', fromBoot, 'onboardingComplete:', onboardingComplete)

  if (shouldShowIntroSplash({
    seenIntro,
    routePath: bootPath,
    fromBoot,
    devMode: import.meta.env.VITE_DEV_MODE,
    onboardingComplete,
    replayRequested,
  })) {
    // The intro is definitely playing — unmute the whole cinematic (speech,
    // synthwave, pops) even on browsers whose localStorage says onboarding is
    // complete; the sound gate otherwise silences replays.
    const { enableCinematicSounds } = await import('@/composables/useLoginSounds')
    enableCinematicSounds()
    // Kick off the intro video download NOW: the splash's <video> element only
    // mounts ~20s in (after the typing sequence), and on a cold cache the file
    // would otherwise start fetching mid-sequence and stutter. A detached
    // <video preload=auto> is used because Chromium does not support
    // <link rel=preload as=video> — the media cache then serves the splash's
    // element, which uses the identical URL.
    try {
      const warm = document.createElement('video')
      warm.muted = true
      warm.preload = 'auto'
      warm.src = '/assets/video/video-intro.mp4?v=8'
      ;(window as unknown as { __introVideoWarm?: HTMLVideoElement }).__introVideoWarm = warm
    } catch { /* ignore */ }
    // Coming from boot screen — show the full splash intro (Enter to Exit → typing → logo)
    showSplash.value = true
  } else {
    // Already seen intro, direct route, or boot mode (boot screen handles intro)
    // Set isReady BEFORE hiding splash to prevent flash of partial content
    await router.isReady()
    isReady.value = true
    showSplash.value = false
    document.body.classList.add('splash-complete')
  }
})

onBeforeUnmount(() => {
  document.documentElement.classList.remove('kiosk-safe-area')
  document.documentElement.style.removeProperty('--kiosk-safe-area-x')
  document.documentElement.style.removeProperty('--kiosk-safe-area-y')
  document.removeEventListener('visibilitychange', onVisibilityChange)
  window.removeEventListener('keydown', onKeyDown, true)
  window.removeEventListener('mousemove', onUserActivity)
  window.removeEventListener('mousedown', onUserActivity)
  window.removeEventListener('keydown', onUserActivity)
  window.removeEventListener('touchstart', onUserActivity)
  window.removeEventListener('message', onShareToMeshMessage)
  document.removeEventListener('wheel', containModalWheel, { capture: true })
  document.removeEventListener('touchstart', containModalTouchStart, { capture: true })
  document.removeEventListener('touchmove', containModalTouchMove, { capture: true })
  modalOverlayObserver?.disconnect()
  modalOverlayObserver = null
  unlockBodyForModal()
})

/**
 * Phase 3c: marketplace app iframes share files into mesh chats by POSTing
 * to /api/share-to-mesh then postMessaging the CID back to this parent
 * window. We stash it in sessionStorage + route to /mesh; Mesh.vue reads the
 * stash on mount and stages it as a pending attachment.
 */
function onShareToMeshMessage(ev: MessageEvent) {
  // Same-origin senders only (matches Chat.vue's handler) — otherwise any
  // embedded frame can force-navigate the UI to /mesh with a staged CID.
  if (ev.origin !== window.location.origin) return
  const data = ev.data as { type?: string; cid?: string } | null
  if (!data || data.type !== 'share-to-mesh' || !data.cid) return
  try {
    sessionStorage.setItem('archipelago_share_to_mesh', JSON.stringify(data))
  } catch {
    /* quota — fall through */
  }
  if (route.path !== '/mesh') {
    router.push('/mesh')
  } else {
    // Already on /mesh — dispatch a synthetic event so the view picks it up.
    window.dispatchEvent(new CustomEvent('archipelago:share-to-mesh'))
  }
}

/**
 * Handle splash screen completion
 * Routes user directly to appropriate screen based on onboarding status (from backend)
 */
async function handleSplashComplete() {
  sessionStorage.setItem('archipelago_from_splash', '1')

  // Commit the destination route BEFORE revealing RouterView. Revealing
  // first rendered whatever route was current — '/' → RootRedirect's
  // spinner — for a beat between the intro's final frame and the
  // onboarding/login screen actually mounting: a visible flash on every
  // fresh install. Await the push so the first RouterView render is
  // already the destination.
  const reveal = () => {
    showSplash.value = false
    document.body.classList.add('splash-complete')
    isReady.value = true
  }

  const devMode = import.meta.env.VITE_DEV_MODE
  if (devMode === 'setup' || devMode === 'existing') {
    await router.push('/login').catch(() => {})
    reveal()
    return
  }

  // Demo: the cinematic always continues into the onboarding intro page —
  // the mock backend reports "onboarded", which would otherwise route
  // straight to /login and cut the sequence short.
  {
    const { IS_DEMO } = await import('@/composables/useDemoIntro')
    if (IS_DEMO) {
      // Companion in-app demo skips the intro and lands on /login; browser demo unaffected.
      if (isCompanionApp()) {
        await router.push('/login').catch(() => {})
        reveal()
        return
      }
      await router.push('/onboarding/intro').catch(() => {})
      reveal()
      return
    }
  }

  try {
    const { checkOnboardingStatus } = await import('@/composables/useOnboarding')
    const seenOnboarding = await checkOnboardingStatus()
    if (seenOnboarding === true) {
      await router.push('/login').catch(() => {})
    } else if (seenOnboarding === false) {
      await router.push('/onboarding/intro').catch(() => {})
    } else if (localStorage.getItem('neode_onboarding_complete') === '1') {
      // Backend unreachable after retries. Prefer the localStorage
      // cache on THIS browser (if a prior successful check set it) —
      // otherwise defer to RootRedirect which polls + retries rather
      // than forcing an already-onboarded user through the wizard.
      await router.push('/login').catch(() => {})
    } else {
      await router.push('/').catch(() => {})
    }
  } catch {
    // Do NOT default to /onboarding/intro here. RootRedirect has retry
    // + polling + boot-screen handling; let it decide.
    await router.push('/').catch(() => {})
  }
  reveal()
}
</script>

<style>
/* Global styles are in style.css */
</style>
