// Sync store — WebSocket connection, real-time data, patch application

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { DataModel, PatchOperation } from '../types/api'
import { wsClient, applyDataPatch } from '../api/websocket'
import { rpcClient } from '../api/rpc-client'
import { useResourcesStore } from './resources'

/** Unescape one JSON-pointer segment (RFC 6901: ~1 → '/', ~0 → '~'). */
function pointerSegment(path: string, prefix: string): string {
  const seg = path.slice(prefix.length).split('/')[0] ?? ''
  return seg.replace(/~1/g, '/').replace(/~0/g, '~')
}

/** B5: bridge /ws/db pushes into the cached-resource layer. Each patch op
 * maps to the resource keys whose backing data it changes; invalidate()
 * debounces (800ms) and only refetches keys with mounted subscribers, so a
 * patch storm costs one revalidation per key. The 30s staleness
 * reconciliation stays as the backstop for anything unmapped. */
function invalidateResourcesForPatch(patch: PatchOperation[]): void {
  const resources = useResourcesStore()
  for (const op of patch) {
    const path = op.path ?? ''
    if (path.startsWith('/peer-health/')) {
      // A peer flipping reachability changes both its browse result and the
      // federation node list's online state.
      const onion = pointerSegment(path, '/peer-health/')
      if (onion) resources.invalidate(`cloud.peer-browse:${onion}`)
      resources.invalidate('federation.nodes')
    } else if (path.startsWith('/package-data/')) {
      // App installs/uninstalls add or remove their tor services.
      resources.invalidate('server.tor-services')
    }
  }
}

export const useSyncStore = defineStore('sync', () => {
  // State
  const data = ref<DataModel | null>(null)
  const isConnected = ref(false)
  const isReconnecting = ref(false)
  const hasLoadedInitialData = ref(false)
  let isWsSubscribed = false
  let isWsConnecting = false
  let isRefreshingState = false
  let stalenessTimer: ReturnType<typeof setInterval> | null = null
  const STALENESS_INTERVAL_MS = 30_000

  // Computed
  const serverInfo = computed(() => data.value?.['server-info'])
  const packages = computed(() => data.value?.['package-data'] || {})
  const peerHealth = computed<Record<string, boolean>>(() => data.value?.['peer-health'] || {})
  const uiData = computed(() => data.value?.ui)

  /**
   * Refetch the full state snapshot via RPC and apply it through the same
   * path as the post-connect fetch (revision/hasLoadedInitialData handling
   * stays identical). Guarded against overlapping fetches.
   */
  async function refreshStateFromServer(): Promise<void> {
    if (isRefreshingState) return
    isRefreshingState = true
    try {
      const freshState = await rpcClient.call<{ data: DataModel }>({ method: 'server.get-state' })
      if (freshState?.data) {
        data.value = freshState.data
        hasLoadedInitialData.value = true
      }
    } catch {
      // Non-fatal: WebSocket patches will still work
      if (import.meta.env.DEV) console.warn('[Store] Failed to refresh state from server')
    } finally {
      isRefreshingState = false
    }
  }

  /**
   * Belt-and-suspenders against missed broadcasts: while the WebSocket is
   * connected, periodically resync the full state so a dropped patch can
   * never permanently strand the UI on stale data.
   */
  function startStalenessReconciliation(): void {
    if (stalenessTimer) return
    stalenessTimer = setInterval(() => {
      if (wsClient.isConnected()) void refreshStateFromServer()
    }, STALENESS_INTERVAL_MS)
  }

  function stopStalenessReconciliation(): void {
    if (stalenessTimer) {
      clearInterval(stalenessTimer)
      stalenessTimer = null
    }
  }

  // Actions
  async function connectWebSocket(): Promise<void> {
    // Prevent concurrent connection attempts
    if (isWsConnecting) return
    isWsConnecting = true

    try {
      if (import.meta.env.DEV) console.log('[Store] Connecting WebSocket...')
      isReconnecting.value = true

      // Don't create multiple subscriptions - check if already subscribed
      if (!isWsSubscribed) {
        // Subscribe to updates BEFORE connecting (so we catch initial data)
        isWsSubscribed = true

        // Listen for connection state changes
        wsClient.onConnectionStateChange((state) => {
          if (import.meta.env.DEV) console.log('[Store] WebSocket connection state changed:', state)
          isConnected.value = state === 'connected'
          isReconnecting.value = state === 'connecting'
        })

        wsClient.subscribe((update: { type?: string; data?: DataModel; rev?: number; patch?: import('@/types/api').PatchOperation[] }) => {
        // Handle mock backend format: {type: 'initial', data: {...}}
        if (update?.type === 'initial' && update?.data) {
          if (import.meta.env.DEV) console.log('[Store] Received initial data from mock backend')
          data.value = update.data
          hasLoadedInitialData.value = true
          isConnected.value = true
          isReconnecting.value = false
        }
        // Handle real backend format: {rev: 0, data: {...}}
        else if (update?.data && update?.rev !== undefined) {
          data.value = update.data
          hasLoadedInitialData.value = true
          isConnected.value = true
          isReconnecting.value = false
        }
        // Handle patch updates (both backends)
        else if (data.value && update?.patch) {
          try {
            if (import.meta.env.DEV) console.log('[Store] Applying patch at revision', update.rev || 'unknown')
          data.value = applyDataPatch(data.value, update.patch)
            invalidateResourcesForPatch(update.patch)
            // Mark as connected once we receive any valid patch
            if (!isConnected.value) {
              isConnected.value = true
              isReconnecting.value = false
            }
          } catch (err) {
            if (import.meta.env.DEV) console.error('[Store] Failed to apply WebSocket patch:', err)
          }
        }
        })
      }

      // Now connect (or reconnect if already connected)
      // Only attempt to connect if not already connected
      if (wsClient.isConnected()) {
        if (import.meta.env.DEV) console.log('[Store] WebSocket already connected')
        isConnected.value = true
        isReconnecting.value = false
        // Re-entrant call while already connected (e.g. after onboarding
        // completes): resync state in case a broadcast was missed.
        void refreshStateFromServer()
        startStalenessReconciliation()
        return
      }

      await wsClient.connect()
      if (import.meta.env.DEV) console.log('[Store] WebSocket connected')

      // Fetch fresh state after reconnect to avoid stale patch application
      await refreshStateFromServer()
      startStalenessReconciliation()

      // Connection state will be updated via the callback
      if (wsClient.isConnected()) {
        isConnected.value = true
        isReconnecting.value = false
      }

    } catch (err) {
      if (import.meta.env.DEV) console.error('[Store] WebSocket connection failed:', err)
      // Don't mark as disconnected immediately - let reconnection logic handle it
      // The WebSocket client will retry automatically
      isReconnecting.value = true
      isConnected.value = false
      // Don't throw - allow app to work without real-time updates
      // The WebSocket will reconnect in the background
    } finally {
      isWsConnecting = false
    }
  }

  async function initializeData(): Promise<void> {
    // Initialize with empty data structure
    // The WebSocket will populate it with real data
    data.value = {
      'server-info': {
        id: '',
        version: '',
        name: null,
        pubkey: '',
        'status-info': {
          restarting: false,
          'shutting-down': false,
          updated: false,
          'backup-progress': null,
          'update-progress': null,
        },
        'lan-address': null,
        'tor-address': null,
        unread: 0,
        'wifi-ssids': [],
        'zram-enabled': false,
        'seed-backed': false,
      },
      'package-data': {},
      ui: {
        name: null,
        'ack-welcome': '',
        marketplace: {
          'selected-hosts': [],
          'known-hosts': {},
        },
        theme: 'dark',
      },
    }
    hasLoadedInitialData.value = false
  }

  /** Reset sync state on logout — called by auth store */
  function resetOnLogout(): void {
    stopStalenessReconciliation()
    data.value = null
    hasLoadedInitialData.value = false
    isWsSubscribed = false
    wsClient.disconnect()
    isConnected.value = false
    isReconnecting.value = false
  }

  return {
    // State
    data,
    isConnected,
    isReconnecting,
    hasLoadedInitialData,

    // Computed
    serverInfo,
    packages,
    peerHealth,
    uiData,

    // Actions
    connectWebSocket,
    initializeData,
    resetOnLogout,
  }
})
