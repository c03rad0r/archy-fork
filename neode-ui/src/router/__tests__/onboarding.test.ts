import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createRouter, createWebHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import { defineComponent } from 'vue'

// Mock the app store module
const mockStore = {
  isAuthenticated: false,
  isConnected: false,
  isReconnecting: false,
  needsSessionValidation: vi.fn().mockReturnValue(false),
  checkSession: vi.fn().mockResolvedValue(false),
  connectWebSocket: vi.fn().mockResolvedValue(undefined),
}

vi.mock('@/stores/app', () => ({
  useAppStore: () => mockStore,
}))

const Stub = defineComponent({ template: '<div />' })

function createTestRouter() {
  return createRouter({
    history: createWebHistory(),
    routes: [
      {
        path: '/',
        component: Stub,
        meta: { public: true },
        children: [
          { path: '', component: Stub },
          { path: 'login', name: 'login', component: Stub },
          { path: 'onboarding/intro', name: 'onboarding-intro', component: Stub },
          { path: 'onboarding/options', name: 'onboarding-options', component: Stub },
          { path: 'onboarding/path', name: 'onboarding-path', component: Stub },
          { path: 'onboarding/did', name: 'onboarding-did', component: Stub },
          { path: 'onboarding/identity', name: 'onboarding-identity', component: Stub },
          { path: 'onboarding/backup', name: 'onboarding-backup', component: Stub },
          { path: 'onboarding/verify', name: 'onboarding-verify', component: Stub },
          { path: 'onboarding/done', name: 'onboarding-done', component: Stub },
        ],
      },
      {
        path: '/dashboard',
        component: Stub,
        children: [
          { path: '', name: 'home', component: Stub },
          { path: 'apps', name: 'apps', component: Stub },
          { path: 'settings', name: 'settings', component: Stub },
        ],
      },
    ],
  })
}

describe('Onboarding Routing Flow', () => {
  let router: ReturnType<typeof createTestRouter>

  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    mockStore.isAuthenticated = false
    mockStore.isConnected = false
    mockStore.isReconnecting = false
    mockStore.needsSessionValidation.mockReturnValue(false)
    mockStore.checkSession.mockResolvedValue(false)
    router = createTestRouter()

    // Add the same beforeEach guard as the real router
    router.beforeEach(async (to) => {
      const isPublic = to.meta.public

      if (isPublic) {
        if (to.path === '/login' && mockStore.isAuthenticated) {
          if (mockStore.needsSessionValidation()) {
            return true
          }
          return { name: 'home' }
        }
        return true
      }

      if (mockStore.needsSessionValidation()) {
        mockStore.checkSession()
        return true
      }

      if (!mockStore.isAuthenticated) {
        const hasSession = await mockStore.checkSession()
        if (hasSession) return true
        return '/login'
      }

      if (!mockStore.isConnected && !mockStore.isReconnecting) {
        mockStore.connectWebSocket()
      }

      return true
    })
  })

  describe('unauthenticated users', () => {
    it('redirects to /login when accessing /dashboard, not to /onboarding', async () => {
      mockStore.checkSession.mockResolvedValue(false)
      await router.push('/dashboard')
      expect(router.currentRoute.value.path).toBe('/login')
      expect(router.currentRoute.value.path).not.toContain('/onboarding')
    })

    it('redirects to /login when accessing /dashboard/apps', async () => {
      mockStore.checkSession.mockResolvedValue(false)
      await router.push('/dashboard/apps')
      expect(router.currentRoute.value.path).toBe('/login')
    })

    it('redirects to /login when accessing /dashboard/settings', async () => {
      mockStore.checkSession.mockResolvedValue(false)
      await router.push('/dashboard/settings')
      expect(router.currentRoute.value.path).toBe('/login')
    })
  })

  describe('onboarding routes are accessible when authenticated', () => {
    beforeEach(() => {
      mockStore.isAuthenticated = true
    })

    it('allows access to /onboarding/intro', async () => {
      await router.push('/onboarding/intro')
      expect(router.currentRoute.value.path).toBe('/onboarding/intro')
      expect(router.currentRoute.value.name).toBe('onboarding-intro')
    })

    it('allows access to /onboarding/options', async () => {
      await router.push('/onboarding/options')
      expect(router.currentRoute.value.path).toBe('/onboarding/options')
      expect(router.currentRoute.value.name).toBe('onboarding-options')
    })

    it('allows access to /onboarding/path', async () => {
      await router.push('/onboarding/path')
      expect(router.currentRoute.value.path).toBe('/onboarding/path')
      expect(router.currentRoute.value.name).toBe('onboarding-path')
    })
  })

  describe('OnboardingDid route', () => {
    it('is accessible when authenticated', async () => {
      mockStore.isAuthenticated = true
      await router.push('/onboarding/did')
      expect(router.currentRoute.value.path).toBe('/onboarding/did')
      expect(router.currentRoute.value.name).toBe('onboarding-did')
    })

    it('is accessible when unauthenticated (public route)', async () => {
      mockStore.isAuthenticated = false
      await router.push('/onboarding/did')
      expect(router.currentRoute.value.path).toBe('/onboarding/did')
      expect(router.currentRoute.value.name).toBe('onboarding-did')
    })
  })

  describe('post-onboarding navigation to dashboard', () => {
    it('allows authenticated users to navigate from onboarding to /dashboard', async () => {
      mockStore.isAuthenticated = true

      // Start at onboarding done page
      await router.push('/onboarding/done')
      expect(router.currentRoute.value.path).toBe('/onboarding/done')

      // Navigate to dashboard (simulating post-onboarding completion)
      await router.push('/dashboard')
      expect(router.currentRoute.value.path).toBe('/dashboard')
      expect(router.currentRoute.value.name).toBe('home')
    })

    it('allows authenticated users to navigate from onboarding/did to /dashboard', async () => {
      mockStore.isAuthenticated = true

      await router.push('/onboarding/did')
      expect(router.currentRoute.value.path).toBe('/onboarding/did')

      await router.push('/dashboard')
      expect(router.currentRoute.value.path).toBe('/dashboard')
      expect(router.currentRoute.value.name).toBe('home')
    })

    it('allows navigation through the full onboarding sequence', async () => {
      mockStore.isAuthenticated = true

      await router.push('/onboarding/intro')
      expect(router.currentRoute.value.name).toBe('onboarding-intro')

      await router.push('/onboarding/path')
      expect(router.currentRoute.value.name).toBe('onboarding-path')

      await router.push('/onboarding/did')
      expect(router.currentRoute.value.name).toBe('onboarding-did')

      await router.push('/onboarding/identity')
      expect(router.currentRoute.value.name).toBe('onboarding-identity')

      await router.push('/onboarding/backup')
      expect(router.currentRoute.value.name).toBe('onboarding-backup')

      await router.push('/onboarding/verify')
      expect(router.currentRoute.value.name).toBe('onboarding-verify')

      await router.push('/onboarding/done')
      expect(router.currentRoute.value.name).toBe('onboarding-done')
    })
  })

  describe('dashboard guard blocks unauthenticated access after onboarding', () => {
    it('prevents unauthenticated navigation from onboarding/done to /dashboard', async () => {
      // Start at onboarding done (public route, accessible without auth)
      await router.push('/onboarding/done')
      expect(router.currentRoute.value.path).toBe('/onboarding/done')

      // Try to navigate to dashboard without auth
      mockStore.checkSession.mockResolvedValue(false)
      await router.push('/dashboard')
      expect(router.currentRoute.value.path).toBe('/login')
    })

    it('prevents unauthenticated access to /dashboard even with stale session', async () => {
      mockStore.isAuthenticated = false
      mockStore.needsSessionValidation.mockReturnValue(false)
      mockStore.checkSession.mockResolvedValue(false)

      await router.push('/dashboard/apps')
      expect(router.currentRoute.value.path).toBe('/login')
    })

    it('allows dashboard access when session check succeeds', async () => {
      mockStore.isAuthenticated = false
      mockStore.checkSession.mockResolvedValue(true)

      await router.push('/dashboard')
      expect(router.currentRoute.value.path).toBe('/dashboard')
    })
  })
})
