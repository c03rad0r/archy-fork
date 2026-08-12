const STORAGE_KEY = 'archipelago-app-usage'

export interface AppUsageEntry {
  count: number
  lastLaunchedAt: number
}

export type AppUsageMap = Record<string, AppUsageEntry>

function readUsage(): AppUsageMap {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as AppUsageMap
    if (!parsed || typeof parsed !== 'object') return {}
    return parsed
  } catch {
    return {}
  }
}

function writeUsage(usage: AppUsageMap) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(usage))
  } catch {
    // Ignore unavailable or full localStorage.
  }
}

export function recordAppLaunch(appId: string, now = Date.now()) {
  if (!appId) return
  const usage = readUsage()
  const current = usage[appId]
  usage[appId] = {
    count: (current?.count || 0) + 1,
    lastLaunchedAt: now,
  }
  writeUsage(usage)
}

export function getAppUsage(): AppUsageMap {
  return readUsage()
}
