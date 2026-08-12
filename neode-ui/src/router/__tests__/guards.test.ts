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

describe('Router Guards', () => {
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

  it('allows unauthenticated access to public routes', async () => {
    await router.push('/login')
    expect(router.currentRoute.value.path).toBe('/login')
  })

  it('redirects unauthenticated users from protected routes to login', async () => {
    mockStore.checkSession.mockResolvedValue(false)
    await router.push('/dashboard')
    expect(router.currentRoute.value.path).toBe('/login')
  })

  it('allows authenticated users to access protected routes', async () => {
    mockStore.isAuthenticated = true
    await router.push('/dashboard/apps')
    expect(router.currentRoute.value.path).toBe('/dashboard/apps')
  })

  it('redirects authenticated users from /login to home', async () => {
    mockStore.isAuthenticated = true
    mockStore.needsSessionValidation.mockReturnValue(false)
    await router.push('/login')
    expect(router.currentRoute.value.name).toBe('home')
  })

  it('validates stale session and allows access if valid', async () => {
    mockStore.isAuthenticated = true
    mockStore.needsSessionValidation.mockReturnValue(true)
    await router.push('/dashboard/settings')
    expect(router.currentRoute.value.path).toBe('/dashboard/settings')
    expect(mockStore.checkSession).toHaveBeenCalled()
  })

  it('allows access to onboarding routes without auth', async () => {
    await router.push('/onboarding/intro')
    expect(router.currentRoute.value.path).toBe('/onboarding/intro')
  })

  it('triggers WebSocket connection for authenticated users without connection', async () => {
    mockStore.isAuthenticated = true
    mockStore.isConnected = false
    mockStore.isReconnecting = false
    await router.push('/dashboard')
    expect(mockStore.connectWebSocket).toHaveBeenCalled()
  })

  it('does not reconnect WebSocket if already connected', async () => {
    mockStore.isAuthenticated = true
    mockStore.isConnected = true
    await router.push('/dashboard')
    expect(mockStore.connectWebSocket).not.toHaveBeenCalled()
  })

  it('does not reconnect WebSocket if already reconnecting', async () => {
    mockStore.isAuthenticated = true
    mockStore.isConnected = false
    mockStore.isReconnecting = true
    await router.push('/dashboard')
    expect(mockStore.connectWebSocket).not.toHaveBeenCalled()
  })
})
