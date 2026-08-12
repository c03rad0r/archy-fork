/** Composable for app start/stop/restart/uninstall actions */

import { ref, onBeforeUnmount } from 'vue'
import { useAppStore } from '@/stores/app'
import { useServerStore } from '@/stores/server'

export function useAppsActions() {
  const store = useAppStore()
  const serverStore = useServerStore()
  const loadingActions = ref<Record<string, boolean>>({})
  const actionError = ref('')
  let errorTimer: ReturnType<typeof setTimeout> | undefined
  const actionTimers = new Map<string, ReturnType<typeof setTimeout>>()

  const uninstalling = ref(false)
  // Use global store so uninstall state persists across navigation
  const uninstallingApps = serverStore.uninstallingApps

  function showActionError(msg: string) {
    actionError.value = msg
    if (errorTimer) clearTimeout(errorTimer)
    errorTimer = setTimeout(() => { actionError.value = '' }, 5000)
  }

  async function startApp(id: string) {
    loadingActions.value[id] = true
    try {
      await store.startPackage(id)
      if (actionTimers.has(id)) clearTimeout(actionTimers.get(id)!)
      actionTimers.set(id, setTimeout(() => {
        loadingActions.value[id] = false
        actionTimers.delete(id)
      }, 5000))
    } catch (err) {
      if (import.meta.env.DEV) console.error('Failed to start app:', err)
      showActionError(`Failed to start app: ${err instanceof Error ? err.message : 'Unknown error'}`)
      loadingActions.value[id] = false
    }
  }

  async function stopApp(id: string) {
    loadingActions.value[id] = true
    try {
      await store.stopPackage(id)
      if (actionTimers.has(id)) clearTimeout(actionTimers.get(id)!)
      actionTimers.set(id, setTimeout(() => {
        loadingActions.value[id] = false
        actionTimers.delete(id)
      }, 5000))
    } catch (err) {
      if (import.meta.env.DEV) console.error('Failed to stop app:', err)
      showActionError(`Failed to stop app: ${err instanceof Error ? err.message : 'Unknown error'}`)
      loadingActions.value[id] = false
    }
  }

  async function restartApp(id: string) {
    loadingActions.value[id] = true
    try {
      await store.restartPackage(id)
      if (actionTimers.has(id)) clearTimeout(actionTimers.get(id)!)
      actionTimers.set(id, setTimeout(() => {
        loadingActions.value[id] = false
        actionTimers.delete(id)
      }, 8000))
    } catch (err) {
      if (import.meta.env.DEV) console.error('Failed to restart app:', err)
      showActionError(`Failed to restart app: ${err instanceof Error ? err.message : 'Unknown error'}`)
      loadingActions.value[id] = false
    }
  }

  async function confirmUninstall(appId: string, options: { preserveData?: boolean } = {}) {
    uninstalling.value = true
    try {
      uninstallingApps.add(appId)
      await store.uninstallPackage(appId, options)
      // Don't clear uninstallingApps here — let the WebSocket watcher clear it
      // when the container actually disappears from backend data
    } catch (err) {
      if (import.meta.env.DEV) console.error('Failed to uninstall app:', err)
      showActionError(`Failed to uninstall: ${err instanceof Error ? err.message : 'Unknown error'}`)
      uninstallingApps.delete(appId)
    } finally {
      uninstalling.value = false
    }
  }

  onBeforeUnmount(() => {
    for (const t of actionTimers.values()) clearTimeout(t)
    actionTimers.clear()
    if (errorTimer) clearTimeout(errorTimer)
  })

  return {
    loadingActions,
    actionError,
    uninstalling,
    uninstallingApps,
    showActionError,
    startApp,
    stopApp,
    restartApp,
    confirmUninstall,
  }
}
