import { createRouter, createWebHistory } from 'vue-router'
import { nextTick } from 'vue'
import { useAppStore } from '../stores/app'
import { stopAllAudio } from '../composables/useLoginSounds'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      component: () => import('../views/OnboardingWrapper.vue'),
      meta: { public: true },
      children: [
        {
          path: '',
          component: () => import('../views/RootRedirect.vue'),
        },
        {
          path: 'login',
          name: 'login',
          component: () => import('../views/Login.vue'),
        },
        {
          path: 'onboarding/intro',
          name: 'onboarding-intro',
          component: () => import('../views/OnboardingIntro.vue'),
        },
        {
          path: 'onboarding/options',
          name: 'onboarding-options',
          component: () => import('../views/OnboardingOptions.vue'),
        },
        {
          path: 'onboarding/path',
          name: 'onboarding-path',
          component: () => import('../views/OnboardingPath.vue'),
        },
        {
          path: 'onboarding/seed',
          name: 'onboarding-seed',
          component: () => import('../views/OnboardingSeedGenerate.vue'),
        },
        {
          path: 'onboarding/seed-verify',
          name: 'onboarding-seed-verify',
          component: () => import('../views/OnboardingSeedVerify.vue'),
        },
        {
          path: 'onboarding/seed-restore',
          name: 'onboarding-seed-restore',
          component: () => import('../views/OnboardingSeedRestore.vue'),
        },
        {
          path: 'onboarding/did',
          name: 'onboarding-did',
          component: () => import('../views/OnboardingDid.vue'),
        },
        {
          path: 'onboarding/identity',
          name: 'onboarding-identity',
          component: () => import('../views/OnboardingIdentity.vue'),
        },
        {
          path: 'onboarding/backup',
          name: 'onboarding-backup',
          component: () => import('../views/OnboardingBackup.vue'),
        },
        {
          path: 'onboarding/verify',
          name: 'onboarding-verify',
          component: () => import('../views/OnboardingVerify.vue'),
        },
        {
          path: 'onboarding/done',
          name: 'onboarding-done',
          component: () => import('../views/OnboardingDone.vue'),
        },
      ],
    },
    {
      path: '/recovery',
      name: 'recovery',
      component: () => import('../views/KioskRecovery.vue'),
      meta: { public: true },
    },
    {
      // The kiosk display no longer has its own launcher screen. It runs the
      // normal app (onboarding → login → dashboard) like any other client.
      // This route only persists kiosk mode + safe-area insets, then redirects
      // to the root app. The launcher still points Chromium here (not directly
      // at `/`) so the 'kiosk' flag gets set — App.vue keys kiosk-only styling
      // (kiosk-mode/kiosk-safe-area classes, dark AIUI pin) off it. Public so
      // the auth guard doesn't bounce us before beforeEnter.
      path: '/kiosk',
      name: 'kiosk',
      meta: { public: true },
      component: () => import('../views/RootRedirect.vue'),
      beforeEnter: (to) => {
        localStorage.setItem('kiosk', 'true')
        const safeArea = to.query.safe_area
        const safeAreaPx = Array.isArray(safeArea) ? safeArea[0] : safeArea
        if (safeAreaPx && /^\d{1,3}$/.test(safeAreaPx)) {
          localStorage.setItem('archipelago_kiosk_safe_area_px', safeAreaPx)
        }
        const safeAreaX = to.query.safe_area_x
        const safeAreaXPx = Array.isArray(safeAreaX) ? safeAreaX[0] : safeAreaX
        if (safeAreaXPx && /^\d{1,3}$/.test(safeAreaXPx)) {
          localStorage.setItem('archipelago_kiosk_safe_area_x_px', safeAreaXPx)
        }
        const safeAreaY = to.query.safe_area_y
        const safeAreaYPx = Array.isArray(safeAreaY) ? safeAreaY[0] : safeAreaY
        if (safeAreaYPx && /^\d{1,3}$/.test(safeAreaYPx)) {
          localStorage.setItem('archipelago_kiosk_safe_area_y_px', safeAreaYPx)
        }
        // Grid screen removed — hand off to the normal app flow.
        return { path: '/' }
      },
    },
    {
      path: '/dashboard',
      component: () => import('../views/Dashboard.vue'),
      children: [
        {
          path: '',
          name: 'home',
          component: () => import('../views/Home.vue'),
        },
        {
          path: 'apps',
          name: 'apps',
          component: () => import('../views/Apps.vue'),
        },
        {
          path: 'apps/:id',
          name: 'app-details',
          component: () => import('../views/AppDetails.vue'),
        },
        {
          path: 'apps/lnd/channels',
          name: 'lightning-channels',
          component: () => import('../views/apps/LightningChannels.vue'),
        },
        {
          path: 'discover',
          name: 'discover',
          component: () => import('../views/Discover.vue'),
        },
        {
          path: 'marketplace',
          name: 'marketplace',
          component: () => import('../views/Marketplace.vue'),
        },
        {
          path: 'marketplace/:id',
          name: 'marketplace-app-detail',
          component: () => import('../views/MarketplaceAppDetails.vue'),
        },
        {
          path: 'cloud',
          name: 'cloud',
          component: () => import('../views/Cloud.vue'),
        },
        {
          path: 'cloud/peers/:peerId?',
          name: 'peer-files',
          component: () => import('../views/PeerFiles.vue'),
          props: true,
        },
        {
          path: 'cloud/:folderId',
          name: 'cloud-folder',
          component: () => import('../views/CloudFolder.vue'),
        },
        {
          path: 'server',
          name: 'server',
          component: () => import('../views/Server.vue'),
        },
        {
          path: 'server/openwrt',
          name: 'openwrt-gateway',
          component: () => import('../views/server/OpenWrtGateway.vue'),
        },
        {
          path: 'monitoring',
          name: 'monitoring',
          component: () => import('../views/Monitoring.vue'),
        },
        {
          path: 'fleet',
          name: 'fleet',
          component: () => import('../views/Fleet.vue'),
        },
        {
          path: 'server/federation',
          name: 'federation',
          component: () => import('../views/Federation.vue'),
        },
        {
          path: 'mesh',
          name: 'mesh',
          component: () => import('../views/Mesh.vue'),
        },
        {
          path: 'web5',
          name: 'web5',
          component: () => import('../views/web5/Web5.vue'),
        },
        {
          path: 'web5/credentials',
          name: 'credentials',
          component: () => import('../views/Credentials.vue'),
        },
        {
          path: 'web5/networking-profits',
          name: 'networking-profits-settings',
          component: () => import('../views/web5/Web5NetworkingProfitsSettings.vue'),
        },
        {
          path: 'settings',
          name: 'settings',
          component: () => import('../views/Settings.vue'),
        },
        {
          path: 'settings/update',
          name: 'system-update',
          component: () => import('../views/SystemUpdate.vue'),
        },
        {
          path: 'settings/registries',
          name: 'app-registries',
          component: () => import('../views/AppRegistries.vue'),
        },
        {
          path: 'goals/:goalId',
          name: 'goal-detail',
          component: () => import('../views/GoalDetail.vue'),
        },
        {
          path: 'chat',
          name: 'chat',
          component: () => import('../views/Chat.vue'),
        },
        {
          path: 'app-session/:appId',
          name: 'app-session',
          component: () => import('../views/AppSession.vue'),
        },
        // Containers removed: My Apps serves the same purpose. Redirect old links.
        {
          path: 'containers',
          redirect: () => ({ path: '/dashboard/apps' }),
        },
        {
          path: 'containers/:id',
          redirect: (to) => ({ path: `/dashboard/apps/${to.params.id}` }),
        },
      ],
    },
    {
      path: '/:pathMatch(.*)*',
      name: 'not-found',
      component: () => import('../views/NotFound.vue'),
    },
  ],
})

// Session check with timeout - avoids endless spinner on mobile/slow networks
const SESSION_CHECK_TIMEOUT_MS = 8000

async function checkSessionWithTimeout(store: ReturnType<typeof useAppStore>): Promise<boolean> {
  try {
    return await Promise.race([
      store.checkSession(),
      new Promise<boolean>((resolve) =>
        setTimeout(() => resolve(false), SESSION_CHECK_TIMEOUT_MS)
      ),
    ])
  } catch {
    return false
  }
}

/**
 * Navigation Guard
 * Handles authentication and onboarding flow routing
 */
export function isLocalRedirect(path: unknown): path is string {
  if (typeof path !== 'string') return false
  try {
    if (path.startsWith('//') || path.includes('://')) return false
    const url = new URL(path, window.location.origin)
    return url.origin === window.location.origin
  } catch {
    return false
  }
}

router.beforeEach(async (to, _from, next) => {
  const store = useAppStore()
  const isPublic = to.meta.public

  // Allow all public routes (login, onboarding) without auth check
  if (isPublic) {
    // If authenticated and visiting /login: show login immediately, validate in background.
    // This prevents endless spinner on mobile when checkSession hangs (slow/unreachable network).
    if (to.path === '/login' && store.isAuthenticated) {
      // Redirect back to intended page (from ?redirect= query) or default to home
      const rawRedirect = to.query.redirect
      const redirectTo = isLocalRedirect(rawRedirect) ? rawRedirect : '/dashboard'
      if (store.needsSessionValidation()) {
        next()
        checkSessionWithTimeout(store).then((valid) => {
          if (valid) {
            router.replace(redirectTo).catch(() => {})
          }
        })
        return
      }
      next(redirectTo)
      return
    }
    next()
    return
  }

  // Protected routes: validate session if stale auth from localStorage
  if (store.needsSessionValidation()) {
    // localStorage says we're authed — proceed immediately, revalidate in background.
    // No timeout wrapper here: a slow server shouldn't bounce the user to login.
    next()
    store.checkSession().then((valid) => {
      if (!valid) {
        router.replace({ path: '/login', query: { redirect: to.fullPath } }).catch(() => {})
      }
    })
    return
  }

  // Not authenticated at all (with timeout to avoid endless spinner on mobile)
  if (!store.isAuthenticated) {
    const hasSession = await checkSessionWithTimeout(store)
    if (hasSession) {
      next()
      return
    }
    // Check if this is a fresh install that needs onboarding.
    // Prefer checkOnboardingStatus() (tri-state) so we can distinguish
    // "confirmed fresh install" from "backend unreachable". On the
    // latter, send the user to RootRedirect (/) rather than the intro
    // wizard — RootRedirect polls the backend and will route to
    // /login once it answers, instead of forcing a re-onboarding.
    try {
      const { checkOnboardingStatus, getSavedOnboardingStep } = await import('@/composables/useOnboarding')
      const setupDone = await checkOnboardingStatus()
      if (setupDone === false) {
        const step = getSavedOnboardingStep()
        next(`/onboarding/${step}`)
        return
      }
      if (setupDone === null) {
        // Backend unreachable after retries — bounce through RootRedirect
        // so it can keep polling and land the user on /login once the
        // backend answers, instead of flashing the onboarding wizard.
        const cached = localStorage.getItem('neode_onboarding_complete') === '1'
        if (!cached) {
          next('/')
          return
        }
        // Cached as onboarded — continue to the /login path below.
      }
    } catch {
      // Unexpected error — do NOT default to onboarding. Hand off to
      // RootRedirect which has retry + polling + boot-screen handling.
      next('/')
      return
    }
    next({ path: '/login', query: { redirect: to.fullPath } })
    return
  }

  // Validated and authenticated - ensure WebSocket is connected
  if (!store.isConnected && !store.isReconnecting) {
    store.connectWebSocket().catch((err) => {
      if (import.meta.env.DEV) console.warn('[Router] WebSocket connection failed:', err)
    })
  }

  next()
})

// Persist onboarding step so page refresh resumes where user left off
router.afterEach((to) => {
  const match = to.path.match(/^\/onboarding\/(.+)/)
  if (match && match[1] !== 'intro') {
    localStorage.setItem('neode_onboarding_step', match[1]!)
  }
})

// Stop all login/splash audio when entering the dashboard
router.afterEach((to, from) => {
  if (to.path.startsWith('/dashboard') && !from.path.startsWith('/dashboard')) {
    stopAllAudio()
  }
})

// Focus Home nav item for gamepad when landing on dashboard home (e.g. after login)
router.afterEach((to) => {
  if (to.path === '/dashboard' || to.path === '/dashboard/') {
    nextTick(() => {
      setTimeout(() => {
        const homeLink = document.querySelector<HTMLAnchorElement>(
          '[data-controller-zone="sidebar"] a[href="/dashboard"], [data-controller-zone="sidebar"] a[href="/dashboard/"]'
        )
        if (homeLink) homeLink.focus()
      }, 150)
    })
  }
})

// A route whose lazy-loaded component chunk 404s (stale index after a deploy)
// rejects through router.onError rather than window.unhandledrejection. Reload
// once so the browser fetches the fresh index + chunk map; the sessionStorage
// guard (10s) prevents a reload loop if the chunk is genuinely broken.
router.onError((err) => {
  const msg = (err as { message?: string })?.message ?? String(err ?? '')
  if (!/Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed|ChunkLoadError|dynamically imported module/i.test(msg)) {
    return
  }
  try {
    const KEY = 'archy-chunk-reload-at'
    if (Date.now() - Number(sessionStorage.getItem(KEY) || '0') < 10_000) return
    sessionStorage.setItem(KEY, String(Date.now()))
  } catch { /* sessionStorage unavailable — reload anyway */ }
  location.reload()
})

export default router
