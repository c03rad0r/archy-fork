// Facade store — re-exports auth, sync, and server stores for backward compatibility.
// All 29+ files that import useAppStore() continue to work without changes.
// Uses defineStore with computed/writableComputed to preserve full reactivity.

import { defineStore, storeToRefs } from 'pinia'
import { useAuthStore } from './auth'
import { useSyncStore } from './sync'
import { useServerStore } from './server'

export const useAppStore = defineStore('app', () => {
  const auth = useAuthStore()
  const sync = useSyncStore()
  const server = useServerStore()

  // Writable refs — delegate reads and writes to the sub-stores
  const { isAuthenticated, isLoading, error } = storeToRefs(auth)
  const { data, isConnected, isReconnecting, hasLoadedInitialData } = storeToRefs(sync)

  // Read-only computed — delegate to sub-stores
  const { serverInfo, packages, peerHealth, uiData } = storeToRefs(sync)
  const { serverName, isRestarting, isShuttingDown, isOffline } = storeToRefs(server)

  return {
    // Auth state (writable refs)
    isAuthenticated,
    isLoading,
    error,

    // Sync state (writable refs)
    data,
    isConnected,
    isReconnecting,
    hasLoadedInitialData,

    // Sync computed (read-only)
    serverInfo,
    packages,
    peerHealth,
    uiData,

    // Server computed (read-only)
    serverName,
    isRestarting,
    isShuttingDown,
    isOffline,

    // Auth actions
    login: auth.login,
    completeLoginAfterTotp: auth.completeLoginAfterTotp,
    logout: auth.logout,
    checkSession: auth.checkSession,
    needsSessionValidation: auth.needsSessionValidation,

    // Sync actions
    connectWebSocket: sync.connectWebSocket,

    // Server actions
    installPackage: server.installPackage,
    uninstallPackage: server.uninstallPackage,
    startPackage: server.startPackage,
    stopPackage: server.stopPackage,
    restartPackage: server.restartPackage,
    updatePackage: server.updatePackage,
    updateServer: server.updateServer,
    restartServer: server.restartServer,
    shutdownServer: server.shutdownServer,
    getMetrics: server.getMetrics,
    getMarketplace: server.getMarketplace,
    updateServerName: server.updateServerName,
  }
})
