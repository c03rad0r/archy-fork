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
  it('admits the exact digest-version reported by the installed private demo', async () => {
    vi.resetModules()
    const digest = '4f28702e4f85368e4ad886f06d58b38f869c560c90ca40487bfaebe69090a870'
    const image = `localhost/node-demo:0.6.7-alpha-private@sha256:${digest}`
    const payload = { ...demo(), apps: { 'node-demo-v4v': {
      version: '0.6.7-alpha-private', digest: `sha256:${digest}`, image,
      manifest: { app: { id: 'node-demo-v4v', version: '0.6.7-alpha-private',
        container: { image }, metadata: { launch: { media_controls: 'archipelago-v1', requires_host_frame: true } } } },
    } } }
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => payload })))
    const { ensureNodeAppAvailable, appHasMediaBridge, appRequiresHostFrame } = await import('../curatedApps')
    expect(await ensureNodeAppAvailable('node-demo-v4v')).toBe(true)
    expect(appHasMediaBridge('node-demo-v4v', digest)).toBe(true)
    expect(appRequiresHostFrame('node-demo-v4v', digest)).toBe(true)
    expect(appHasMediaBridge('node-demo-v4v', 'a'.repeat(64))).toBe(false)
    expect(appHasMediaBridge('node-demo-v4v', digest.slice(0, 12))).toBe(false)
    payload.apps['node-demo-v4v'].digest = `sha256:${'b'.repeat(64)}`
    expect(appHasMediaBridge('node-demo-v4v', digest)).toBe(false)
    payload.apps['node-demo-v4v'].digest = `sha256:${digest}`
    payload.apps['node-demo-v4v'].manifest.app.container.image = `${image.slice(0, image.indexOf('@'))}@sha256:${'c'.repeat(64)}`
    expect(appHasMediaBridge('node-demo-v4v', digest)).toBe(false)
    payload.apps['node-demo-v4v'].manifest.app.container.image = image
    payload.apps['node-demo-v4v'].image = 'localhost/node-demo:mutable'
    expect(appHasMediaBridge('node-demo-v4v', digest)).toBe(false)
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
