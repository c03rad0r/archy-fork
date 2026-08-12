import { flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter, type RouteRecordRaw } from 'vue-router'
import { defineComponent, h, onActivated, onMounted } from 'vue'
import { beforeEach, describe, expect, it } from 'vitest'
import DashboardRouterView from '../DashboardRouterView.vue'
import { shouldKeepAlive } from '../keepAliveRoutes'
import RefreshIndicator from '@/components/RefreshIndicator.vue'

let mountCount = 0
let activateCount = 0
let detailMountCount = 0

const KeptAliveStub = defineComponent({
  name: 'KeptAliveStub',
  setup() {
    onMounted(() => { mountCount++ })
    onActivated(() => { activateCount++ })
    return () => h('div', { class: 'kept-alive-stub' }, 'kept-alive')
  },
})

const DetailStub = defineComponent({
  name: 'DetailStub',
  setup() {
    onMounted(() => { detailMountCount++ })
    return () => h('div', { class: 'detail-stub' }, 'detail')
  },
})

const OtherStub = defineComponent({
  name: 'OtherStub',
  setup() {
    return () => h('div', { class: 'other-stub' }, 'other')
  },
})

function makeRouter() {
  const routes: RouteRecordRaw[] = [
    { path: '/dashboard/marketplace', component: KeptAliveStub },
    { path: '/dashboard/marketplace/:id', component: DetailStub },
    { path: '/dashboard/other', component: OtherStub },
  ]
  return createRouter({ history: createMemoryHistory(), routes })
}

describe('DashboardRouterView keep-alive behavior', () => {
  beforeEach(() => {
    mountCount = 0
    activateCount = 0
    detailMountCount = 0
  })

  it('keeps the instance alive across a round trip for an included path', async () => {
    const router = makeRouter()
    router.push('/dashboard/marketplace')
    await router.isReady()

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: { plugins: [router] },
    })
    await flushPromises()

    expect(mountCount).toBe(1)

    await router.push('/dashboard/other')
    await flushPromises()
    await router.push('/dashboard/marketplace')
    await flushPromises()

    expect(mountCount).toBe(1) // never remounted
    expect(activateCount).toBe(2) // initial mount + the round-trip reactivation

    wrapper.unmount()
  })

  it('remounts a detail path on every visit (not instance-cached)', async () => {
    const router = makeRouter()
    router.push('/dashboard/marketplace/abc')
    await router.isReady()

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: { plugins: [router] },
    })
    await flushPromises()

    expect(detailMountCount).toBe(1)

    await router.push('/dashboard/other')
    await flushPromises()
    await router.push('/dashboard/marketplace/abc')
    await flushPromises()

    expect(detailMountCount).toBe(2)

    wrapper.unmount()
  })

  it('shouldKeepAlive returns false for a detail path whose prefix matches an included path', () => {
    expect(shouldKeepAlive({ path: '/dashboard/marketplace' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/marketplace/abc' })).toBe(false)
  })

  // Visual contract (Task 3 checkpoint regression): the padded default-shape
  // wrapper must render INSIDE `div.view-wrapper`, never the other way around.
  // dashboard-styles.css scopes every transition as a compound selector
  // (`.slide-up-enter-active.view-wrapper`, …) and `.view-wrapper` is
  // `absolute inset-0` — if `view-wrapper` ever moves onto the view root
  // inside the padded wrapper, page margins break and the slide/depth
  // transitions stop firing. This test pins the structure so that regression
  // cannot silently return.
  it('renders the padded default wrapper inside div.view-wrapper', async () => {
    const router = makeRouter()
    router.push('/dashboard/other')
    await router.isReady()

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: { plugins: [router] },
    })
    await flushPromises()

    const viewWrapper = wrapper.find('.view-wrapper')
    expect(viewWrapper.exists()).toBe(true)

    // view-wrapper is the bare transition surface — no padding classes on it.
    expect(viewWrapper.classes()).not.toContain('px-4')
    expect(viewWrapper.classes()).not.toContain('view-container')

    // Its first child is the padded scroll container carrying the page margins.
    const padded = viewWrapper.element.firstElementChild as HTMLElement
    expect(padded).not.toBeNull()
    for (const cls of [
      'absolute', 'inset-0', 'px-4', 'pt-4', 'md:pt-8', 'md:px-8',
      'overflow-y-auto', 'mobile-safe-top', 'dashboard-scroll-panel', 'mobile-scroll-pad',
    ]) {
      expect(padded.classList.contains(cls), `padded wrapper missing ${cls}`).toBe(true)
    }

    // The routed view sits inside the padded wrapper with its per-component
    // classes, followed by the scroll-clearance spacer.
    const view = wrapper.find('.other-stub')
    expect(view.exists()).toBe(true)
    expect(view.classes()).toContain('view-container')
    expect(view.classes()).toContain('flex-none')
    expect(padded.contains(view.element)).toBe(true)
    const spacer = padded.querySelector('[aria-hidden="true"]')
    expect(spacer).not.toBeNull()
    expect(spacer!.classList.contains('shrink-0')).toBe(true)

    wrapper.unmount()
  })
})

describe('RefreshIndicator', () => {
  it('renders nothing for ready, idle and loading, and a labeled status element for refreshing', () => {
    const states = ['ready', 'idle', 'loading'] as const
    for (const state of states) {
      const wrapper = mount(RefreshIndicator, { props: { state } })
      expect(wrapper.find('[role="status"]').exists()).toBe(false)
      wrapper.unmount()
    }

    const refreshing = mount(RefreshIndicator, { props: { state: 'refreshing', label: 'Refreshing things' } })
    const status = refreshing.find('[role="status"]')
    expect(status.exists()).toBe(true)
    expect(status.attributes('aria-live')).toBe('polite')
    expect(refreshing.text()).toContain('Refreshing things')
    refreshing.unmount()
  })

  it('falls back to a non-empty default accessible label when none is given', () => {
    const wrapper = mount(RefreshIndicator, { props: { state: 'refreshing' } })
    expect(wrapper.find('[role="status"]').text().length).toBeGreaterThan(0)
    wrapper.unmount()
  })
})
