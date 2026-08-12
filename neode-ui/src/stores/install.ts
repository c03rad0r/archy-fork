// Install store — tracks in-progress app installations across navigation.
// Marketplace.vue writes here; Apps.vue reads to show "Installing..." cards.

import { defineStore } from 'pinia'
import { reactive, computed } from 'vue'

export interface InstallEntry {
  id: string
  title: string
  status: 'downloading' | 'installing' | 'starting' | 'complete' | 'error'
  progress: number
  message: string
}

export const useInstallStore = defineStore('install', () => {
  // Reactive map: appId -> InstallEntry
  const entries = reactive(new Map<string, InstallEntry>())

  /** All app IDs currently installing */
  const installingIds = computed(() => new Set(entries.keys()))

  /** Start tracking an install */
  function trackInstall(id: string, title: string) {
    entries.set(id, {
      id,
      title,
      status: 'downloading',
      progress: 0,
      message: 'Preparing installation...',
    })
  }

  /** Update progress for an in-flight install */
  function updateProgress(id: string, update: Partial<Omit<InstallEntry, 'id'>>) {
    const current = entries.get(id)
    if (!current) return
    entries.set(id, { ...current, ...update })
  }

  /** Mark install complete and auto-clear after delay */
  function completeInstall(id: string) {
    const current = entries.get(id)
    if (!current) return
    entries.set(id, { ...current, status: 'complete', progress: 100, message: 'Installation complete!' })
    setTimeout(() => entries.delete(id), 2000)
  }

  /** Mark install as failed and auto-clear after delay */
  function failInstall(id: string, message: string) {
    const current = entries.get(id)
    if (!current) return
    entries.set(id, { ...current, status: 'error', progress: 0, message })
    setTimeout(() => entries.delete(id), 5000)
  }

  /** Remove tracking (e.g. when backend reports the app is installed) */
  function clearInstall(id: string) {
    entries.delete(id)
  }

  /** Check if an app is currently installing */
  function isInstalling(id: string): boolean {
    return entries.has(id)
  }

  /** Get progress for an app, or undefined */
  function getProgress(id: string): InstallEntry | undefined {
    return entries.get(id)
  }

  return {
    entries,
    installingIds,
    trackInstall,
    updateProgress,
    completeInstall,
    failInstall,
    clearInstall,
    isInstalling,
    getProgress,
  }
})
