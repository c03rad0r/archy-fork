import { mount } from '@vue/test-utils'
import { nextTick, reactive } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import DashboardMobileNav from '../DashboardMobileNav.vue'

const state = vi.hoisted(() => ({ launcher: null as any }))
vi.mock('@/stores/appLauncher', () => ({ useAppLauncherStore: () => state.launcher }))
vi.mock('@/stores/uiMode', () => ({ useUIModeStore: () => ({ isEasy: false, isChat: false }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('vue-router', () => ({
  useRoute: () => ({ path: '/dashboard/discover', name: 'discover', query: {} }),
  useRouter: () => ({ push: vi.fn() }),
  RouterLink: { template: '<a><slot /></a>' },
}))

describe('mobile navigation clearance across app sessions', () => {
  let height = 72
  const observers: Array<{ callback: () => void; disconnect: ReturnType<typeof vi.fn> }> = []
  beforeEach(() => {
    vi.useFakeTimers()
    state.launcher = reactive({ panelAppId: null })
    height = 72
    observers.length = 0
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(() => height)
    vi.stubGlobal('ResizeObserver', class {
      disconnect = vi.fn()
      observe = vi.fn()
      constructor(callback: () => void) { observers.push({ callback, disconnect: this.disconnect }) }
    })
  })
  afterEach(() => {
    vi.clearAllTimers()
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
    document.documentElement.style.removeProperty('--mobile-tab-bar-height')
  })
  const clearance = () => document.documentElement.style.getPropertyValue('--mobile-tab-bar-height')

  it('restores measured space without a route change after closing an app', async () => {
    const wrapper = mount(DashboardMobileNav, { props: { showZoomIn: false } })
    await nextTick()
    expect(clearance()).toBe('72px')
    state.launcher.panelAppId = 'node-demo-v4v'
    await nextTick()
    expect(wrapper.find('[data-mobile-tab-bar]').exists()).toBe(false)
    expect(clearance()).toBe('')
    expect(observers[0].disconnect).toHaveBeenCalledOnce()
    height = 88
    state.launcher.panelAppId = null
    await nextTick()
    expect(wrapper.find('[data-mobile-tab-bar]').exists()).toBe(true)
    expect(clearance()).toBe('88px')
    height = 104
    observers.at(-1)!.callback()
    expect(clearance()).toBe('104px')
    wrapper.unmount()
    expect(observers.at(-1)!.disconnect).toHaveBeenCalledOnce()
  })

  it('does not reserve a visible navigation height on desktop', async () => {
    height = 0
    const wrapper = mount(DashboardMobileNav, { props: { showZoomIn: false } })
    await nextTick()
    expect(clearance()).toBe('')
    wrapper.unmount()
  })
})
