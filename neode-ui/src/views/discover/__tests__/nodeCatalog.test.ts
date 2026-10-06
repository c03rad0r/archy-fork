import { afterEach, describe, expect, it, vi } from 'vitest'

const demo = () => ({ scope: 'single-node-demo', expires_at: new Date(Date.now() + 60000).toISOString(), apps: {
  'node-demo-v4v': { version: '1', manifest: { app: { id: 'node-demo-v4v', name: 'V4V demo', ports: [{ host: 7475, auth: 'gated' }] } } },
}, storefront: { promotions: [{ id: 'node-demo-v4v', headline: 'Sovereign Music' }] } })

afterEach(() => { vi.unstubAllGlobals(); localStorage.clear() })
describe('node-scoped demo catalog', () => {
  it('resolves launch policy before a slow public storefront and deduplicates requests', async () => {
    vi.resetModules()
    let release!: (value: unknown) => void
    const pending = new Promise(resolve => { release = resolve })
    const fetcher = vi.fn((_url: string) => pending)
    vi.stubGlobal('fetch', fetcher)
    const { ensureNodeAppAvailable, appHasMediaBridge } = await import('../curatedApps')
    const first = ensureNodeAppAvailable('node-demo-v4v')
    const second = ensureNodeAppAvailable('node-demo-v4v')
    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(fetcher.mock.calls[0]?.[0]).toBe('/api/node-app-catalog')
    expect(appHasMediaBridge('node-demo-v4v')).toBe(false)
    const payload = demo()
    Object.assign(payload.apps['node-demo-v4v'].manifest.app, { metadata: { launch: { media_controls: 'archipelago-v1' } } })
    release({ ok: true, json: async () => payload })
    expect(await first).toBe(true)
    expect(await second).toBe(true)
    expect(appHasMediaBridge('node-demo-v4v')).toBe(true)
    expect(await ensureNodeAppAvailable('node-demo-v4v')).toBe(true)
    expect(fetcher).toHaveBeenCalledTimes(1)
  })
  it('fails closed for unavailable and expired launch policy', async () => {
    vi.resetModules()
    const fetcher = vi.fn(async () => ({ ok: true, json: async () => ({ ...demo(), expires_at: '2000-01-01T00:00:00Z' }) }))
    vi.stubGlobal('fetch', fetcher)
    const { ensureNodeAppAvailable, appHasMediaBridge } = await import('../curatedApps')
    expect(await ensureNodeAppAvailable('node-demo-v4v')).toBe(false)
    expect(appHasMediaBridge('node-demo-v4v')).toBe(false)
    fetcher.mockRejectedValueOnce(new Error('offline'))
    expect(await ensureNodeAppAvailable('node-demo-v4v')).toBe(false)
  })

  it.each([{}, [null]])('ignores malformed optional promotions: %j', async promotions => {
    vi.resetModules()
    vi.stubGlobal('fetch', vi.fn(async (url: string) => {
      if (url === '/api/node-app-catalog') return { ok: true, json: async () => ({ ...demo(), storefront: { promotions } }) }
      if (url === '/api/app-catalog') return { ok: true, json: async () => ({ apps: { mempool: { version: '3' } } }) }
      return { ok: false }
    }))
    const { fetchAppCatalog } = await import('../curatedApps')
    const catalog = await fetchAppCatalog()
    expect(catalog?.apps.map(app => app.id)).toContain('node-demo-v4v')
    expect(catalog?.storefront?.promotions).toEqual([])
  })
  it('adds only the verified node response and never persists it into shared fallback', async () => {
    vi.resetModules()
    let available = true
    vi.stubGlobal('fetch', vi.fn(async (url: string) => {
      if (url === '/api/node-app-catalog') return { ok: available, json: async () => demo() }
      if (url === '/api/app-catalog') return { ok: true, json: async () => ({ apps: { mempool: { version: '3' } } }) }
      return { ok: false }
    }))
    const { fetchAppCatalog, portIsGateFronted } = await import('../curatedApps')
    const first = await fetchAppCatalog()
    expect(first?.apps.map(app => app.id)).toEqual(['mempool', 'node-demo-v4v'])
    expect(first?.storefront?.promotions[0]?.headline).toBe('Sovereign Music')
    expect(localStorage.getItem('archy_catalog')).not.toContain('node-demo-v4v')
    expect(portIsGateFronted('node-demo-v4v', 7475)).toBe(true)
    available = false
    const second = await fetchAppCatalog()
    expect(second?.apps.map(app => app.id)).toEqual(['mempool'])
    expect(second?.storefront?.promotions ?? []).toHaveLength(0)
    expect(portIsGateFronted('node-demo-v4v', 7475)).toBe(false)
  })
})
