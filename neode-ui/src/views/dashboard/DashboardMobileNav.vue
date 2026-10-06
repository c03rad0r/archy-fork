<template>
  <!-- Persistent Mobile Tabs for Apps/Marketplace -->
  <div
    v-if="showAppsTabs && !isAppSessionActive"
    class="md:hidden fixed top-0 left-0 right-0 z-40 px-4 pb-2 glass-piece mobile-top-tabs"
    :class="{ 'glass-throw-mobile-tabs': showZoomIn }"
    style="background: rgba(0, 0, 0, 0.25); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); transform: translateZ(0); padding-top: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 16px);"
  >
    <div class="mode-switcher mode-switcher-full">
      <RouterLink
        to="/dashboard/apps"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': (route.path === '/dashboard/apps' || route.path.startsWith('/dashboard/apps/')) && route.query.tab !== 'services' && route.query.tab !== 'websites' }"
        @click.prevent="router.push({ path: '/dashboard/apps', query: {} })"
      >My Apps</RouterLink>
      <RouterLink
        to="/dashboard/discover"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.path === '/dashboard/marketplace' || route.path.startsWith('/dashboard/marketplace/') || route.path === '/dashboard/discover' }"
      >App Store</RouterLink>
      <RouterLink
        to="/dashboard/apps?tab=services"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.query.tab === 'services' || route.query.tab === 'websites' }"
        @click.prevent="router.push({ path: '/dashboard/apps', query: { tab: 'services' } })"
      >Services</RouterLink>
    </div>
  </div>

  <!-- Persistent Mobile Tabs for Network/Cloud -->
  <div
    v-if="showNetworkTabs && !isAppSessionActive"
    class="md:hidden fixed left-0 right-0 z-40 px-4 pb-2 glass-piece mobile-top-tabs"
    :class="{ 'glass-throw-mobile-tabs-2': showZoomIn }"
    style="background: rgba(0, 0, 0, 0.25); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); transform: translateZ(0);"
    :style="{ top: showAppsTabs ? '80px' : '0', paddingTop: showAppsTabs ? '16px' : 'calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 16px)' }"
  >
    <div class="mode-switcher mode-switcher-full">
      <RouterLink
        to="/dashboard/web5"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.path === '/dashboard/web5' || route.path.startsWith('/dashboard/web5/') }"
      >Web5</RouterLink>
      <RouterLink
        to="/dashboard/cloud"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.path === '/dashboard/cloud' || route.path.startsWith('/dashboard/cloud/') }"
      >Cloud</RouterLink>
      <RouterLink
        to="/dashboard/server"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.path === '/dashboard/server' || route.path.startsWith('/dashboard/server/') }"
      >Network</RouterLink>
      <RouterLink
        to="/dashboard/mesh"
        class="mode-switcher-btn"
        :class="{ 'mode-switcher-btn-active': route.path === '/dashboard/mesh' || route.path.startsWith('/dashboard/mesh/') }"
      >Mesh</RouterLink>
    </div>
  </div>

  <!-- Mobile Bottom Tab Bar (hidden when app is open fullscreen) -->
  <nav
    v-if="!isAppSessionActive"
    ref="mobileTabBar"
    data-mobile-tab-bar
    :aria-label="t('dashboard.mobileNav')"
    class="md:hidden fixed bottom-0 left-0 right-0 border-t border-glass-border shadow-glass z-50 glass-piece"
    :class="{ 'glass-throw-tabbar': showZoomIn }"
    style="background: rgba(0, 0, 0, 0.25); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); padding-bottom: var(--safe-area-bottom, env(safe-area-inset-bottom, 0px));"
  >
    <div class="flex justify-around items-center px-2 py-3 relative">
      <RouterLink
        v-for="item in mobileNavItems"
        :key="item.path"
        :to="item.path"
        aria-current-value="page"
        @click="appLauncher.closePanel()"
        class="flex items-center justify-center w-14 h-14 rounded-xl text-white/70 transition-all duration-300 relative z-10"
        :class="{
          'nav-tab-active': item.isCombined
            ? (item.path === '/dashboard/apps'
                ? (route.path.includes('/apps') || route.path.includes('/marketplace') || route.path.includes('/discover') || route.path.includes('/app-session'))
                : item.path === '/dashboard/web5'
                  ? (route.path.includes('/web5') || route.path.includes('/federation') || route.path.includes('/mesh'))
                  : (route.path.includes('/cloud') || route.path.includes('/server')))
            : undefined
        }"
        :exact-active-class="item.isCombined ? undefined : 'nav-tab-active'"
      >
        <svg v-if="item.icon === 'web5'" class="w-6 h-6 transition-all duration-300" aria-hidden="true" fill="currentColor" viewBox="0 0 1631 1624">
          <path fill-rule="evenodd" clip-rule="evenodd" d="M914.932 359.228H916.229V715.252H1630.47V1088.98H1451.41V1267.98H1274.33V1445H1093.31V1624H715.534V1264.77H714.237V908.748H0V535.02H179.051V356.025H356.135V178.996H537.154V0H914.932V359.228ZM916.229 1425.33H1073.64V1248.31H1254.66V1071.28H1431.74V913.918H916.229V1425.33ZM556.83 375.695H375.811V552.723H198.727V710.082H714.237V198.666H556.83V375.695Z" />
        </svg>
        <svg v-else class="w-6 h-6 transition-all duration-300" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            v-for="(path, index) in getIconPath(item.icon)"
            :key="index"
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            :d="path"
          />
        </svg>
      </RouterLink>
      <!-- Chat launcher -->
      <button
        @click="router.push('/dashboard/chat')"
        class="chat-launcher-btn-mobile flex items-center justify-center w-14 h-14 rounded-xl transition-all duration-300 relative z-10"
      >
        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path v-for="(path, index) in getIconPath('chat')" :key="index" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="path" />
        </svg>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { RouterLink, useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useUIModeStore } from '@/stores/uiMode'

interface NavItem {
  path: string
  label: string
  icon: string
  isCombined?: boolean
}

defineProps<{
  showZoomIn: boolean
}>()

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const appLauncher = useAppLauncherStore()
const uiMode = useUIModeStore()

const mobileTabBar = ref<HTMLElement | null>(null)
const MOBILE_LAYOUT_MAX_WIDTH = 920
const viewportWidth = ref(typeof window === 'undefined' ? 1024 : window.innerWidth)

// App sessions own their mobile controls, so the nav hides while one is open.
// Mobile launches now use the store-driven panel (no route change) to keep the
// background tab intact, so treat an active panel the same as a routed session.
const isAppSessionActive = computed(() => route.name === 'app-session' || !!appLauncher.panelAppId)

// Show persistent tabs for Apps/Marketplace on mobile
const showAppsTabs = computed(() => {
  if (typeof window === 'undefined') return false
  if (viewportWidth.value > MOBILE_LAYOUT_MAX_WIDTH) return false
  return route.path.includes('/apps') || route.path.includes('/marketplace') || route.path.includes('/discover')
})

// Show persistent tabs for Network/Cloud on mobile
const showNetworkTabs = computed(() => {
  if (typeof window === 'undefined') return false
  if (viewportWidth.value > MOBILE_LAYOUT_MAX_WIDTH) return false
  if (route.name === 'cloud-folder') return false
  return route.path.includes('/server') || route.path.includes('/cloud') || route.path.includes('/web5') || route.path.includes('/mesh')
})

// Top padding for content div to clear fixed mobile tab overlays.
// Includes safe area inset for Android (read from CSS custom property set by WebView).
const safeAreaTop = ref(0)

function readSafeAreaTop() {
  if (typeof window === 'undefined') return
  const val = getComputedStyle(document.documentElement).getPropertyValue('--safe-area-top').trim()
  if (val) safeAreaTop.value = parseInt(val, 10) || 0
}

const mobileTabPaddingTop = computed(() => {
  if (typeof window === 'undefined' || viewportWidth.value > MOBILE_LAYOUT_MAX_WIDTH) return 0
  const sat = safeAreaTop.value
  if (showAppsTabs.value && showNetworkTabs.value) return 160 + sat
  if (showAppsTabs.value || showNetworkTabs.value) return 80 + sat
  return 0
})

defineExpose({
  showAppsTabs,
  showNetworkTabs,
  mobileTabPaddingTop,
})

function updateTabBarHeight() {
  if (typeof window === 'undefined') return
  const el = mobileTabBar.value
  // offsetHeight is 0 when the bar is hidden (desktop `md:hidden`) or not yet
  // laid out. Writing `--mobile-tab-bar-height: 0px` would DEFEAT the `, 88px`
  // fallback baked into the `.mobile-scroll-pad` clearance calc (an explicit
  // 0px is still "set"), so the fixed tab bar ends up covering the last row of
  // content — the Cloud/files "bottom elements cut off" bug. Only write a real
  // measured height; otherwise remove the var so the fallback applies.
  if (el && el.offsetHeight > 0) {
    document.documentElement.style.setProperty('--mobile-tab-bar-height', `${el.offsetHeight}px`)
  } else {
    document.documentElement.style.removeProperty('--mobile-tab-bar-height')
  }
}

// Store-driven app sessions do not change the route. Their v-if removes and
// recreates the navigation, so route/resize listeners alone leave its height
// unset after closing an app and let the navigation cover the audio controls.
watch(mobileTabBar, (element, _previous, onCleanup) => {
  updateTabBarHeight()
  if (!element || typeof ResizeObserver === 'undefined') return
  const observer = new ResizeObserver(updateTabBarHeight)
  observer.observe(element)
  onCleanup(() => observer.disconnect())
}, { flush: 'post' })

function onResize() {
  viewportWidth.value = window.innerWidth
  updateTabBarHeight()
}

function onInsetsInjected() {
  readSafeAreaTop()
  updateTabBarHeight()
}

onMounted(() => {
  updateTabBarHeight()
  // Re-measure after the first paint: on mount the bar may not have its final
  // laid-out height yet (fonts/safe-area padding still settling), which would
  // leave the clearance var short.
  requestAnimationFrame(updateTabBarHeight)
  readSafeAreaTop()
  window.addEventListener('resize', onResize)
  // The Android WebView injects --safe-area-top asynchronously and fires this
  // event when it lands. An authenticated session mounts the dashboard BEFORE
  // the injection (fresh installs mount after login, long after it), so a
  // one-shot read here bakes in 0 and content slides under the growing fixed
  // tab bar — the update-install-only overlap bug.
  window.addEventListener('archy-insets', onInsetsInjected)
  // Fallback retry ladder for APKs that predate the event.
  for (const delay of [500, 1500, 3000, 6000]) {
    setTimeout(onInsetsInjected, delay)
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize)
  window.removeEventListener('archy-insets', onInsetsInjected)
})

// Re-measure on route changes
watch(() => route.path, () => {
  nextTick(() => {
    updateTabBarHeight()
  })
})

const gamerMobileNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'Apps', icon: 'apps', isCombined: true },
  { path: '/dashboard/web5', label: 'Web5', icon: 'web5', isCombined: true },
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const easyMobileNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'Apps', icon: 'apps' },
  { path: '/dashboard/cloud', label: 'Cloud', icon: 'cloud' },
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const chatMobileNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'Apps', icon: 'apps' },
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const mobileNavItems = computed(() => {
  if (uiMode.isEasy) return easyMobileNav
  if (uiMode.isChat) return chatMobileNav
  return gamerMobileNav
})

function getIconPath(iconName: string): string[] {
  const icons: Record<string, string[]> = {
    home: ['M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6'],
    apps: ['M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z'],
    cloud: ['M3 15a4 4 0 004 4h9a5 5 0 10-.1-9.999 5.002 5.002 0 10-9.78 2.096A4.001 4.001 0 003 15z'],
    server: ['M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01'],
    web5: ['M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9'],
    mesh: ['M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01M5.636 13.636a9 9 0 0112.728 0M1.5 10.5a14 14 0 0121 0'],
    fleet: ['M9 17V7m0 10a2 2 0 01-2 2H5a2 2 0 01-2-2V7a2 2 0 012-2h2a2 2 0 012 2m0 10a2 2 0 002 2h2a2 2 0 002-2M9 7a2 2 0 012-2h2a2 2 0 012 2m0 10V7m0 10a2 2 0 002 2h2a2 2 0 002-2V7a2 2 0 00-2-2h-2a2 2 0 00-2 2'],
    chat: ['M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z'],
    settings: [
      'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z',
      'M15 12a3 3 0 11-6 0 3 3 0 016 0z',
    ],
  }
  return icons[iconName] || []
}
</script>
