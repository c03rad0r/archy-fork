// 02-04: instance caching turned on for every audited main tab. This file
// covers the activate/deactivate lifecycle contract every registered view
// must honor (Task 1) and the widened registration set + eviction (Task 2).
//
// Two kinds of coverage:
// - A small synthetic consumer component, built with the exact same
//   idempotent onActivated/onDeactivated idiom every real view in this plan
//   uses (see Home.vue/Chat.vue/Apps.vue/Server.vue/Mesh.vue/Web5.vue/
//   Cloud.vue), proves the *pattern* in isolation.
// - At least one assertion against a real converted view (Server.vue's
//   vpnPollInterval), mounted inside a real <KeepAlive>, with fake timers.

import { flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter, type RouteRecordRaw } from 'vue-router'
import { createPinia } from 'pinia'
import { KeepAlive, defineComponent, h, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import DashboardRouterView from '../DashboardRouterView.vue'
import { KEEP_ALIVE_MAX, KEEP_ALIVE_PATHS, shouldKeepAlive } from '../keepAliveRoutes'
import { keepAliveIncludeNames, KEEP_WRAP_PREFIX } from '../dashboardViewWrappers'
import { rpcClient } from '@/api/rpc-client'
import Server from '@/views/Server.vue'
import Web5 from '@/views/web5/Web5.vue'
import Fleet from '@/views/Fleet.vue'
import FipsNetworkCard from '@/views/server/FipsNetworkCard.vue'
import Web5Monitoring from '@/views/web5/Web5Monitoring.vue'

// Hoisted by vitest — must live at module scope, not inside a test body, or
// Server.vue's/Web5.vue's own module-level `import { rpcClient } from
// '@/api/rpc-client'` would already be bound to the real implementation by
// the time a test ran. A Proxy fallback covers every rpcClient method either
// real view calls (Web5.vue's onMounted/armWeb5Live() reaches several beyond
// the four Server.vue needs) without having to enumerate them all by name.
vi.mock('@/api/rpc-client', () => {
  const base: Record<string, unknown> = {
    call: vi.fn().mockResolvedValue({}),
    vpnStatus: vi.fn().mockResolvedValue({ connected: false }),
    dnsStatus: vi.fn().mockResolvedValue({ provider: 'system', resolv_conf_servers: [], doh_enabled: false }),
    diskStatus: vi.fn().mockResolvedValue({ encrypted: false, warnings: [] }),
    resolveDid: vi.fn().mockResolvedValue({}),
    detectUsbDevices: vi.fn().mockResolvedValue({ devices: [] }),
    getNodeDid: vi.fn().mockResolvedValue({}),
  }
  return {
    rpcClient: new Proxy(base, {
      get(target, prop: string) {
        if (prop in target) return target[prop]
        const fn = vi.fn().mockResolvedValue({})
        target[prop] = fn
        return fn
      },
    }),
  }
})
vi.mock('@/stores/app', () => ({ useAppStore: () => ({ packages: {} }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

describe('keepAliveLifecycle: activate/deactivate contract (synthetic consumer)', () => {
  let mountCount: number
  let activateCount: number
  let deactivateCount: number
  let unmountCount: number
  let pollCallCount: number
  let subscribeCount: number
  let unsubscribeCount: number
  let listenerAddCount: number
  let listenerRemoveCount: number

  function makeSubscription() {
    subscribeCount++
    return () => { unsubscribeCount++ }
  }

  function onWindowEvent() { /* no-op handler identity for add/remove counting */ }

  // Mirrors the idiom established across Home.vue/Chat.vue/Server.vue/etc:
  // idempotent arm (clears/removes any existing handle first), started in
  // onActivated (which Vue also fires on first mount), torn down in
  // onDeactivated, with onBeforeUnmount left in place for the non-cached path.
  const Consumer = defineComponent({
    name: 'LifecycleConsumer',
    setup() {
      onMounted(() => { mountCount++ })

      let pollInterval: ReturnType<typeof setInterval> | null = null
      let unsubscribe: (() => void) | null = null

      function arm() {
        activateCount++
        if (pollInterval) clearInterval(pollInterval)
        pollCallCount++ // immediate invocation on (re)activation
        pollInterval = setInterval(() => { pollCallCount++ }, 1000)

        if (unsubscribe) unsubscribe()
        unsubscribe = makeSubscription()

        window.removeEventListener('resize', onWindowEvent)
        window.addEventListener('resize', onWindowEvent)
        listenerAddCount++
      }
      function disarm() {
        deactivateCount++
        if (pollInterval) { clearInterval(pollInterval); pollInterval = null }
        if (unsubscribe) { unsubscribe(); unsubscribe = null }
        window.removeEventListener('resize', onWindowEvent)
        listenerRemoveCount++
      }

      onActivated(() => arm())
      onDeactivated(() => disarm())
      onBeforeUnmount(() => { disarm(); unmountCount++ })

      return () => h('div', 'consumer')
    },
  })
  const Other = defineComponent({ render: () => h('div', 'other') })

  function mountHost() {
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Consumer, { key: 'consumer' }) : h(Other, { key: 'other' }),
        )
      },
    })
    return mount(Host)
  }

  beforeEach(() => {
    vi.useFakeTimers()
    mountCount = 0; activateCount = 0; deactivateCount = 0; unmountCount = 0
    pollCallCount = 0; subscribeCount = 0; unsubscribeCount = 0
    listenerAddCount = 0; listenerRemoveCount = 0
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('deactivating clears the polling interval — its callback is not invoked again while off screen', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()
    const callsAtActivation = pollCallCount

    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()

    vi.advanceTimersByTime(5000)
    expect(pollCallCount).toBe(callsAtActivation) // no further ticks while deactivated

    wrapper.unmount()
  })

  it('reactivating restarts the interval and immediately invokes the loader once, so the first frame is not interval-stale', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()
    const callsAtFirstActivation = pollCallCount

    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()

    // Immediate call on reactivation, before any timer has elapsed.
    expect(pollCallCount).toBe(callsAtFirstActivation + 1)

    wrapper.unmount()
  })

  it('deactivating unsubscribes a websocket handle; reactivating re-subscribes exactly once', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()
    expect(subscribeCount).toBe(1)
    expect(unsubscribeCount).toBe(0)

    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    expect(unsubscribeCount).toBe(1)

    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    expect(subscribeCount).toBe(2) // re-subscribed exactly once, not twice

    wrapper.unmount()
  })

  it('deactivating removes a window listener; reactivating adds it back exactly once', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()
    expect(listenerAddCount).toBe(1)

    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    expect(listenerRemoveCount).toBe(1)

    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    expect(listenerAddCount).toBe(2) // added back exactly once, not twice

    wrapper.unmount()
  })

  it('unmounting (a non-cached mount path) still tears everything down', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()

    wrapper.unmount()
    expect(unmountCount).toBe(1)
    expect(unsubscribeCount).toBe(1)
    expect(listenerRemoveCount).toBeGreaterThanOrEqual(1)

    const callsAtUnmount = pollCallCount
    vi.advanceTimersByTime(5000)
    expect(pollCallCount).toBe(callsAtUnmount) // no interval survives real unmount
  })

  it('two consecutive activations never double-arm a timer, subscription or listener', async () => {
    const wrapper = mountHost()
    await wrapper.vm.$nextTick()
    expect(activateCount).toBe(1)
    expect(subscribeCount).toBe(1)

    // Vue only calls onActivated on an actual (re)activation transition, so
    // simulate the double-activation risk directly: arm() is idempotent by
    // construction (clears/removes before re-adding), proven by driving two
    // deactivate/activate round trips back-to-back with no intervening tick.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()

    expect(activateCount).toBe(3) // initial + two round trips
    expect(subscribeCount).toBe(3) // never more than one live subscription at a time
    expect(unsubscribeCount).toBe(2) // one per deactivation, none doubled

    // Only one interval is ever live: advancing time ticks it exactly once
    // per 1000ms, never twice, proving no double-arm survived the round trips.
    const before = pollCallCount
    vi.advanceTimersByTime(1000)
    expect(pollCallCount).toBe(before + 1)

    wrapper.unmount()
  })

  it('a one-shot intro flag set on first entry is still set exactly once across three visits to the same tab', async () => {
    let onceFlagSetCount = 0
    const OnceConsumer = defineComponent({
      name: 'OnceFlagConsumer',
      setup() {
        onMounted(() => { onceFlagSetCount++ }) // once-per-session bucket: onMounted only, never onActivated
        return () => h('div', 'once')
      },
    })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(OnceConsumer, { key: 'once' }) : h(Other, { key: 'other' }),
        )
      },
    })
    const wrapper = mount(Host)
    await wrapper.vm.$nextTick()
    expect(onceFlagSetCount).toBe(1)

    // Visit 2
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    expect(onceFlagSetCount).toBe(1)

    // Visit 3
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    expect(onceFlagSetCount).toBe(1)

    wrapper.unmount()
  })

  it('an entry-scoped connection-timeout timer is re-armed on each re-entry and cleared on each exit, so it never fires against a stale visit', async () => {
    // Mirrors Apps.vue's connectionTimer idiom exactly (arm in onActivated,
    // idempotent; clear in onDeactivated; onBeforeUnmount clears too).
    let timerFiredCount = 0
    const ConnectionTimeoutConsumer = defineComponent({
      name: 'ConnectionTimeoutConsumer',
      setup() {
        let timer: ReturnType<typeof setTimeout> | undefined
        function arm() {
          if (timer) clearTimeout(timer)
          timer = setTimeout(() => { timerFiredCount++ }, 15000)
        }
        function disarm() {
          if (timer) clearTimeout(timer)
        }
        onActivated(() => arm())
        onDeactivated(() => disarm())
        onBeforeUnmount(() => disarm())
        return () => h('div', 'conn')
      },
    })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(ConnectionTimeoutConsumer, { key: 'conn' }) : h(Other, { key: 'other' }),
        )
      },
    })
    const wrapper = mount(Host)
    await wrapper.vm.$nextTick()

    // Leave well before the 15s guard fires — it must not fire against a
    // stale (now off-screen) visit.
    vi.advanceTimersByTime(10000)
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(10000) // would have fired by now if left armed
    expect(timerFiredCount).toBe(0)

    // Re-enter — the guard re-arms fresh; it should not fire before its own
    // full 15s has elapsed again from THIS entry.
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(10000)
    expect(timerFiredCount).toBe(0)
    vi.advanceTimersByTime(5000)
    expect(timerFiredCount).toBe(1)

    wrapper.unmount()
  })
})

describe('keepAliveLifecycle: real converted view (Server.vue vpnPollInterval)', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it("a real view's poll interval is not invoked while deactivated, and is invoked once on reactivation", async () => {
    const Other = defineComponent({ render: () => h('div', 'other') })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show
            ? h(Server, { key: 'server' })
            : h(Other, { key: 'other' }),
        )
      },
    })

    const wrapper = mount(Host, {
      global: {
        plugins: [createPinia()],
        stubs: {
          QuickActionsCard: true,
          TorServicesCard: true,
          ServerModals: true,
          FipsNetworkCard: true,
        },
      },
    })
    await flushPromises()

    const vpnStatusMock = vi.mocked(rpcClient.vpnStatus)
    const callsAtActivation = vpnStatusMock.mock.calls.length
    expect(callsAtActivation).toBeGreaterThan(0) // immediate call on first activation

    // Deactivate — the 15s poll must not keep calling vpnStatus while off
    // screen, regardless of how long the deactivation lasts (armVpnPoll's
    // setInterval is cleared by disarmVpnPoll on deactivate).
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(20000)
    await flushPromises()
    expect(vpnStatusMock.mock.calls.length).toBe(callsAtActivation)

    // Reactivate — two independent effects each call vpnStatus once here:
    // armVpnPoll's immediate first tick (always fires on activation), and
    // (02-06) networkRes's own onActivated revalidation, since 20s exceeds
    // networkRes's explicit 10s TTL (server.network-summary, the "fast
    // tier" per 02-06-SUMMARY.md) and its fetcher also calls vpnStatus.
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(vpnStatusMock.mock.calls.length).toBe(callsAtActivation + 2)

    wrapper.unmount()
  })
})

// 02-11 gap closure: three child components/composables inside the
// KeepAlive'd Fleet/Server/Web5 subtrees armed a setInterval poll in
// onMounted and only ever disarmed it in onUnmounted/onBeforeUnmount — a
// no-op before 02-04's KeepAlive registration (the owning view was
// destroyed on every tab-away, so onUnmounted fired every time) and a
// permanent, session-long background-RPC leak afterward, invisible to
// 02-04's own audit because it grepped each top-level view file, not the
// composables/child components it delegates to. These pin the fix: no
// poll activity while deactivated, resumed on reactivation — the exact
// contract `Server.vue vpnPollInterval` above already proves for the view
// files 02-04 did audit directly.
describe('keepAliveLifecycle: 02-11 gap closure — leaked background pollers in un-audited child composables', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it("Fleet.vue's useFleetData() poll (telemetry.fleet-status/-alerts) is not invoked while deactivated, and resumes on reactivation", async () => {
    const Other = defineComponent({ render: () => h('div', 'other') })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Fleet, { key: 'fleet' }) : h(Other, { key: 'other' }),
        )
      },
    })

    const wrapper = mount(Host, {
      global: {
        plugins: [createPinia()],
        stubs: {
          FleetOverviewCards: true,
          FleetNodeGrid: true,
          FleetAlerts: true,
          FleetNodeDetail: true,
          FleetContainerMatrix: true,
          BackButton: true,
          RouterLink: true,
        },
      },
    })
    await flushPromises()

    const callMock = vi.mocked(rpcClient.call)
    const fleetCallCount = () =>
      callMock.mock.calls.filter(([arg]) => (arg as { method?: string }).method?.startsWith('telemetry.fleet-')).length

    const callsAtActivation = fleetCallCount()
    expect(callsAtActivation).toBeGreaterThan(0) // refreshAll() on the initial mount

    // Deactivate — the 60s poll must not keep firing while off screen.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(180_000) // 3x the poll interval
    await flushPromises()
    expect(fleetCallCount()).toBe(callsAtActivation)

    // Reactivate — armFleetPoll() re-arms the interval; no call is expected
    // synchronously on activation itself (unlike Server's vpnPollInterval,
    // this composable's poll has no "immediate tick" — it only ticks on the
    // next 60s boundary), but a tick after reactivation must fire again.
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    vi.advanceTimersByTime(60_000)
    await flushPromises()
    expect(fleetCallCount()).toBeGreaterThan(callsAtActivation)

    wrapper.unmount()
  })

  it("FipsNetworkCard.vue's 15s fips.status poll is not invoked while deactivated, and resumes on reactivation", async () => {
    const Other = defineComponent({ render: () => h('div', 'other') })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(FipsNetworkCard, { key: 'fips' }) : h(Other, { key: 'other' }),
        )
      },
    })

    const wrapper = mount(Host, { global: { plugins: [createPinia()] } })
    await flushPromises()

    const callMock = vi.mocked(rpcClient.call)
    const fipsCallCount = () =>
      callMock.mock.calls.filter(([arg]) => (arg as { method?: string }).method === 'fips.status').length

    const callsAtActivation = fipsCallCount()
    expect(callsAtActivation).toBeGreaterThan(0) // useCachedResource's own immediate fetch on mount

    // Deactivate — the manual 15s poll must not keep calling fips.status
    // while off screen, regardless of how long the deactivation lasts.
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(45_000) // 3x the poll interval
    await flushPromises()
    expect(fipsCallCount()).toBe(callsAtActivation)

    // Reactivate — armFipsPoll's immediate setInterval arm plus
    // useCachedResource's own onActivated revalidation (stale past its 15s
    // TTL) both contribute; the decisive assertion is simply "more than
    // zero new calls after reactivation", not an exact count, since the two
    // effects' relative ordering is an implementation detail.
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(fipsCallCount()).toBeGreaterThan(callsAtActivation)

    wrapper.unmount()
  })

  it("Web5Monitoring.vue's 30s system.stats poll is not invoked while deactivated, and resumes on reactivation", async () => {
    const Other = defineComponent({ render: () => h('div', 'other') })
    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Web5Monitoring, { key: 'web5mon' }) : h(Other, { key: 'other' }),
        )
      },
    })

    // homeStatus.ts's refreshSystemStats() assigns numeric fields straight
    // from the RPC response (e.g. `stats.cpuPercent = res.cpu_usage_percent`)
    // with no fallback — the file-wide default `call` mock resolving `{}`
    // leaves `cpuPercent` as `undefined`, and the template's `.toFixed(0)`
    // throws. Scoped to this test only (restored to the file-wide default in
    // reverse order after) so no other test's call-count assertions shift.
    const callMock = vi.mocked(rpcClient.call)
    const priorImpl = callMock.getMockImplementation()
    callMock.mockImplementation(async (arg: unknown) => {
      const method = (arg as { method?: string }).method
      if (method === 'system.stats') {
        return {
          cpu_usage_percent: 12,
          mem_used_bytes: 1,
          mem_total_bytes: 2,
          disk_used_bytes: 1,
          disk_total_bytes: 2,
          uptime_secs: 100,
        }
      }
      return {}
    })

    const wrapper = mount(Host, {
      global: {
        plugins: [createPinia()],
        stubs: { RouterLink: true },
      },
    })
    await flushPromises()
    const statsCallCount = () =>
      callMock.mock.calls.filter(([arg]) => (arg as { method?: string }).method === 'system.stats').length

    const callsAtActivation = statsCallCount()
    expect(callsAtActivation).toBeGreaterThan(0) // loadStats() on the initial mount

    // Deactivate — the 30s poll must not keep calling system.stats while
    // off screen (previously ran forever once Web5 was first visited).
    ;(wrapper.vm as unknown as { show: boolean }).show = false
    await wrapper.vm.$nextTick()
    vi.advanceTimersByTime(90_000) // 3x the poll interval
    await flushPromises()
    expect(statsCallCount()).toBe(callsAtActivation)

    // Reactivate — armWeb5MonitoringPoll() calls loadStats() immediately.
    ;(wrapper.vm as unknown as { show: boolean }).show = true
    await wrapper.vm.$nextTick()
    await flushPromises()
    expect(statsCallCount()).toBeGreaterThan(callsAtActivation)

    wrapper.unmount()
    // Restore the file-wide default so later tests' `call` behavior is
    // unaffected — never `mockReset()`, which would also wipe the queued
    // implementation the top-level `vi.mock('@/api/rpc-client', ...)`
    // factory set up, leaving `call()` return `undefined` instead of a
    // resolved promise for every test that runs after this one.
    callMock.mockImplementation(priorImpl ?? (async () => ({})))
  })
})

describe('keepAliveRoutes: widened registration set (02-04)', () => {
  it('shouldKeepAlive returns true for every registered main-tab path and false for every detail path, including detail paths whose prefix matches a registered path', () => {
    expect(shouldKeepAlive({ path: '/dashboard/apps' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/apps/bitcoin' })).toBe(false)

    expect(shouldKeepAlive({ path: '/dashboard/marketplace' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/marketplace/x' })).toBe(false)

    expect(shouldKeepAlive({ path: '/dashboard/cloud' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/cloud/x' })).toBe(false)

    expect(shouldKeepAlive({ path: '/dashboard/server' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/server/openwrt' })).toBe(false)

    expect(shouldKeepAlive({ path: '/dashboard/web5' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/web5/credentials' })).toBe(false)

    // Settings is in TAB_ORDER but withheld (unaudited child sections) —
    // registering it is neither required nor safe yet (see keepAliveRoutes.ts).
    expect(shouldKeepAlive({ path: '/dashboard/settings' })).toBe(false)
    expect(shouldKeepAlive({ path: '/dashboard/settings/update' })).toBe(false)

    // Discover (App Store's second tab) is not in TAB_ORDER but is registered
    // explicitly — measured `remount storm` in 02-FINDINGS.md.
    expect(shouldKeepAlive({ path: '/dashboard/discover' })).toBe(true)
  })

  it('registers every TAB_ORDER path measured Remounted:true or unmeasured, per the literal exclusion rule (only a measured Remounted:false excludes)', () => {
    for (const path of ['/dashboard', '/dashboard/apps', '/dashboard/marketplace', '/dashboard/cloud', '/dashboard/mesh', '/dashboard/server', '/dashboard/web5', '/dashboard/fleet', '/dashboard/chat']) {
      expect(shouldKeepAlive({ path })).toBe(true)
    }
  })

  it('visiting more distinct registered tabs than KEEP_ALIVE_MAX leaves exactly KEEP_ALIVE_MAX instances resident — the least recently used is evicted', async () => {
    const paths = Array.from(KEEP_ALIVE_PATHS).slice(0, KEEP_ALIVE_MAX + 2)
    expect(paths.length).toBe(KEEP_ALIVE_MAX + 2)
    const firstPath = paths[0]
    if (!firstPath) throw new Error('expected at least one registered path')
    const remainingPaths = paths.slice(1)

    const mountCounts: Record<string, number> = {}
    const routes: RouteRecordRaw[] = paths.map((path) => {
      mountCounts[path] = 0
      const Stub = defineComponent({
        name: `EvictionStub:${path}`,
        setup() {
          onMounted(() => { mountCounts[path] = (mountCounts[path] ?? 0) + 1 })
          return () => h('div', { class: 'eviction-stub' }, path)
        },
      })
      return { path, component: Stub }
    })
    const router = createRouter({ history: createMemoryHistory(), routes })
    router.push(firstPath)
    await router.isReady()

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: { plugins: [router] },
    })
    await flushPromises()
    expect(mountCounts[firstPath]).toBe(1)

    // Visit every remaining registered path in order — KEEP_ALIVE_MAX + 1
    // more distinct instances, so the cache (capped at KEEP_ALIVE_MAX) must
    // evict the least-recently-used one (firstPath) to stay within budget.
    for (const path of remainingPaths) {
      await router.push(path)
      await flushPromises()
    }
    expect(mountCounts[firstPath]).toBe(1) // not yet revisited — still just the one mount

    // Revisiting the evicted path proves eviction: it remounts (count -> 2)
    // rather than merely reactivating (which would leave the count at 1).
    await router.push(firstPath)
    await flushPromises()
    expect(mountCounts[firstPath]).toBe(2)

    wrapper.unmount()
  })
})

// 02-09 gap closure: Task 1 named the cause of the apparent "Server.vue
// genuinely remounts" gap as a proven probe-measurement artifact (see
// 02-FINDINGS.md's "## Server KeepAlive Root Cause (gap closure)" section)
// rather than a real KeepAlive/lifecycle defect — an authoritative,
// independent `document.elementFromPoint()` hit-test signal on the deployed
// a test node build directly contradicted the naive selector-match probe's
// "remounted" verdict for both Server.vue and Web5.vue, and a companion
// diagnostic found the ORIGINAL stamped root still connected and visible
// under a different (unpicked) DOM match. No source change to
// DashboardRouterView.vue / dashboardViewWrappers.ts / keepAliveRoutes.ts /
// Server.vue's KeepAlive/lifecycle wiring is made — these tests pin the
// architecture's CURRENT, unmodified behavior as the regression guard Task
// 2's `<behavior>` calls for, using Vue's own component-instance identity
// (`vm.$.uid`) rather than a CSS selector, which is exactly the class of
// signal that sidesteps the generic-`.view-container`-selector ambiguity
// Task 1 found responsible for the false "remounted" reading in the first
// place.
describe('keepAliveLifecycle: 02-09 gap closure — Server/Web5 round-trip survival, measured via component-instance identity (not a CSS selector)', () => {
  function mountRealRouterView(startPath: string) {
    const SettingsStub = defineComponent({ name: 'SettingsStub', render: () => h('div', 'settings') })
    const routes: RouteRecordRaw[] = [
      { path: '/dashboard/server', name: 'server', component: Server },
      { path: '/dashboard/web5', name: 'web5', component: Web5 },
      { path: '/dashboard/settings', name: 'settings', component: SettingsStub },
    ]
    const router = createRouter({ history: createMemoryHistory(), routes })
    router.push(startPath)
    return { router, routes }
  }

  it('Test 1 (the gap): a round-trip through /dashboard/server mounts the real Server.vue exactly once — the second arrival reactivates, not remounts', async () => {
    const { router } = mountRealRouterView('/dashboard/server')
    await router.isReady()

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: {
        plugins: [router, createPinia()],
        stubs: {
          QuickActionsCard: true,
          TorServicesCard: true,
          ServerModals: true,
          FipsNetworkCard: true,
        },
      },
    })
    await flushPromises()

    const firstInstance = wrapper.findComponent(Server)
    expect(firstInstance.exists()).toBe(true)
    const firstUid = firstInstance.vm.$.uid

    await router.push('/dashboard/settings')
    await flushPromises()
    await router.push('/dashboard/server')
    await flushPromises()

    const secondInstance = wrapper.findComponent(Server)
    expect(secondInstance.exists()).toBe(true)
    // Same instance uid == exactly one Server.vue instance ever existed for
    // this round trip (a genuine remount would create a second, higher uid).
    expect(secondInstance.vm.$.uid).toBe(firstUid)

    wrapper.unmount()
  })

  it('Test 2 (no collateral damage): the same round-trip through Web5.vue (the other surface this gap closure re-examined) and a synthetic second tab both keep their instance/mount counts at 1', async () => {
    let appsMountCount = 0
    const AppsStubWithCounter = defineComponent({
      name: 'AppsStubCounter',
      setup() {
        onMounted(() => { appsMountCount++ })
        return () => h('div', 'apps')
      },
    })

    const SettingsStub = defineComponent({ name: 'SettingsStub', render: () => h('div', 'settings') })
    const routes: RouteRecordRaw[] = [
      { path: '/dashboard/web5', name: 'web5', component: Web5 },
      { path: '/dashboard/apps', name: 'apps', component: AppsStubWithCounter },
      { path: '/dashboard/settings', name: 'settings', component: SettingsStub },
    ]
    const router = createRouter({ history: createMemoryHistory(), routes })
    router.push('/dashboard/web5')
    await router.isReady()

    // Web5.vue's armWeb5Live() (its onActivated/onMounted arm function, the
    // same idiom Server.vue's armServerEntryEffects() uses) calls exposed
    // methods on child refs directly — a bare `stubs: { X: true }` auto-stub
    // doesn't expose anything, so it throws under test. Real, minimal expose
    // stubs (no-op bodies) let armWeb5Live() run exactly as it does against
    // the real children, without needing their full implementations.
    const ConnectedNodesStub = defineComponent({
      name: 'Web5ConnectedNodesStub',
      setup(_props, { expose }) {
        expose({ loadPeers: () => {}, loadReceivedMessages: () => {}, loadConnectionRequests: () => {} })
        return () => h('div')
      },
    })
    const NodeVisibilityStub = defineComponent({
      name: 'Web5NodeVisibilityStub',
      setup(_props, { expose }) {
        expose({ loadVisibility: () => {} })
        return () => h('div')
      },
    })
    const IdentitiesStub = defineComponent({
      name: 'Web5IdentitiesStub',
      setup(_props, { expose }) {
        expose({ loadIdentities: () => {} })
        return () => h('div')
      },
    })
    const NostrRelaysStub = defineComponent({
      name: 'Web5NostrRelaysStub',
      setup(_props, { expose }) {
        expose({ loadNostrRelays: () => {}, openRelaysModal: () => {} })
        return () => h('div')
      },
    })

    const wrapper = mount(DashboardRouterView, {
      props: { mobileTabPaddingTop: null, needsMobileBackButtonSpace: false },
      global: {
        plugins: [router, createPinia()],
        stubs: {
          Web5QuickActions: true,
          Web5ConnectedNodes: ConnectedNodesStub,
          Web5NodeVisibility: NodeVisibilityStub,
          Web5Identities: IdentitiesStub,
          Web5NostrRelays: NostrRelaysStub,
          Web5Monitoring: true,
          Web5Federation: true,
        },
      },
    })
    await flushPromises()

    const firstWeb5 = wrapper.findComponent(Web5)
    expect(firstWeb5.exists()).toBe(true)
    const firstWeb5Uid = firstWeb5.vm.$.uid

    await router.push('/dashboard/apps')
    await flushPromises()
    expect(appsMountCount).toBe(1)

    await router.push('/dashboard/settings')
    await flushPromises()
    await router.push('/dashboard/web5')
    await flushPromises()
    expect(wrapper.findComponent(Web5).vm.$.uid).toBe(firstWeb5Uid) // Web5 reactivated, not remounted

    await router.push('/dashboard/settings')
    await flushPromises()
    await router.push('/dashboard/apps')
    await flushPromises()
    expect(appsMountCount).toBe(1) // still just the one mount — Apps reactivated too

    wrapper.unmount()
  })

  it('Test 3 (registration is really the include list): every keepAliveIncludeNames() entry is a KeepWrap:<path> name for a registered path, contains no comma (Vue\'s own include-matching splits only on commas), and /dashboard/server\'s wrapper name is among them', () => {
    const names = keepAliveIncludeNames()
    expect(names.length).toBe(KEEP_ALIVE_PATHS.size)
    for (const name of names) {
      expect(name.startsWith(KEEP_WRAP_PREFIX)).toBe(true)
      const path = name.slice(KEEP_WRAP_PREFIX.length)
      expect(KEEP_ALIVE_PATHS.has(path)).toBe(true)
      expect(name.includes(',')).toBe(false)
    }
    expect(names).toContain(`${KEEP_WRAP_PREFIX}/dashboard/server`)
    expect(names).toContain(`${KEEP_WRAP_PREFIX}/dashboard/web5`)
  })

  it('Test 4 (the bound stays bound): shouldKeepAlive/KEEP_ALIVE_MAX are unchanged by this gap closure — Server and Web5 are still registered, still evictable, and the cap is still 6', () => {
    expect(shouldKeepAlive({ path: '/dashboard/server' })).toBe(true)
    expect(shouldKeepAlive({ path: '/dashboard/web5' })).toBe(true)
    expect(KEEP_ALIVE_MAX).toBe(6)
  })
})
