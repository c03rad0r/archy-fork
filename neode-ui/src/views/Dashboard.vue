<template>
  <div class="min-h-screen flex relative dashboard-view" :class="{ 'glass-throw-active': showZoomIn }">
    <!-- Skip to main content link for keyboard users -->
    <!-- Skip-to-content handled by controller nav sidebar→main transition -->
    <!-- Background container with 3D perspective - full width to avoid letterboxing -->
    <div class="bg-perspective-container">
      <!-- Background - primary layer (visible for all routes, transitions out only for detail pages) -->
      <div
        ref="bgDefault"
        class="bg-layer bg-fullwidth"
        :class="[
          { 'bg-transitioning-out': showAltBackground },
          { 'zoom-reveal-bg': showZoomIn }
        ]"
        :style="{ backgroundImage: `url(/assets/img/${backgroundImage})` }"
      />
      <!-- Background - detail layer (only visible during app/marketplace detail 3D transition) -->
      <div
        ref="bgAlt"
        class="bg-layer bg-fullwidth"
        :class="{ 'bg-transitioning-in': showAltBackground }"
        style="background-image: url(/assets/img/bg-intro-3.jpg)"
      />
      <!-- Glitch overlays - trigger on background change -->
      <div
        class="bg-glitch-layer-1"
        :class="{ 'glitch-active': isGlitching }"
        :style="{ backgroundImage: `url(/assets/img/${backgroundImage})` }"
      />
      <div
        class="bg-glitch-layer-2"
        :class="{ 'glitch-active': isGlitching }"
        :style="{ backgroundImage: `url(/assets/img/${backgroundImage})` }"
      />
      <div
        class="bg-glitch-scan"
        :class="{ 'glitch-active': isGlitching }"
      />
      <!-- Glitch overlays removed — only intro glitch plays (via isGlitching) -->
    </div>

    <!-- Oomph accent - brief impact flash when dashboard loads -->
    <div
      v-if="showZoomIn"
      class="fixed inset-0 pointer-events-none z-[100] oomph-flash"
      aria-hidden="true"
    />
    <!-- Reveal flashes and glitch overlay - enthralling entrance -->
    <div
      v-if="showZoomIn"
      class="fixed inset-0 pointer-events-none z-[99] reveal-flash-glitch"
      aria-hidden="true"
    />

    <!-- Background overlay. The web5/server backdrop is a much lighter image
         than the others, so it gets a heavier scrim for text contrast. -->
    <div
      class="fixed inset-0 pointer-events-none bg-black transition-opacity duration-500"
      :class="isWeb5Bg ? 'opacity-[0.45]' : 'opacity-20'"
      style="z-index: -5;"
    />

    <!-- Sidebar - Desktop Only -->
    <DashboardSidebar :show-zoom-in="showZoomIn" @logout="handleLogout" />

    <!-- Main Content (Xbox: Right goes here from sidebar) -->
    <main
      id="main-content"
      data-controller-zone="main"
      class="flex-1 overflow-hidden relative pb-0 glass-piece z-10"
      :class="{ 'glass-throw-main': showZoomIn }"
      tabindex="-1"
      @pointerenter="activateMainScroll"
      @wheel.capture="activateMainScroll"
      @touchstart.passive="onContentTouchStart"
      @touchend.passive="onContentTouchEnd"
    >
      <div data-controller-main-entry class="absolute top-4 right-4 md:top-6 md:right-8 z-20">
        <!-- Controller zone entry point - no switcher -->
      </div>

      <!-- Connection Status Banners -->
      <ConnectionBanner />

      <div class="perspective-container-wrapper glass-piece" :class="{ 'glass-throw-content': showZoomIn && !isHomeRoute }">
        <div class="perspective-container">
          <DashboardRouterView
            :mobile-tab-padding-top="mobileTabPaddingTop"
            :needs-mobile-back-button-space="needsMobileBackButtonSpace"
          />
        </div>
      </div>

      <!-- Panel mode app session — renders alongside current page content -->
      <Transition name="panel-slide">
        <div v-if="appLauncher.panelAppId" class="app-panel-container">
          <AppSession
            :app-id-prop="appLauncher.panelAppId"
            :path-prop="appLauncher.panelPath ?? undefined"
            @close="appLauncher.closePanel()"
          />
        </div>
      </Transition>
    </main>

    <!-- Persistent Mobile Tabs + Bottom Tab Bar — outside <main> so position:fixed isn't broken by will-change:transform -->
    <DashboardMobileNav ref="mobileNavRef" :show-zoom-in="showZoomIn" />

    <!-- Health Notifications Toast -->
    <HealthNotifications />

    <!-- First-use companion intro overlay -->
    <CompanionIntroOverlay />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useAppStore } from '../stores/app'
import { useAppLauncherStore } from '../stores/appLauncher'
import AppSession from '@/views/AppSession.vue'
import { useLoginTransitionStore } from '../stores/loginTransition'
import { playDashboardLoadOomph } from '@/composables/useLoginSounds'

import DashboardSidebar from '@/views/dashboard/DashboardSidebar.vue'
import DashboardMobileNav from '@/views/dashboard/DashboardMobileNav.vue'
import DashboardRouterView from '@/views/dashboard/DashboardRouterView.vue'
import ConnectionBanner from '@/views/dashboard/ConnectionBanner.vue'
import HealthNotifications from '@/views/dashboard/HealthNotifications.vue'
import CompanionIntroOverlay from '@/components/CompanionIntroOverlay.vue'
import { isDetailRoute, ROUTE_BACKGROUNDS } from '@/views/dashboard/useRouteTransitions'
import { useIbdFinishWatcher } from '@/composables/useIbdFinishWatcher'
import '@/views/dashboard/dashboard-styles.css'

// Pops a "Finish setup" toast when Bitcoin IBD completes mid-Lightning-setup.
useIbdFinishWatcher()

const router = useRouter()
const route = useRoute()
const store = useAppStore()
const appLauncher = useAppLauncherStore()
const loginTransition = useLoginTransitionStore()

const showZoomIn = ref(false)
const pendingTimers: ReturnType<typeof setTimeout>[] = []

function scheduledTimeout(fn: () => void, ms: number) {
  const id = setTimeout(fn, ms)
  pendingTimers.push(id)
  return id
}

const showAltBackground = ref(false)
const isHomeRoute = computed(() => route.path === '/dashboard' || route.path === '/dashboard/')
const isGlitching = ref(false)

const backgroundImage = computed(() => {
  const mapped = ROUTE_BACKGROUNDS[route.path]
  if (mapped) return mapped
  // Detail/sub pages inherit their parent tab's background so they stay
  // visually "inside" the section instead of snapping to the home backdrop.
  if (route.path.startsWith('/dashboard/cloud/')) return 'bg-cloud.webp'
  if (route.path.startsWith('/dashboard/web5/')) return 'bg-web5.jpg'
  if (route.path.startsWith('/dashboard/server/')) return 'bg-web5.jpg'
  if (route.path.startsWith('/dashboard/settings/')) return 'bg-settings.webp'
  if (isDetailRoute(route.path)) return 'bg-intro.jpg'
  return 'bg-home.webp'
})

// bg-web5.jpg (web5 + server sections) is bright — the scrim overlay deepens
// while it's showing so light text keeps its contrast.
const isWeb5Bg = computed(() => backgroundImage.value === 'bg-web5.jpg')

const isDarkRoute = computed(() => {
  const p = route.path
  return p.includes('/dashboard/web5') ||
         p.includes('/dashboard/server') ||
         p.includes('/dashboard/settings') ||
         (p.includes('/dashboard/apps') && !isDetailRoute(p)) ||
         p.includes('/dashboard/marketplace') ||
         p.includes('/dashboard/discover') ||
         p.includes('/dashboard/cloud')
})

const showDarkOverlay = ref(isDarkRoute.value)
let overlayTimer: ReturnType<typeof setTimeout> | null = null

watch(isDarkRoute, (dark) => {
  if (overlayTimer) { clearTimeout(overlayTimer); overlayTimer = null }
  if (dark) {
    showDarkOverlay.value = true
  } else {
    overlayTimer = scheduledTimeout(() => { showDarkOverlay.value = false }, 450)
  }
})

const WEB5_DETAIL_ROUTES = ['/dashboard/server/federation', '/dashboard/monitoring', '/dashboard/fleet']
const needsMobileBackButtonSpace = computed(() =>
  isDetailRoute(route.path) || WEB5_DETAIL_ROUTES.includes(route.path)
)

const mobileNavRef = ref<InstanceType<typeof DashboardMobileNav> | null>(null)

const mobileTabPaddingTop = computed(() => {
  return mobileNavRef.value?.mobileTabPaddingTop ?? 0
})

function activateMainScroll() {
  const active = document.activeElement as HTMLElement | null
  if (active?.closest?.('[data-controller-zone="sidebar"]')) {
    active.blur()
    document.getElementById('main-content')?.focus({ preventScroll: true })
  }
}

// ── Swipe left/right to move between the mobile top tabs ──────────────────
// Apps screen:  My Apps ⇄ App Store ⇄ Services
// Web5 screen:  Web5 ⇄ Cloud ⇄ Network ⇄ Mesh
// Only active while the matching tab strip is actually showing (mobile).
type TabTarget = { key: string; to: { path: string; query: Record<string, string> } }
const APPS_TABS: TabTarget[] = [
  { key: 'myapps', to: { path: '/dashboard/apps', query: {} } },
  { key: 'store', to: { path: '/dashboard/discover', query: {} } },
  { key: 'services', to: { path: '/dashboard/apps', query: { tab: 'services' } } },
]
const NET_TABS: TabTarget[] = [
  { key: 'web5', to: { path: '/dashboard/web5', query: {} } },
  { key: 'cloud', to: { path: '/dashboard/cloud', query: {} } },
  { key: 'server', to: { path: '/dashboard/server', query: {} } },
  { key: 'mesh', to: { path: '/dashboard/mesh', query: {} } },
]

function activeAppsKey(): string {
  if (route.query.tab === 'services' || route.query.tab === 'websites') return 'services'
  if (route.path.includes('/marketplace') || route.path.includes('/discover')) return 'store'
  if (route.path === '/dashboard/apps' || route.path.startsWith('/dashboard/apps')) return 'myapps'
  return ''
}
function activeNetKey(): string {
  const p = route.path
  if (p.startsWith('/dashboard/web5')) return 'web5'
  if (p.startsWith('/dashboard/cloud')) return 'cloud'
  if (p.startsWith('/dashboard/server')) return 'server'
  if (p.startsWith('/dashboard/mesh')) return 'mesh'
  return ''
}

let touchStartX = 0
let touchStartY = 0
let touchStartTime = 0
let swipeSuppressed = false
function onContentTouchStart(e: TouchEvent) {
  const t = e.touches[0]
  if (!t) return
  // Don't begin a tab swipe when the gesture starts on an app icon (let the icon
  // handle tap/long-press) or on a horizontally-scrollable category strip (let
  // it scroll its own chips). Swiping anywhere else still changes tabs.
  swipeSuppressed = !!(
    e.target instanceof Element &&
    e.target.closest('.app-icon-item, .mobile-category-strip')
  )
  touchStartX = t.clientX
  touchStartY = t.clientY
  touchStartTime = e.timeStamp
}
function onContentTouchEnd(e: TouchEvent) {
  if (swipeSuppressed) { swipeSuppressed = false; return }
  const t = e.changedTouches[0]
  if (!t) return
  const dx = t.clientX - touchStartX
  const dy = t.clientY - touchStartY
  const dt = e.timeStamp - touchStartTime
  // Clear horizontal flick: far enough, mostly sideways, and quick.
  if (Math.abs(dx) < 60 || Math.abs(dx) < Math.abs(dy) * 1.8 || dt > 600) return

  const nav = mobileNavRef.value
  if (!nav) return
  let tabs: TabTarget[] | null = null
  let activeKey = ''
  if (nav.showAppsTabs) { tabs = APPS_TABS; activeKey = activeAppsKey() }
  else if (nav.showNetworkTabs) { tabs = NET_TABS; activeKey = activeNetKey() }
  if (!tabs) return

  const idx = tabs.findIndex(tb => tb.key === activeKey)
  if (idx < 0) return
  const next = idx + (dx < 0 ? 1 : -1) // swipe left → next tab, right → previous
  if (next < 0 || next >= tabs.length) return
  const target = tabs[next]
  if (!target) return
  router.push(target.to).catch(() => {})
}

watch(() => route.path, (newPath) => {
  const isAppDetails = isDetailRoute(newPath)
  const wasAppDetails = showAltBackground.value

  showAltBackground.value = isAppDetails

  if (isAppDetails && !wasAppDetails) {
    scheduledTimeout(() => {
      isGlitching.value = true
      scheduledTimeout(() => { isGlitching.value = false }, 375)
    }, 500)
  }
})

onMounted(() => {
  document.body.classList.add('dashboard-active')
  if (loginTransition.justCompletedOnboarding) {
    // Full glitchy reveal — only on the very first dashboard entry
    // right after onboarding (one-time event, persists in feel).
    // force=true: onboarding is already marked complete by now, so the
    // un-forced call would gate itself silent.
    playDashboardLoadOomph(true)
    showZoomIn.value = true
    loginTransition.setIntroCinematicPlaying(true)
    loginTransition.setPendingWelcomeTyping(true)
    loginTransition.setJustCompletedOnboarding(false)
    loginTransition.setJustLoggedIn(false)
    const triggerRevealGlitch = () => {
      isGlitching.value = true
      scheduledTimeout(() => { isGlitching.value = false }, 380)
    }
    scheduledTimeout(triggerRevealGlitch, 500)
    scheduledTimeout(triggerRevealGlitch, 1200)
    scheduledTimeout(triggerRevealGlitch, 2000)
    scheduledTimeout(() => {
      showZoomIn.value = false
      loginTransition.setIntroCinematicPlaying(false)
    }, 8000)
    scheduledTimeout(() => {
      loginTransition.setStartWelcomeTyping(true)
      loginTransition.setPendingWelcomeTyping(false)
    }, 4000)
  } else if (loginTransition.justLoggedIn) {
    // Regular re-login — no zoom, no glitch. Just land on the
    // dashboard and kick off the welcome typing quickly.
    playDashboardLoadOomph()
    loginTransition.setPendingWelcomeTyping(true)
    loginTransition.setJustLoggedIn(false)
    scheduledTimeout(() => {
      loginTransition.setStartWelcomeTyping(true)
      loginTransition.setPendingWelcomeTyping(false)
    }, 300)
  }

  window.addEventListener('keydown', handleKioskShortcuts)
})

onBeforeUnmount(() => {
  document.body.classList.remove('dashboard-active')
  // Timers are cleared below; make sure overlays waiting on the cinematic
  // aren't left blocked forever if we unmount mid-reveal.
  loginTransition.setIntroCinematicPlaying(false)
  window.removeEventListener('keydown', handleKioskShortcuts)
  for (const id of pendingTimers) clearTimeout(id)
  pendingTimers.length = 0
  if (overlayTimer) { clearTimeout(overlayTimer); overlayTimer = null }
})

function isKioskMode(): boolean {
  try {
    return localStorage.getItem('kiosk') === 'true' || new URLSearchParams(window.location.search).has('kiosk')
  } catch { return false }
}

function handleKioskShortcuts(e: KeyboardEvent) {
  if (!isKioskMode()) return
  if (e.ctrlKey && e.shiftKey) {
    if (e.key === 'R' || e.key === 'r') {
      e.preventDefault()
      router.push('/recovery')
    } else if (e.key === 'H' || e.key === 'h') {
      e.preventDefault()
      router.push('/dashboard')
    } else if (e.key === 'Q' || e.key === 'q') {
      e.preventDefault()
      if (confirm('Reboot the server?')) {
        fetch('/rpc/', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ method: 'system.reboot' }) }).catch(() => {})
      }
    }
  }
}

async function handleLogout() {
  try {
    await store.logout()
  } catch {
    /* proceed to login regardless */
  }
  router.push('/login').catch(() => {
    window.location.href = '/login'
  })
}
</script>

<!-- Styles extracted to dashboard/dashboard-styles.css -->
