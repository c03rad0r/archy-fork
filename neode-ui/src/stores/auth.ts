// Authentication store — login, logout, session management

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { rpcClient } from '../api/rpc-client'
import { useSyncStore } from './sync'
import { useResourcesStore } from './resources'

export const useAuthStore = defineStore('auth', () => {
  // State
  const isAuthenticated = ref(localStorage.getItem('neode-auth') === 'true')
  const isLoading = ref(false)
  const error = ref<string | null>(null)
  let sessionValidated = false

  // Actions
  async function login(password: string): Promise<{ requires_totp?: boolean }> {
    isLoading.value = true
    error.value = null

    try {
      const result = await rpcClient.login(password)
      if (result && result.requires_totp) {
        return { requires_totp: true }
      }

      isAuthenticated.value = true
      sessionValidated = true
      try { localStorage.setItem('neode-auth', 'true') } catch { /* localStorage full or unavailable */ }

      const sync = useSyncStore()

      // Initialize data structure immediately so dashboard can render
      await sync.initializeData()

      // Verify session cookies are established before WebSocket connect.
      // Without this, the WS upgrade can race ahead of cookie processing → 401.
      try {
        await rpcClient.call({ method: 'server.echo', params: { message: 'session-ready' } })
      } catch {
        // Non-fatal: WS reconnect logic will handle it
      }

      // Connect WebSocket in background
      sync.connectWebSocket().catch((err) => {
        if (import.meta.env.DEV) console.warn('[Store] WebSocket connection failed after login, will retry:', err)
      })
      return {}
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Login failed'
      throw err
    } finally {
      isLoading.value = false
    }
  }

  async function completeLoginAfterTotp(): Promise<void> {
    isAuthenticated.value = true
    sessionValidated = true
    try { localStorage.setItem('neode-auth', 'true') } catch { /* localStorage full or unavailable */ }

    const sync = useSyncStore()
    await sync.initializeData()

    // Verify session cookies are established before WebSocket connect
    try {
      await rpcClient.call({ method: 'server.echo', params: { message: 'session-ready' } })
    } catch {
      // Non-fatal: WS reconnect logic will handle it
    }

    sync.connectWebSocket().catch((err) => {
      if (import.meta.env.DEV) console.warn('[Store] WebSocket connection failed after TOTP login, will retry:', err)
    })
  }

  async function logout(): Promise<void> {
    const sync = useSyncStore()
    try {
      await rpcClient.logout()
    } catch (err) {
      if (import.meta.env.DEV) console.error('Logout error:', err)
    } finally {
      isAuthenticated.value = false
      sessionValidated = false
      localStorage.removeItem('neode-auth')
      sync.resetOnLogout()
      // Purge every cached resource (memory + sessionStorage) regardless of
      // whether the server-side logout RPC succeeded — a failed remote
      // logout must not leave a locally-cached payload behind (T-02-02).
      useResourcesStore().clearAll()
    }
  }

  async function checkSession(options: {
    allowCookieWithoutLocalMarker?: boolean
    bootstrapDashboard?: boolean
  } = {}): Promise<boolean> {
    // `neode-auth` is only a client-side hint; the HttpOnly session cookie is
    // the authority. Most dashboard navigations deliberately require the hint
    // so logging out does not immediately resurrect a still-expiring cookie.
    // The contained tab signer is the exception: an app-gate login happens on
    // the app's port and sets the shared host cookie, but cannot set dashboard-
    // origin localStorage. Let that route validate the real cookie explicitly.
    if (!options.allowCookieWithoutLocalMarker && !localStorage.getItem('neode-auth')) {
      return false
    }

    try {
      // Unlike public `server.echo`, this implemented read-only method requires
      // a valid session while remaining CSRF-exempt. That makes checkSession a
      // real authentication check, including for the app-gate cookie bootstrap.
      await rpcClient.call({ method: 'system.get-hostname' })
      isAuthenticated.value = true
      sessionValidated = true
      try { localStorage.setItem('neode-auth', 'true') } catch { /* localStorage full or unavailable */ }

      // The hidden signer broker only needs proof of the session cookie. Do
      // not make its first consent prompt wait for a full dashboard snapshot
      // and WebSocket connection; a normal dashboard check keeps this default.
      if (options.bootstrapDashboard !== false) {
        const sync = useSyncStore()
        await sync.initializeData()

        sync.connectWebSocket().catch((err) => {
          if (import.meta.env.DEV) console.warn('[Store] WebSocket reconnection failed, will retry:', err)
        })
      }

      return true
    } catch (err) {
      if (import.meta.env.DEV) console.error('[Store] Session check failed:', err)
      localStorage.removeItem('neode-auth')
      isAuthenticated.value = false
      sessionValidated = false

      const sync = useSyncStore()
      sync.resetOnLogout()
      return false
    }
  }

  function needsSessionValidation(): boolean {
    return isAuthenticated.value && !sessionValidated
  }

  return {
    // State
    isAuthenticated,
    isLoading,
    error,

    // Actions
    login,
    completeLoginAfterTotp,
    logout,
    checkSession,
    needsSessionValidation,
  }
})
