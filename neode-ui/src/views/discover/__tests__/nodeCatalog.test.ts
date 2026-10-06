import { afterEach, describe, expect, it, vi } from 'vitest'

const demo = () => ({ scope: 'single-node-demo', expires_at: new Date(Date.now() + 60000).toISOString(), apps: {
  'node-demo-v4v': { version: '1', manifest: { app: { id: 'node-demo-v4v', name: 'V4V demo', ports: [{ host: 7475, auth: 'gated' }] } } },
}, storefront: { promotions: [{ id: 'node-demo-v4v', headline: 'Sovereign Music' }] } })

afterEach(() => { vi.unstubAllGlobals(); localStorage.clear() })
describe('node-scoped demo catalog', () => {
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
