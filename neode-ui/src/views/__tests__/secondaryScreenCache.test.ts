// Per-item keyed useCachedResource conversions for secondary screens (D-04,
// plan 02-03). Pins: instant repeat-open from cache, no new RPC inside the
// TTL, per-item key isolation (rendered content, not just call counts),
// TTL-lapse revalidation, and keep-last-value on a rejected background
// refresh (D-07).

import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, type Pinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import AppDetails from '../AppDetails.vue'
import MarketplaceAppDetails from '../MarketplaceAppDetails.vue'
import OpenWrtGateway from '../server/OpenWrtGateway.vue'
import { rpcClient } from '@/api/rpc-client'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    getPackageVersions: vi.fn(),
  },
}))

let currentRouteParams: Record<string, string> = {}
let currentRouteQuery: Record<string, string> = {}

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn(() => Promise.resolve()), replace: vi.fn() }),
  useRoute: () => ({ params: currentRouteParams, query: currentRouteQuery }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

let currentPackages: Record<string, unknown> = {}

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    packages: currentPackages,
    startPackage: vi.fn().mockResolvedValue(undefined),
    stopPackage: vi.fn().mockResolvedValue(undefined),
    restartPackage: vi.fn().mockResolvedValue(undefined),
    updatePackage: vi.fn().mockResolvedValue(undefined),
    uninstallPackage: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('@/composables/useMarketplaceApp', () => ({
  useMarketplaceApp: () => ({ getCurrentApp: () => currentMarketplaceApp }),
}))

let currentMarketplaceApp: Record<string, unknown> | null = null

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function makePkg(overrides: Record<string, unknown> = {}) {
  return {
    manifest: { id: 'pkg', version: '1.0.0', title: 'Test App' },
    state: 'running',
    health: 'healthy',
    installed: null,
    ...overrides,
  }
}

// A single Pinia instance is shared across the unmount/remount pairs within
// one test — matching production, where the resources store's in-memory
// entries Map is a singleton that survives navigating away from and back to
// a secondary screen (only a full page reload, or clearAll() on logout,
// resets it). Creating a fresh Pinia per mount would wipe persist:false
// entries (credentials) between "visits" and falsely fail the cache-hit
// assertions below.
function mountAppDetails(id: string, packages: Record<string, unknown>, pinia: Pinia) {
  currentRouteParams = { id }
  currentRouteQuery = {}
  currentPackages = packages
  return mount(AppDetails, {
    global: {
      plugins: [pinia],
      stubs: { AppHeroSection: true, AppContentSection: true, LndSeedBackup: true, AppsUninstallModal: true },
      mocks: { $ver: (v: string) => v },
    },
  })
}

describe('AppDetails.vue — per-item cached resources (bitcoin sync + credentials)', () => {
  beforeEach(() => {
    sessionStorage.clear()
    vi.mocked(rpcClient.getPackageVersions).mockResolvedValue({
      id: 'x', supportsVersions: false, default: null, installedVersion: null,
      pinnedVersion: null, autoUpdate: false, versions: [],
    })
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  function credentialsHandler(byId: Record<string, { label: string; value: string }>) {
    return vi.fn((req: { method: string; params?: { app_id?: string } }) => {
      if (req.method === 'package.credentials') {
        const appId = req.params?.app_id ?? ''
        const cred = byId[appId]
        return Promise.resolve(cred ? { credentials: [cred] } : { credentials: [] })
      }
      if (req.method === 'bitcoin.getinfo') return Promise.resolve({ block_height: 0, sync_progress: 0 })
      return Promise.resolve({})
    })
  }

  it('repeat mount for the same app id inside the TTL issues exactly one credentials fetch total', async () => {
    const pinia = createPinia()
    const handler = credentialsHandler({ 'app-alpha': { label: 'Password', value: 'alpha-secret' } })
    vi.mocked(rpcClient.call).mockImplementation(handler as never)

    const w1 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    w1.unmount()

    const w2 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    w2.unmount()

    const credCalls = handler.mock.calls.filter((c) => (c[0] as { method: string }).method === 'package.credentials')
    expect(credCalls.length).toBe(1)
  })

  it('mounting for app id alpha then beta issues two credentials fetches and never renders alpha data for beta', async () => {
    const pinia = createPinia()
    const handler = credentialsHandler({
      'app-alpha': { label: 'Password', value: 'alpha-secret' },
      'app-beta': { label: 'Password', value: 'beta-secret' },
    })
    vi.mocked(rpcClient.call).mockImplementation(handler as never)

    const w1 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    expect(w1.text()).toContain('alpha-secret')
    w1.unmount()

    const w2 = mountAppDetails('app-beta', { 'app-beta': makePkg() }, pinia)
    await flushPromises()
    expect(w2.text()).toContain('beta-secret')
    expect(w2.text()).not.toContain('alpha-secret')
    w2.unmount()

    const credCalls = handler.mock.calls.filter((c) => (c[0] as { method: string }).method === 'package.credentials')
    expect(credCalls.length).toBe(2)
  })

  it('a repeat mount after the TTL lapses shows cached data on the first frame and refetches exactly once more', async () => {
    const pinia = createPinia()
    vi.useFakeTimers({ shouldAdvanceTime: true })
    let credentialCallCount = 0

    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      if (req.method === 'package.credentials') {
        credentialCallCount++
        return Promise.resolve({ credentials: [{ label: 'Password', value: 'alpha-secret' }] })
      }
      if (req.method === 'bitcoin.getinfo') return Promise.resolve({ block_height: 0, sync_progress: 0 })
      return Promise.resolve({})
    })
    const w1 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    w1.unmount()

    vi.advanceTimersByTime(31_000) // past the 30s TTL

    // A deferred second fetch lets us observe the cached value on-screen
    // before the TTL-triggered revalidate resolves.
    const stalled = deferred<{ credentials: { label: string; value: string }[] }>()
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      if (req.method === 'package.credentials') {
        credentialCallCount++
        return stalled.promise
      }
      if (req.method === 'bitcoin.getinfo') return Promise.resolve({ block_height: 0, sync_progress: 0 })
      return Promise.resolve({})
    })

    const w2 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await w2.vm.$nextTick()
    // Cached value renders before the (TTL-triggered) revalidate resolves.
    expect(w2.text()).toContain('alpha-secret')

    stalled.resolve({ credentials: [{ label: 'Password', value: 'alpha-secret' }] })
    await flushPromises()
    w2.unmount()

    expect(credentialCallCount).toBe(2)
  })

  it('a rejected background refresh keeps the previously rendered credentials on screen', async () => {
    const pinia = createPinia()
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const good = credentialsHandler({ 'app-alpha': { label: 'Password', value: 'alpha-secret' } })
    vi.mocked(rpcClient.call).mockImplementation(good as never)

    const w1 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    expect(w1.text()).toContain('alpha-secret')
    w1.unmount()

    vi.advanceTimersByTime(31_000)
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      if (req.method === 'package.credentials') return Promise.reject(new Error('offline'))
      if (req.method === 'bitcoin.getinfo') return Promise.resolve({ block_height: 0, sync_progress: 0 })
      return Promise.resolve({})
    })

    const w2 = mountAppDetails('app-alpha', { 'app-alpha': makePkg() }, pinia)
    await flushPromises()
    // Keep-last-value (D-07): the in-memory cache still shows the previous
    // credential even though the revalidate failed.
    expect(w2.text()).toContain('alpha-secret')
    w2.unmount()
  })

  it('bitcoin-sync and credentials fetchers for a single mount are issued concurrently, not one after another', async () => {
    const pinia = createPinia()
    const bitcoinDeferred = deferred<{ block_height: number; sync_progress: number }>()
    const credsDeferred = deferred<{ credentials: { label: string; value: string }[] }>()
    const calledMethods: string[] = []
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      calledMethods.push(req.method)
      if (req.method === 'bitcoin.getinfo') return bitcoinDeferred.promise
      if (req.method === 'package.credentials') return credsDeferred.promise
      return Promise.resolve({})
    })

    // 'mempool-electrs' is in BITCOIN_DEPENDENT_APPS and maps to itself via
    // ROUTE_TO_PACKAGE_KEY, so this id exercises the bitcoin-sync resource.
    const w = mountAppDetails('mempool-electrs', { 'mempool-electrs': makePkg() }, pinia)
    await w.vm.$nextTick()

    // Both fetchers must already be in flight before either resolves —
    // proof neither loader awaited the other.
    expect(calledMethods).toContain('bitcoin.getinfo')
    expect(calledMethods).toContain('package.credentials')

    bitcoinDeferred.resolve({ block_height: 800000, sync_progress: 1 })
    credsDeferred.resolve({ credentials: [] })
    await flushPromises()
    w.unmount()
  })
})

describe('MarketplaceAppDetails.vue — per-item cached catalog versions', () => {
  beforeEach(() => {
    sessionStorage.clear()
    currentRouteQuery = {}
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  function mountMarketplaceDetails(id: string, app: Record<string, unknown> | null) {
    currentRouteParams = { id }
    currentRouteQuery = {}
    currentMarketplaceApp = app
    return mount(MarketplaceAppDetails, {
      global: {
        plugins: [createPinia()],
        mocks: { $ver: (v: string) => v },
      },
    })
  }

  function makeApp(id: string, overrides: Record<string, unknown> = {}) {
    return {
      id,
      title: `App ${id}`,
      description: 'desc',
      version: '1.0.0',
      dockerImage: `registry/${id}:latest`,
      screenshots: [],
      ...overrides,
    }
  }

  it('repeat mount for the same marketplace app id inside the TTL issues exactly one package.versions fetch', async () => {
    const calls: string[] = []
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      calls.push(req.method)
      if (req.method === 'package.versions') {
        return Promise.resolve({ id: 'demo-app', supportsVersions: false, default: null, installedVersion: null, pinnedVersion: null, autoUpdate: false, versions: [] })
      }
      return Promise.resolve({})
    })

    const w1 = mountMarketplaceDetails('demo-app', makeApp('demo-app'))
    await flushPromises()
    w1.unmount()

    const w2 = mountMarketplaceDetails('demo-app', makeApp('demo-app'))
    await flushPromises()
    w2.unmount()

    expect(calls.filter((m) => m === 'package.versions').length).toBe(1)
  })

  it('mounting for marketplace app id alpha then beta each issue their own versions fetch (distinct per-item keys)', async () => {
    const requestedIds: string[] = []
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string; params?: { id?: string } }) => {
      if (req.method === 'package.versions') {
        requestedIds.push(req.params?.id ?? '')
        return Promise.resolve({ id: req.params?.id, supportsVersions: false, default: null, installedVersion: null, pinnedVersion: null, autoUpdate: false, versions: [] })
      }
      return Promise.resolve({})
    })

    const w1 = mountMarketplaceDetails('mkt-alpha', makeApp('mkt-alpha'))
    await flushPromises()
    w1.unmount()

    const w2 = mountMarketplaceDetails('mkt-beta', makeApp('mkt-beta'))
    await flushPromises()
    w2.unmount()

    expect(requestedIds).toEqual(['mkt-alpha', 'mkt-beta'])
  })
})

describe('OpenWrtGateway.vue — cached router status (no item id: one gateway per node)', () => {
  beforeEach(() => {
    sessionStorage.clear()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  function makeStatus(hostname: string) {
    return {
      host: '192.168.1.1',
      hostname,
      uptime_secs: 100,
      release: {},
      tollgate: { installed: false },
      wifi_interfaces: [],
      wan: { configured: false, ssid: '', assoc_ssid: '', encryption: '', ip: '', internet: false, radio0_disabled: false, sta_iface: '', sta_state: '' },
    }
  }

  function mountGateway(pinia: Pinia) {
    return mount(OpenWrtGateway, {
      global: { plugins: [pinia] },
    })
  }

  it('repeat mount inside the TTL issues exactly one openwrt.get-status fetch total', async () => {
    const pinia = createPinia()
    let calls = 0
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      if (req.method === 'openwrt.get-status') {
        calls++
        return Promise.resolve(makeStatus('router-1'))
      }
      return Promise.resolve({})
    })

    const w1 = mountGateway(pinia)
    await flushPromises()
    expect(w1.text()).toContain('router-1')
    w1.unmount()

    const w2 = mountGateway(pinia)
    await flushPromises()
    expect(w2.text()).toContain('router-1')
    w2.unmount()

    expect(calls).toBe(1)
  })

  it('a repeat mount after the TTL lapses shows cached data on the first frame and refetches exactly once more', async () => {
    const pinia = createPinia()
    vi.useFakeTimers({ shouldAdvanceTime: true })
    let calls = 0
    vi.mocked(rpcClient.call).mockImplementation((req: { method: string }) => {
      if (req.method === 'openwrt.get-status') {
        calls++
        return Promise.resolve(makeStatus('router-1'))
      }
      return Promise.resolve({})
    })

    const w1 = mountGateway(pinia)
    await flushPromises()
    w1.unmount()

    vi.advanceTimersByTime(31_000)

    const w2 = mountGateway(pinia)
    await w2.vm.$nextTick()
    expect(w2.text()).toContain('router-1')

    await flushPromises()
    w2.unmount()

    expect(calls).toBe(2)
  })
})
