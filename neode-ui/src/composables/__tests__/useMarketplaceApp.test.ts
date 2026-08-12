import { describe, it, expect, beforeEach } from 'vitest'
import { useMarketplaceApp } from '../useMarketplaceApp'

describe('useMarketplaceApp', () => {
  beforeEach(() => {
    const { clearCurrentApp } = useMarketplaceApp()
    clearCurrentApp()
  })

  it('getCurrentApp returns null initially', () => {
    const { getCurrentApp } = useMarketplaceApp()
    expect(getCurrentApp()).toBeNull()
  })

  it('setCurrentApp stores a full app', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({
      id: 'bitcoin',
      title: 'Bitcoin Core',
      version: '25.0',
      icon: '/icons/btc.png',
      category: 'Finance',
      description: 'Bitcoin node',
      author: 'Satoshi',
      source: 'github',
      manifestUrl: 'https://example.com/manifest',
      url: 'https://example.com',
      repoUrl: 'https://github.com/bitcoin/bitcoin',
      s9pkUrl: '',
      dockerImage: 'bitcoin:25.0',
    })

    const app = getCurrentApp()
    expect(app).not.toBeNull()
    expect(app!.id).toBe('bitcoin')
    expect(app!.title).toBe('Bitcoin Core')
    expect(app!.version).toBe('25.0')
  })

  it('setCurrentApp with partial app fills defaults', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({ id: 'lnd' })

    const app = getCurrentApp()
    expect(app).not.toBeNull()
    expect(app!.id).toBe('lnd')
    expect(app!.title).toBe('')
    expect(app!.version).toBe('')
    expect(app!.icon).toBe('')
    expect(app!.dockerImage).toBe('')
  })

  it('manifestUrl falls back to s9pkUrl then url', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({ id: 'test', s9pkUrl: 'https://s9pk.example.com/app.s9pk' })

    const app = getCurrentApp()
    expect(app!.manifestUrl).toBe('https://s9pk.example.com/app.s9pk')
    expect(app!.url).toBe('https://s9pk.example.com/app.s9pk')
  })

  it('url falls back to s9pkUrl then manifestUrl', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({ id: 'test', manifestUrl: 'https://manifest.example.com' })

    const app = getCurrentApp()
    expect(app!.url).toBe('https://manifest.example.com')
  })

  it('clearCurrentApp sets app to null', () => {
    const { setCurrentApp, clearCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({ id: 'bitcoin' })
    expect(getCurrentApp()).not.toBeNull()
    clearCurrentApp()
    expect(getCurrentApp()).toBeNull()
  })

  it('shared state across multiple useMarketplaceApp calls', () => {
    const instance1 = useMarketplaceApp()
    const instance2 = useMarketplaceApp()

    instance1.setCurrentApp({ id: 'mempool', title: 'Mempool' })
    const app = instance2.getCurrentApp()
    expect(app!.id).toBe('mempool')
    expect(app!.title).toBe('Mempool')
  })

  it('handles description as object', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({
      id: 'test',
      description: { short: 'Short desc', long: 'Long description' },
    })
    const app = getCurrentApp()
    expect(app!.description).toEqual({ short: 'Short desc', long: 'Long description' })
  })

  it('preserves real screenshot metadata', () => {
    const { setCurrentApp, getCurrentApp } = useMarketplaceApp()
    setCurrentApp({
      id: 'test',
      screenshots: [
        '/screenshots/test-dashboard.png',
        { src: '/screenshots/test-settings.png', alt: 'Settings view' },
      ],
    })

    expect(getCurrentApp()!.screenshots).toEqual([
      '/screenshots/test-dashboard.png',
      { src: '/screenshots/test-settings.png', alt: 'Settings view' },
    ])
  })
})
