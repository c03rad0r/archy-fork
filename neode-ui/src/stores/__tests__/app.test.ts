import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Mock the rpc-client module
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    login: vi.fn(),
    logout: vi.fn(),
    call: vi.fn(),
    installPackage: vi.fn(),
    uninstallPackage: vi.fn(),
    startPackage: vi.fn(),
    stopPackage: vi.fn(),
    restartPackage: vi.fn(),
    updateServer: vi.fn(),
    restartServer: vi.fn(),
    shutdownServer: vi.fn(),
    getMetrics: vi.fn(),
    getMarketplace: vi.fn(),
  },
}))

// Mock the websocket module
vi.mock('@/api/websocket', () => ({
  wsClient: {
    connect: vi.fn().mockResolvedValue(undefined),
    disconnect: vi.fn(),
    subscribe: vi.fn(),
    isConnected: vi.fn().mockReturnValue(false),
    onConnectionStateChange: vi.fn(),
  },
  applyDataPatch: vi.fn(),
}))

import { useAppStore } from '../app'
import { rpcClient } from '@/api/rpc-client'
import { wsClient } from '@/api/websocket'

const mockedRpc = vi.mocked(rpcClient)
const mockedWs = vi.mocked(wsClient)

describe('useAppStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    localStorage.clear()
    mockedWs.isConnected.mockReturnValue(false)
  })

  it('starts with default unauthenticated state', () => {
    const store = useAppStore()
    expect(store.isAuthenticated).toBe(false)
    expect(store.isConnected).toBe(false)
    expect(store.isLoading).toBe(false)
    expect(store.error).toBeNull()
    expect(store.data).toBeNull()
  })

  it('login succeeds and sets authenticated state', async () => {
    mockedRpc.login.mockResolvedValue(null)
    const store = useAppStore()

    await store.login('password123')

    expect(store.isAuthenticated).toBe(true)
    expect(localStorage.getItem('neode-auth')).toBe('true')
    expect(store.data).not.toBeNull()
    expect(store.isLoading).toBe(false)
  })

  it('login handles TOTP requirement', async () => {
    mockedRpc.login.mockResolvedValue({ requires_totp: true })
    const store = useAppStore()

    const result = await store.login('password123')

    expect(result).toEqual({ requires_totp: true })
    expect(store.isAuthenticated).toBe(false)
  })

  it('login sets error on failure', async () => {
    mockedRpc.login.mockRejectedValue(new Error('Invalid password'))
    const store = useAppStore()

    await expect(store.login('wrong')).rejects.toThrow('Invalid password')

    expect(store.error).toBe('Invalid password')
    expect(store.isAuthenticated).toBe(false)
    expect(store.isLoading).toBe(false)
  })

  it('logout clears all state', async () => {
    mockedRpc.login.mockResolvedValue(null)
    mockedRpc.logout.mockResolvedValue(undefined)
    const store = useAppStore()

    // Login first
    await store.login('password123')
    expect(store.isAuthenticated).toBe(true)

    // Then logout
    await store.logout()

    expect(store.isAuthenticated).toBe(false)
    expect(store.data).toBeNull()
    expect(store.isConnected).toBe(false)
    expect(localStorage.getItem('neode-auth')).toBeNull()
    expect(mockedWs.disconnect).toHaveBeenCalled()
  })

  it('logout still clears state even if RPC fails', async () => {
    mockedRpc.logout.mockRejectedValue(new Error('Network error'))
    localStorage.setItem('neode-auth', 'true')
    const store = useAppStore()

    await store.logout()

    expect(store.isAuthenticated).toBe(false)
    expect(localStorage.getItem('neode-auth')).toBeNull()
  })

  it('checkSession returns true on valid session', async () => {
    localStorage.setItem('neode-auth', 'true')
    mockedRpc.call.mockResolvedValue('ping')
    const store = useAppStore()

    const valid = await store.checkSession()

    expect(valid).toBe(true)
    expect(store.isAuthenticated).toBe(true)
    expect(store.data).not.toBeNull()
  })

  it('checkSession returns false when no auth in localStorage', async () => {
    const store = useAppStore()

    const valid = await store.checkSession()

    expect(valid).toBe(false)
    expect(mockedRpc.call).not.toHaveBeenCalled()
  })

  it('checkSession can validate an app-gate cookie without a dashboard localStorage marker', async () => {
    mockedRpc.call.mockResolvedValue('ping')
    const store = useAppStore()

    const valid = await store.checkSession({
      allowCookieWithoutLocalMarker: true,
      bootstrapDashboard: false,
    })

    expect(valid).toBe(true)
    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'system.get-hostname',
    })
    expect(mockedRpc.call).toHaveBeenCalledOnce()
    expect(mockedWs.connect).not.toHaveBeenCalled()
    expect(store.data).toBeNull()
    expect(store.isAuthenticated).toBe(true)
    expect(localStorage.getItem('neode-auth')).toBe('true')
  })

  it('checkSession rejects a missing app-gate cookie when explicitly probed', async () => {
    mockedRpc.call.mockRejectedValue(new Error('401 Unauthorized'))
    const store = useAppStore()

    const valid = await store.checkSession({ allowCookieWithoutLocalMarker: true })

    expect(valid).toBe(false)
    expect(store.isAuthenticated).toBe(false)
    expect(localStorage.getItem('neode-auth')).toBeNull()
  })

  it('checkSession returns false and clears state on expired session', async () => {
    localStorage.setItem('neode-auth', 'true')
    mockedRpc.call.mockRejectedValue(new Error('401 Unauthorized'))
    const store = useAppStore()

    const valid = await store.checkSession()

    expect(valid).toBe(false)
    expect(store.isAuthenticated).toBe(false)
    expect(localStorage.getItem('neode-auth')).toBeNull()
    expect(mockedWs.disconnect).toHaveBeenCalled()
  })

  it('connectWebSocket subscribes and connects', async () => {
    mockedWs.connect.mockResolvedValue(undefined)
    // First call: not connected (triggers connect), second call: connected (after connect)
    mockedWs.isConnected.mockReturnValueOnce(false).mockReturnValue(true)
    const store = useAppStore()

    await store.connectWebSocket()

    expect(mockedWs.subscribe).toHaveBeenCalledOnce()
    expect(mockedWs.onConnectionStateChange).toHaveBeenCalledOnce()
    expect(mockedWs.connect).toHaveBeenCalledOnce()
  })

  it('needsSessionValidation returns true when auth but not validated', () => {
    localStorage.setItem('neode-auth', 'true')
    const store = useAppStore()

    // isAuthenticated is true from localStorage, but sessionValidated is false
    expect(store.needsSessionValidation()).toBe(true)
  })
})
