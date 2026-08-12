import { describe, it, expect, vi, beforeEach } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import PWAInstallPrompt from '../PWAInstallPrompt.vue'

describe('PWAInstallPrompt', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    sessionStorage.clear()
    // Mock matchMedia to return non-standalone
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    })
  })

  it('renders without errors', () => {
    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })
    expect(wrapper.exists()).toBe(true)
  })

  it('does not show prompt initially', () => {
    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })
    expect(wrapper.text()).not.toContain('Install Archipelago')
  })

  it('shows prompt after beforeinstallprompt event', async () => {
    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })

    // Fire the beforeinstallprompt event
    const event = new Event('beforeinstallprompt')
    Object.defineProperty(event, 'preventDefault', { value: vi.fn() })
    window.dispatchEvent(event)

    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('Install Archipelago')
  })

  it('hides prompt when dismissed', async () => {
    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })

    // Show prompt
    const event = new Event('beforeinstallprompt')
    Object.defineProperty(event, 'preventDefault', { value: vi.fn() })
    window.dispatchEvent(event)
    await wrapper.vm.$nextTick()

    // Click dismiss button
    const dismissBtn = wrapper.findAll('button').find(b => b.text().includes('Not now'))
    expect(dismissBtn).toBeDefined()
    await dismissBtn!.trigger('click')
    expect(sessionStorage.getItem('archipelago_pwa_install_dismissed')).toBe('1')
  })

  it('does not show if already dismissed this session', async () => {
    sessionStorage.setItem('archipelago_pwa_install_dismissed', '1')
    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })

    // Fire beforeinstallprompt — should not show
    const event = new Event('beforeinstallprompt')
    Object.defineProperty(event, 'preventDefault', { value: vi.fn() })
    window.dispatchEvent(event)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('Install Archipelago')
  })

  it('does not show in standalone mode', async () => {
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockReturnValue({ matches: true }),
    })

    const wrapper = shallowMount(PWAInstallPrompt, {
      global: { stubs: { Teleport: true, Transition: true } },
    })

    const event = new Event('beforeinstallprompt')
    Object.defineProperty(event, 'preventDefault', { value: vi.fn() })
    window.dispatchEvent(event)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).not.toContain('Install Archipelago')
  })
})
