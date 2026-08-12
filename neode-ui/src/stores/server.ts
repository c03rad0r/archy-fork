// Server store — computed server state and RPC action proxies

import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { rpcClient } from '../api/rpc-client'
import { useSyncStore } from './sync'
import type { InstallProgress } from '../views/marketplace/marketplaceData'
import type { InstallPhase } from '../types/api'

/**
 * Phase-to-UI mapping. Each backend pipeline phase maps to a fixed
 * progress percentage (so the bar only ever advances forward) and a
 * descriptive label the user can actually understand. This is the
 * source of truth — byte counters from `install-progress.size/downloaded`
 * are a fallback for the rare cases where podman does emit parseable
 * progress on a piped stderr.
 *
 * Percentages chosen so:
 *   - the bar is never fully empty (users panic)
 *   - the bar visibly advances at every phase boundary
 *   - the slowest phases (PullingImage, WaitingHealthy) get the widest
 *     bands so shimmer/indeterminate treatment has room
 *   - 100% is reserved for "Done" / terminal success
 */
const PHASE_INFO: Record<InstallPhase, { progress: number; message: string; status: InstallProgress['status'] }> = {
  'preparing':          { progress: 5,  message: 'Preparing…',                status: 'downloading' },
  'pulling-image':      { progress: 20, message: 'Downloading image…',        status: 'downloading' },
  'creating-container': { progress: 70, message: 'Creating container…',       status: 'installing' },
  'starting-container': { progress: 80, message: 'Starting container…',       status: 'starting' },
  'waiting-healthy':    { progress: 88, message: 'Finalizing first start…',   status: 'starting' },
  'post-install':       { progress: 95, message: 'Finalizing…',                status: 'installing' },
  'done':               { progress: 100, message: 'Installed',                status: 'complete' },
}

export const useServerStore = defineStore('server', () => {
  const sync = useSyncStore()

  // Global install/uninstall tracking — persists across navigation
  const installingApps = ref<Map<string, InstallProgress>>(new Map())
  const uninstallingApps = ref<Set<string>>(new Set())

  // Watch WebSocket data for real install progress — runs globally, not just on Marketplace page
  watch(() => sync.packages, (packages) => {
    if (!packages) return
    for (const [appId, pkg] of Object.entries(packages)) {
      if ((pkg.state as string) === 'installing' || (pkg.state as string) === 'updating') {
        // Backend confirms it's installing — update or create tracking entry
        if (!installingApps.value.has(appId)) {
          installingApps.value.set(appId, {
            id: appId,
            title: pkg.manifest?.title || appId,
            status: 'downloading',
            progress: 0,
            message: 'Installing…',
            attempt: 0,
          })
        }
        const progress = pkg['install-progress']
        if (progress) {
          const current = installingApps.value.get(appId)!
          // Primary source: the pipeline phase. Each phase maps to a
          // fixed progress% and a user-facing label.
          if (progress.phase) {
            const info = PHASE_INFO[progress.phase]
            if (info) {
              // Within the PullingImage band (20→70%), interpolate the
              // bar based on how many images / bytes have landed so far.
              // Without this, multi-container stacks (indeedhub: 7,
              // mempool: 3, btcpay: 4) just sit at 20% for the entire
              // pull duration — exactly what the user reported as
              // "Downloading sticks at 20% mostly". X-of-N progress
              // comes from set_install_progress(i, n) in stacks.rs.
              let bandProgress = info.progress
              if (progress.phase === 'pulling-image' && progress.size > 0) {
                const fraction = Math.min(progress.downloaded / progress.size, 1)
                // PullingImage band: 20% → 70%, so 50pp to interpolate over.
                bandProgress = 20 + Math.round(fraction * 50)
              }
              // Only advance forward — never let the bar step backward
              // between patches (can happen briefly during scan merges).
              const nextProgress = Math.max(current.progress, bandProgress)
              // Show explicit message when set (e.g. install-fail descriptions
              // surfaced via install_progress.message) — otherwise PHASE_INFO label.
              const label = progress.message || info.message
              installingApps.value.set(appId, {
                ...current,
                status: info.status,
                progress: nextProgress,
                message: label,
              })
              continue
            }
          }
          // No phase but message is set (install-fail path) — show the message
          // even if PHASE_INFO doesn't apply. Status stays whatever the watcher
          // currently has.
          if (progress.message) {
            installingApps.value.set(appId, {
              ...current,
              message: progress.message,
            })
            continue
          }
          // Fallback: byte counters (rare — podman usually doesn't
          // emit parseable progress on a piped stderr).
          const pct = progress.size > 0 ? Math.round((progress.downloaded / progress.size) * 100) : 0
          const downloadedMB = (progress.downloaded / (1024 * 1024)).toFixed(1)
          const totalMB = (progress.size / (1024 * 1024)).toFixed(1)
          let message = 'Downloading…'
          if (progress.size > 1024 && pct < 100) {
            message = `Downloading: ${downloadedMB} / ${totalMB} MB (${pct}%)`
          } else if (pct >= 100 || (progress.size > 0 && progress.downloaded >= progress.size)) {
            message = 'Installing package…'
          }
          installingApps.value.set(appId, {
            ...current,
            status: pct >= 100 ? 'installing' : 'downloading',
            progress: Math.max(current.progress, Math.min(pct, 95)),
            message,
          })
        }
      } else if (installingApps.value.has(appId)) {
        const state = pkg.state as string
        // Only clear when app is fully running or definitively stopped — not during 'starting' transition
        if (state === 'running' || state === 'stopped' || state === 'exited') {
          installingApps.value.delete(appId)
        }
      }

      if ((pkg.state as string) === 'removing') {
        uninstallingApps.value.add(appId)
      } else if (uninstallingApps.value.has(appId)) {
        uninstallingApps.value.delete(appId)
      }
    }
    // Clear installingApps entries for apps that vanished from backend data
    // Only clean up entries that have errored — active installs may take minutes to pull images
    for (const [appId] of installingApps.value) {
      if (packages && !(appId in packages)) {
        const entry = installingApps.value.get(appId)
        if (entry && entry.status === 'error') {
          installingApps.value.delete(appId)
        }
      }
    }
    // Clear uninstallingApps when the container disappears from backend data
    for (const appId of uninstallingApps.value) {
      if (packages && !(appId in packages)) {
        uninstallingApps.value.delete(appId)
      }
    }
  }, { deep: true })

  function setInstallProgress(appId: string, progress: Partial<InstallProgress> & { id: string; title: string }) {
    const existing = installingApps.value.get(appId)
    installingApps.value.set(appId, {
      status: 'downloading',
      progress: 0,
      message: 'Preparing...',
      attempt: 0,
      ...existing,
      ...progress,
    })
  }

  function clearInstallProgress(appId: string) {
    installingApps.value.delete(appId)
  }

  function isInstalling(appId: string): boolean {
    return installingApps.value.has(appId)
  }

  // Computed — derived from sync store's data
  const serverName = computed(() => sync.serverInfo?.name || 'Archipelago')
  const isRestarting = computed(() => sync.serverInfo?.['status-info']?.restarting || false)
  const isShuttingDown = computed(() => sync.serverInfo?.['status-info']?.['shutting-down'] || false)
  const isOffline = computed(() => !sync.isConnected || isRestarting.value || isShuttingDown.value)

  // Package actions. install/uninstall/update are async on the backend:
  // the RPC returns immediately with { status: 'installing'|'removing'|'updating',
  // package_id } after flipping state, and the real work runs in a spawn.
  // Progress is streamed via the WebSocket state push, not the RPC response.
  async function installPackage(
    id: string,
    marketplaceUrl: string,
    version: string,
  ): Promise<{ status: string; package_id: string }> {
    return rpcClient.installPackage(id, marketplaceUrl, version)
  }

  async function uninstallPackage(
    id: string,
    options: { preserveData?: boolean } = {},
  ): Promise<{ status: string; package_id: string }> {
    return rpcClient.uninstallPackage(id, options)
  }

  async function startPackage(id: string): Promise<void> {
    return rpcClient.startPackage(id)
  }

  async function stopPackage(id: string): Promise<void> {
    return rpcClient.stopPackage(id)
  }

  async function restartPackage(id: string): Promise<void> {
    return rpcClient.restartPackage(id)
  }

  async function updatePackage(id: string): Promise<{ status: string; package_id: string }> {
    return rpcClient.updatePackage(id)
  }

  // Server actions
  async function updateServer(marketplaceUrl: string): Promise<'updating' | 'no-updates'> {
    return rpcClient.updateServer(marketplaceUrl)
  }

  async function restartServer(): Promise<void> {
    return rpcClient.restartServer()
  }

  async function shutdownServer(): Promise<void> {
    return rpcClient.shutdownServer()
  }

  async function getMetrics(): Promise<Record<string, unknown>> {
    return rpcClient.getMetrics()
  }

  // Marketplace actions
  async function getMarketplace(url: string): Promise<Record<string, unknown>> {
    return rpcClient.getMarketplace(url)
  }

  function updateServerName(name: string) {
    if (sync.data?.['server-info']) {
      sync.data['server-info'].name = name
    }
  }

  return {
    // Computed
    serverName,
    isRestarting,
    isShuttingDown,
    isOffline,

    // Install/uninstall tracking (global, persists across navigation)
    installingApps,
    setInstallProgress,
    clearInstallProgress,
    isInstalling,
    uninstallingApps,

    // Actions
    installPackage,
    uninstallPackage,
    startPackage,
    stopPackage,
    restartPackage,
    updatePackage,
    updateServer,
    restartServer,
    shutdownServer,
    getMetrics,
    getMarketplace,
    updateServerName,
  }
})
