import { getAppTier, INSTALLED_ALIASES, type MarketplaceApp } from '@/views/marketplace/marketplaceData'

export type InstalledPackageMap = Record<string, unknown>

export function isMarketplaceAppInstalled(
  appId: string,
  installedPackages: InstalledPackageMap,
  aliases: Record<string, string[]> = INSTALLED_ALIASES,
): boolean {
  if (appId in installedPackages) return true
  return aliases[appId]?.some((alias) => alias in installedPackages) ?? false
}

export function getHomeRecommendedApps(
  apps: MarketplaceApp[],
  installedPackages: InstalledPackageMap,
  limit = 3,
): MarketplaceApp[] {
  const candidates = apps
    .filter((app) => app.id !== 'dwn' && app.id !== 'thunderhub')
    .filter((app) => {
      if (!app.dockerImage) return false
      if (isMarketplaceAppInstalled(app.id, installedPackages)) return false
      return true
    })
    .sort((a, b) => {
      const tierRank = (app: MarketplaceApp) => {
        const tier = getAppTier(app.id)
        if (tier === 'core') return 0
        if (tier === 'recommended') return 1
        return 2
      }
      const tierDiff = tierRank(a) - tierRank(b)
      if (tierDiff !== 0) return tierDiff
      return (a.title || a.id).localeCompare(b.title || b.id)
    })
  return candidates.slice(0, limit)
}
