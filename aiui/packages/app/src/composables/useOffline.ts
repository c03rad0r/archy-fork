import { ref, onMounted, onUnmounted } from 'vue'

export interface SyncAction {
  type: string
  payload: unknown
  timestamp: number
}

const isOnline = ref(navigator.onLine)
const pendingSync = ref(0)
const syncQueue: SyncAction[] = []

function handleOnline() {
  isOnline.value = true
  processSyncQueue()
}

function handleOffline() {
  isOnline.value = false
}

/** Queue an action to be synced when back online */
function queueForSync(action: SyncAction) {
  syncQueue.push(action)
  pendingSync.value = syncQueue.length
}

/** Process all pending sync actions */
async function processSyncQueue(): Promise<void> {
  if (!isOnline.value || syncQueue.length === 0) return

  while (syncQueue.length > 0) {
    const action = syncQueue.shift()
    if (!action) break

    try {
      // Route sync actions based on type
      if (action.type === 'conversation') {
        // Conversation sync handled by idb-storage auto-save
        continue
      }
      if (action.type === 'favorite') {
        // Favorites are local-only, no sync needed
        continue
      }
      // Add more sync handlers as needed
    } catch {
      // Re-queue failed actions
      syncQueue.unshift(action)
      break
    }
  }

  pendingSync.value = syncQueue.length
}

export function useOffline() {
  onMounted(() => {
    window.addEventListener('online', handleOnline)
    window.addEventListener('offline', handleOffline)
    isOnline.value = navigator.onLine
  })

  onUnmounted(() => {
    window.removeEventListener('online', handleOnline)
    window.removeEventListener('offline', handleOffline)
  })

  return {
    isOnline,
    pendingSync,
    queueForSync,
    processSyncQueue,
  }
}
