import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Marketplace from '../Marketplace.vue'

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
  useToast: () => ({ success: vi.fn(), error: toastErrorMock, info: toastInfoMock }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    marketplaceDiscover: vi.fn().mockResolvedValue({ apps: [] }),
  },
}))

describe('Marketplace tracer tab: background refresh failure (D-07)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', vi.fn(() => ({ observe: vi.fn(), disconnect: vi.fn() })))
    routerPushMock.mockClear()
    toastErrorMock.mockClear()
    toastInfoMock.mockClear()
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
})
