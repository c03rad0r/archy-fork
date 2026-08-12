import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, nextTick } from 'vue'
import { useMobileBackButton } from '../useMobileBackButton'

// Helper component that uses the composable
const TestComponent = defineComponent({
  setup() {
    return useMobileBackButton()
  },
  template: '<div>{{ bottomPosition }}</div>',
})

describe('useMobileBackButton', () => {
  let wrapper: ReturnType<typeof mount>

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    wrapper?.unmount()
    vi.useRealTimers()
  })

  it('returns bottomPosition, bottomClass, and tabBarHeight', () => {
    wrapper = mount(TestComponent)
    const vm = wrapper.vm as unknown as {
      bottomPosition: string
      bottomClass: string
      tabBarHeight: number
    }

    expect(typeof vm.bottomPosition).toBe('string')
    expect(typeof vm.bottomClass).toBe('string')
    expect(typeof vm.tabBarHeight).toBe('number')
  })

  it('defaults tabBarHeight to 72', () => {
    wrapper = mount(TestComponent)
    const vm = wrapper.vm as unknown as { tabBarHeight: number }
    expect(vm.tabBarHeight).toBe(72)
  })

  it('computes bottomPosition as tabBarHeight + 8', () => {
    wrapper = mount(TestComponent)
    const vm = wrapper.vm as unknown as {
      bottomPosition: string
      tabBarHeight: number
    }
    expect(vm.bottomPosition).toBe('80px') // 72 + 8
  })

  it('computes bottomClass with Tailwind arbitrary value', () => {
    wrapper = mount(TestComponent)
    const vm = wrapper.vm as unknown as { bottomClass: string }
    expect(vm.bottomClass).toBe('bottom-[80px]')
  })

  it('reads tabBar element if present', async () => {
    // Create mock tab bar element
    const tabBar = document.createElement('div')
    tabBar.setAttribute('data-mobile-tab-bar', '')
    Object.defineProperty(tabBar, 'offsetHeight', { value: 56 })
    document.body.appendChild(tabBar)

    wrapper = mount(TestComponent)
    await nextTick()

    const vm = wrapper.vm as unknown as { tabBarHeight: number }
    expect(vm.tabBarHeight).toBe(56)

    document.body.removeChild(tabBar)
  })

  it('falls back to CSS variable when no tab bar element', async () => {
    document.documentElement.style.setProperty('--mobile-tab-bar-height', '64')

    wrapper = mount(TestComponent)
    await nextTick()

    const vm = wrapper.vm as unknown as { tabBarHeight: number }
    expect(vm.tabBarHeight).toBe(64)

    document.documentElement.style.removeProperty('--mobile-tab-bar-height')
  })

  it('keeps default when no tab bar or CSS var', async () => {
    wrapper = mount(TestComponent)
    await nextTick()

    const vm = wrapper.vm as unknown as { tabBarHeight: number }
    // Should keep the default of 72
    expect(vm.tabBarHeight).toBe(72)
  })

  it('cleans up observers on unmount', () => {
    wrapper = mount(TestComponent)
    const removeEventSpy = vi.spyOn(window, 'removeEventListener')
    wrapper.unmount()
    expect(removeEventSpy).toHaveBeenCalled()
    removeEventSpy.mockRestore()
  })

  it('updates on window resize', async () => {
    const tabBar = document.createElement('div')
    tabBar.setAttribute('data-mobile-tab-bar', '')
    Object.defineProperty(tabBar, 'offsetHeight', {
      value: 48,
      writable: true,
      configurable: true,
    })
    document.body.appendChild(tabBar)

    wrapper = mount(TestComponent)
    await nextTick()

    // Trigger resize
    window.dispatchEvent(new Event('resize'))
    await nextTick()

    const vm = wrapper.vm as unknown as { tabBarHeight: number }
    expect(vm.tabBarHeight).toBe(48)

    document.body.removeChild(tabBar)
  })
})
