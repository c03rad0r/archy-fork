import { describe, expect, it } from 'vitest'
import { getHomeRecommendedApps, isMarketplaceAppInstalled } from '../homeRecommendations'
import type { MarketplaceApp } from '@/views/marketplace/marketplaceData'

const apps: MarketplaceApp[] = [
  { id: 'vaultwarden', title: 'Vaultwarden', dockerImage: 'vaultwarden:latest' },
  { id: 'bitcoin-knots', title: 'Bitcoin Knots', dockerImage: 'bitcoin:latest' },
  { id: 'homeassistant', title: 'Home Assistant', dockerImage: 'homeassistant:latest' },
  { id: 'mempool', title: 'Mempool', dockerImage: 'mempool:latest' },
  { id: 'thunderhub', title: 'ThunderHub', dockerImage: 'thunderhub:latest' },
  { id: 'dwn', title: 'DWN', dockerImage: 'dwn:latest' },
  { id: 'website-only', title: 'Website Only', webUrl: 'https://example.com' },
]

describe('homeRecommendations', () => {
  it('treats installed aliases as installed apps', () => {
    expect(isMarketplaceAppInstalled('mempool', { 'mempool-web': {} })).toBe(true)
    expect(isMarketplaceAppInstalled('mempool', { vaultwarden: {} })).toBe(false)
  })

  it('returns uninstalled installable core and recommended apps for Home', () => {
    const recommended = getHomeRecommendedApps(apps, { 'mempool-web': {} }, 3)

    expect(recommended.map((app) => app.id)).toEqual([
      'bitcoin-knots',
      'vaultwarden',
      'homeassistant',
    ])
  })

  it('fills from optional apps once core and recommended apps are installed', () => {
    const recommended = getHomeRecommendedApps(apps, {
      'bitcoin-knots': {},
      'mempool-web': {},
      vaultwarden: {},
    })

    expect(recommended.map((app) => app.id)).toEqual(['homeassistant'])
    expect(recommended.some((app) => app.id === 'dwn' || app.id === 'thunderhub')).toBe(false)
  })
})
