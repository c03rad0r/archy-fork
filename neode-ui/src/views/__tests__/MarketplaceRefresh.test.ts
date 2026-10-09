import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Marketplace from '../Marketplace.vue'
import { rpcClient } from '@/api/rpc-client'
import InstallVersionModal from '@/components/InstallVersionModal.vue'
import MarketplaceAppCard from '../marketplace/MarketplaceAppCard.vue'

// Mirrors the CloudPeersRefresh.test.ts pattern (in-repo convention for
// mounting a view directly with its heavier deps mocked at the module
// boundary) — kept in its own file rather than keepAliveTabs.test.ts because
// vi.mock('vue-router', ...) is hoisted file-wide and would otherwise
// clobber that file's real createRouter/createMemoryHistory imports used by
// the DashboardRouterView tests (Rule 3 auto-fix).
const routerPushMock = vi.fn()
const toastErrorMock = vi.fn()
const toastInfoMock = vi.fn()

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: routerPushMock, replace: vi.fn().mockResolvedValue(undefined) }),
  useRoute: () => ({ query: {} }),
  RouterLink: { name: 'RouterLink', props: ['to'], template: '<a><slot /></a>' },
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ data: {}, hasLoadedInitialData: true }),
}))

vi.mock('@/stores/server', () => ({
  useServerStore: () => ({
    installingApps: new Map(),
    setInstallProgress: vi.fn(),
    clearInstallProgress: vi.fn(),
  }),
}))

vi.mock('@/stores/appLauncher', () => ({
  useAppLauncherStore: () => ({ openSession: vi.fn() }),
}))

vi.mock('@/composables/useMarketplaceApp', () => ({
  useMarketplaceApp: () => ({ setCurrentApp: vi.fn() }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ success: vi.fn(), error: toastErrorMock, info: toastInfoMock, action: vi.fn() }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    marketplaceDiscover: vi.fn().mockResolvedValue({ apps: [] }),
    getPackageVersions: vi.fn(),
  },
}))

vi.mock('../discover/curatedApps', () => ({
  fetchAppCatalog: vi.fn().mockResolvedValue({
    apps: ['bitcoin-core', 'bitcoin-knots'].map(id => ({
      id, title: id, version: '29.0', description: 'Bitcoin node',
      dockerImage: `registry.example/${id}:29.0`, source: 'community',
    })),
  }),
}))

describe('Marketplace tracer tab: background refresh failure (D-07)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', class {
      observe = vi.fn()
      disconnect = vi.fn()
    })
    routerPushMock.mockClear()
    toastErrorMock.mockClear()
    toastInfoMock.mockClear()
    vi.mocked(rpcClient.call).mockReset()
    vi.mocked(rpcClient.getPackageVersions).mockResolvedValue({
      supportsVersions: true, default: '29.0', bitcoinPrune: false,
      versions: [{ version: '29.0', default: true, deprecated: false, eol: null }],
    } as Awaited<ReturnType<typeof rpcClient.getPackageVersions>>)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('keeps the curated catalog on screen and raises no toast when the prune-status background refresh rejects', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ blockchain_info: { pruned: false } }),
    }))

    const wrapper = mount(Marketplace, { global: { plugins: [createPinia()] } })
    await flushPromises()

    expect(wrapper.findAll('.glass-card').length).toBeGreaterThan(0)

    vi.mocked(fetch).mockRejectedValueOnce(new Error('node unreachable'))
    await (wrapper.vm as unknown as { loadBitcoinPruneStatus: () => Promise<void> }).loadBitcoinPruneStatus()
    await flushPromises()

    // Prior catalog content is still rendered — a background refresh failure
    // on an unrelated resource never wipes the view.
    expect(wrapper.findAll('.glass-card').length).toBeGreaterThan(0)
    expect(toastErrorMock).not.toHaveBeenCalled()
    expect(toastInfoMock).not.toHaveBeenCalled()

    wrapper.unmount()
  })

  it.each(['bitcoin-core', 'bitcoin-knots'])('requires the version modal before installing %s and forwards pruning', async (id) => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ blockchain_info: { pruned: false } }) }))
    const wrapper = mount(Marketplace, { global: { plugins: [createPinia()], stubs: { Teleport: true } } })
    await flushPromises()
    const card = wrapper.findAllComponents(MarketplaceAppCard).find(c => c.props('app').id === id)!
    expect(card.exists()).toBe(true)
    card.vm.$emit('install', card.props('app'))
    await flushPromises()
    const installs = () => vi.mocked(rpcClient.call).mock.calls.filter(([request]) => request.method === 'package.install')
    expect(installs()).toHaveLength(0)
    const modal = wrapper.findComponent(InstallVersionModal)
    expect(modal.props('show')).toBe(true)
    await modal.get('input[type="checkbox"]').setValue(true)
    await modal.get('button.glass-button-warning').trigger('click')
    await flushPromises()
    expect(installs()).toHaveLength(1)
    expect(installs()[0]?.[0].params).toMatchObject({ id, version: '29.0', prune: true })
    expect(modal.props('show')).toBe(false)
    wrapper.unmount()
  })

  it('cancels Bitcoin selection without sending an installation request', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ blockchain_info: { pruned: false } }) }))
    const wrapper = mount(Marketplace, { global: { plugins: [createPinia()], stubs: { Teleport: true } } })
    await flushPromises()
    const card = wrapper.findAllComponents(MarketplaceAppCard).find(c => c.props('app').id === 'bitcoin-knots')!
    card.vm.$emit('install', card.props('app'))
    await flushPromises()
    wrapper.findComponent(InstallVersionModal).vm.$emit('close')
    await flushPromises()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([request]) => request.method === 'package.install')).toHaveLength(0)
    wrapper.unmount()
  })
})
