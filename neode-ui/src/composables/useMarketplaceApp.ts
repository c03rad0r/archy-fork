import { ref } from 'vue'

export interface MarketplaceAppInfo {
  id: string
  title: string
  version: string
  icon: string
  category: string
  description: string | { short?: string; long?: string }
  author: string
  source: string
  manifestUrl: string
  url: string
  repoUrl: string
  s9pkUrl: string
  dockerImage: string
  /** External web URL for iframe-based web apps (no container needed) */
  webUrl?: string
  screenshots?: AppScreenshot[]
  containerConfig?: {
    ports?: string[]
    volumes?: string[]
    env?: string[]
    command?: string
    args?: string[]
  }
}

export type AppScreenshot = string | {
  src: string
  alt?: string
}

// Simple in-memory store for the current marketplace app
const currentMarketplaceApp = ref<MarketplaceAppInfo | null>(null)

export function useMarketplaceApp() {
  function setCurrentApp(app: Partial<MarketplaceAppInfo> & { id: string }) {
    // Create a clean, serializable copy
    currentMarketplaceApp.value = {
      id: app.id,
      title: app.title ?? '',
      version: app.version ?? '',
      icon: app.icon ?? '',
      category: app.category ?? '',
      description: app.description ?? '',
      author: app.author ?? '',
      source: app.source ?? '',
      manifestUrl: app.manifestUrl || app.s9pkUrl || app.url || '',
      url: app.url || app.s9pkUrl || app.manifestUrl || '',
      repoUrl: app.repoUrl ?? '',
      s9pkUrl: app.s9pkUrl ?? '',
      dockerImage: app.dockerImage ?? '',
      webUrl: (app as Record<string, unknown>).webUrl as string | undefined,
      screenshots: Array.isArray(app.screenshots) ? app.screenshots : undefined,
      containerConfig: (app as Record<string, unknown>).containerConfig as MarketplaceAppInfo['containerConfig'],
    }
  }

  function getCurrentApp() {
    return currentMarketplaceApp.value
  }

  function clearCurrentApp() {
    currentMarketplaceApp.value = null
  }

  return {
    setCurrentApp,
    getCurrentApp,
    clearCurrentApp
  }
}
