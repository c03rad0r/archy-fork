// 02-05 Task 2: bounds MeshMap.vue's Leaflet instance across Mesh.vue's
// activate/deactivate lifecycle (established in 02-04 — Mesh.vue joined
// KEEP_ALIVE_PATHS). Kept in its own file rather than
// src/views/__tests__/meshTabCache.test.ts because `vi.mock('@/stores/mesh')`
// and `vi.mock('leaflet')` are hoisted file-wide and would otherwise clobber
// that file's need for the REAL mesh/transport stores (mirrors the
// MarketplaceRefresh.test.ts precedent set in 02-02 for the same class of
// vi.mock-hoisting conflict — Rule 1/3 auto-fix, documented in
// 02-05-SUMMARY.md).
//
// FLAGGED (see 02-05-SUMMARY.md): RESEARCH.md's premise that Mesh.vue owns a
// live D3 force simulation does not hold for this codebase — a full grep for
// `d3`/`forceSimulation`/`simulation` across neode-ui/src turns up nothing in
// Mesh.vue's component tree (or MeshMap.vue's); the only D3 force simulation
// in the codebase belongs to NetworkMap3D.vue (Federation.vue's graph, out of
// this plan's scope). This file therefore only covers the Leaflet map's
// activate/deactivate lifecycle — the D3-specific truths from the plan are
// vacuously satisfied (there is nothing to leak).

import { flushPromises, mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import MeshMap from '../MeshMap.vue'

const mapInstances: Array<{ invalidateSize: ReturnType<typeof vi.fn>; remove: ReturnType<typeof vi.fn> }> = []
let resizeObserverInstances: Array<{ observe: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn> }> = []

vi.mock('@/stores/mesh', () => ({
  useMeshStore: () => ({
    nodePositions: new Map(),
    federatedPositions: new Map(),
    peers: [],
    status: null,
    deadmanStatus: null,
    updateSelfPosition: vi.fn(),
  }),
}))

vi.mock('leaflet', () => ({
  default: {
    map: vi.fn(() => {
      const instance = { invalidateSize: vi.fn(), fitBounds: vi.fn(), remove: vi.fn(), setView: vi.fn() }
      mapInstances.push(instance)
      return instance
    }),
    tileLayer: vi.fn(() => ({ addTo: vi.fn() })),
    layerGroup: vi.fn(() => ({ addTo: vi.fn(), clearLayers: vi.fn(), addLayer: vi.fn() })),
    divIcon: vi.fn((opts: unknown) => opts),
    marker: vi.fn(() => ({ bindPopup: vi.fn() })),
    polyline: vi.fn(() => ({})),
    latLngBounds: vi.fn(() => ({})),
  },
}))

const Other = defineComponent({ name: 'Other', render: () => h('div', 'other') })

function mountMapHost() {
  const Host = defineComponent({
    setup() {
      const show = ref(true)
      return { show }
    },
    render() {
      return h(KeepAlive, null, () =>
        this.show ? h(MeshMap, { key: 'map' }) : h(Other, { key: 'other' }),
      )
    },
  })
  return mount(Host)
}

async function toggleTab(wrapper: ReturnType<typeof mountMapHost>, show: boolean) {
  ;(wrapper.vm as unknown as { show: boolean }).show = show
  await wrapper.vm.$nextTick()
}

describe('Mesh graphics lifecycle (Task 2): Leaflet map (MeshMap.vue)', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    mapInstances.length = 0
    resizeObserverInstances = []
    vi.stubGlobal('ResizeObserver', vi.fn(() => {
      const inst = { observe: vi.fn(), disconnect: vi.fn(), unobserve: vi.fn() }
      resizeObserverInstances.push(inst)
      return inst
    }))
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
      height: 200, width: 200, top: 0, left: 0, right: 0, bottom: 0, x: 0, y: 0, toJSON: () => undefined,
    } as DOMRect)
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('entering and leaving the tab three times constructs exactly one map instance', async () => {
    const wrapper = mountMapHost()
    await flushPromises()
    vi.advanceTimersByTime(300) // the onMounted-arm's fallback initMap()
    await flushPromises()
    expect(mapInstances.length).toBe(1)

    for (let i = 0; i < 3; i++) {
      await toggleTab(wrapper, false)
      await toggleTab(wrapper, true)
      await flushPromises()
    }
    expect(mapInstances.length).toBe(1)

    wrapper.unmount()
  })

  it('reactivating calls the Leaflet map size-invalidation so a map laid out off screen re-tiles at its real size', async () => {
    const wrapper = mountMapHost()
    await flushPromises()
    vi.advanceTimersByTime(300)
    await flushPromises()
    const instance = mapInstances[0]!
    instance.invalidateSize.mockClear()

    await toggleTab(wrapper, false)
    await toggleTab(wrapper, true)
    await flushPromises()

    expect(instance.invalidateSize).toHaveBeenCalled()
    wrapper.unmount()
  })

  it('a window resize listener is removed on deactivate and re-added exactly once on activate', async () => {
    const addSpy = vi.spyOn(window, 'addEventListener')
    const removeSpy = vi.spyOn(window, 'removeEventListener')
    const wrapper = mountMapHost()
    await flushPromises()
    // armMapVisibility's own idempotent idiom (remove-then-add) means mount
    // itself issues one defensive remove alongside the one add — baseline
    // both counts here rather than assuming remove starts at zero.
    const addCountAtMount = addSpy.mock.calls.filter((c) => c[0] === 'resize').length
    const removeCountAtMount = removeSpy.mock.calls.filter((c) => c[0] === 'resize').length
    expect(addCountAtMount).toBe(1)

    await toggleTab(wrapper, false)
    expect(removeSpy.mock.calls.filter((c) => c[0] === 'resize').length).toBe(removeCountAtMount + 1)

    await toggleTab(wrapper, true)
    expect(addSpy.mock.calls.filter((c) => c[0] === 'resize').length).toBe(addCountAtMount + 1) // once more, not twice

    wrapper.unmount()
  })

  it('deactivating disconnects the ResizeObserver; reactivating re-observes the container', async () => {
    const wrapper = mountMapHost()
    await flushPromises()
    const inst = resizeObserverInstances[0]!
    expect(inst.observe).toHaveBeenCalledTimes(1)

    await toggleTab(wrapper, false)
    expect(inst.disconnect).toHaveBeenCalledTimes(1)

    await toggleTab(wrapper, true)
    expect(inst.observe).toHaveBeenCalledTimes(2)

    wrapper.unmount()
  })
})
