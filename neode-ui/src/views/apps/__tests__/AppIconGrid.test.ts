import { describe, expect, it, vi, beforeEach } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useServerStore } from '@/stores/server'
import AppIconGrid from '../AppIconGrid.vue'

const mockWindowOpen = vi.fn()

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn().mockResolvedValue({ credentials: [] }),
  },
}))

vi.stubGlobal('open', mockWindowOpen)

function makePkg(id: string): PackageDataEntry {
  return {
    state: PackageState.Running,
    manifest: {
      id,
      title: id,
      version: '1.0.0',
      description: { short: '', long: '' },
      'release-notes': '',
      license: '',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': '',
      'donation-url': null,
      interfaces: { main: { ui: true } },
    } as unknown as PackageDataEntry['manifest'],
    'static-files': { license: '', instructions: '', icon: '' },
  }
}

describe('AppIconGrid', () => {
  let pinia: ReturnType<typeof createPinia>

  beforeEach(() => {
    vi.useRealTimers()
    pinia = createPinia()
    setActivePinia(pinia)
    vi.clearAllMocks()
    localStorage.clear()
    Object.defineProperty(window, 'innerWidth', {
      value: 1024,
      writable: true,
      configurable: true,
    })
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.11' },
      writable: true,
      configurable: true,
    })
  })

  it('opens LND companion UI in the app panel', async () => {
    const wrapper = mount(AppIconGrid, {
      props: { apps: [['lnd', makePkg('lnd')]] },
      global: {
        plugins: [pinia],
      },
    })

    await wrapper.get('.app-icon-item').trigger('click')
    await flushPromises()

    expect(mockWindowOpen).not.toHaveBeenCalled()
    expect(useAppLauncherStore(pinia).panelAppId).toBe('lnd')
  })

  it('shows File Browser credentials before launch even when backend returns no credentials', async () => {
    const wrapper = mount(AppIconGrid, {
      props: { apps: [['filebrowser', makePkg('filebrowser')]] },
      global: {
        plugins: [pinia],
        // The credential modal is <Teleport to="body">'d (so its full-screen
        // backdrop isn't clipped by the dashboard's transformed layout) —
        // stub it to render inline so wrapper.text() still sees it.
        stubs: { teleport: true },
      },
    })

    await wrapper.get('.app-icon-item').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('File Browser credentials')
    expect(wrapper.text()).toContain('Username')
    expect(wrapper.text()).toContain('admin')
    expect(useAppLauncherStore(pinia).panelAppId).toBeNull()
  })

  it('opens unresolved new-tab apps externally on mobile', async () => {
    Object.defineProperty(window, 'innerWidth', {
      value: 390,
      writable: true,
      configurable: true,
    })

    const wrapper = mount(AppIconGrid, {
      props: { apps: [['gitea', makePkg('gitea')]] },
      global: {
        plugins: [pinia],
      },
    })

    await wrapper.get('.app-icon-item').trigger('click')
    await flushPromises()

    expect(mockWindowOpen).toHaveBeenCalledWith(
      'http://192.0.2.11:3001',
      '_blank',
      'noopener,noreferrer',
    )
    expect(useAppLauncherStore(pinia).panelAppId).toBeNull()
  })

  it('shows backend uninstall stage while an app is removing', () => {
    const pkg = makePkg('indeedhub')
    pkg.state = PackageState.Removing
    pkg['uninstall-stage'] = 'Stopping containers (2/7)'
    useServerStore(pinia).uninstallingApps.add('indeedhub')

    const wrapper = mount(AppIconGrid, {
      props: { apps: [['indeedhub', pkg]] },
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).toContain('Stopping containers (2/7)')
  })

  it('supports legacy underscore uninstall stage data', () => {
    const pkg = makePkg('indeedhub')
    pkg.state = PackageState.Removing
    ;(pkg as PackageDataEntry & { uninstall_stage?: string }).uninstall_stage = 'Removing app data'
    useServerStore(pinia).uninstallingApps.add('indeedhub')

    const wrapper = mount(AppIconGrid, {
      props: { apps: [['indeedhub', pkg]] },
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).toContain('Removing app data')
  })

  it('opens app details on long press without launching the app', async () => {
    vi.useFakeTimers()
    const wrapper = mount(AppIconGrid, {
      props: { apps: [['lnd', makePkg('lnd')]] },
      global: {
        plugins: [pinia],
      },
    })

    const icon = wrapper.get('.app-icon-item')
    await icon.trigger('pointerdown')
    vi.advanceTimersByTime(550)
    await icon.trigger('click')
    await flushPromises()

    expect(wrapper.emitted('goToApp')).toEqual([['lnd']])
    expect(useAppLauncherStore(pinia).panelAppId).toBeNull()
  })

  it('opens app details from the keyboard options shortcut', async () => {
    const wrapper = mount(AppIconGrid, {
      props: { apps: [['lnd', makePkg('lnd')]] },
      global: {
        plugins: [pinia],
      },
    })

    await wrapper.get('.app-icon-item').trigger('keydown.space')

    expect(wrapper.emitted('goToApp')).toEqual([['lnd']])
  })
})
