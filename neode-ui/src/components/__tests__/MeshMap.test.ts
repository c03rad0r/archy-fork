import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import MeshMap from '../MeshMap.vue'

const meshState = vi.hoisted(() => ({
  nodePositions: new Map(),
  federatedPositions: new Map(),
  peers: [],
  status: null,
  deadmanStatus: null,
  updateSelfPosition: vi.fn(),
}))

vi.mock('@/stores/mesh', () => ({
  useMeshStore: () => meshState,
}))

vi.mock('leaflet', () => ({
  default: {
    map: vi.fn(() => ({
      invalidateSize: vi.fn(),
      fitBounds: vi.fn(),
      remove: vi.fn(),
    })),
    tileLayer: vi.fn(() => ({ addTo: vi.fn() })),
    layerGroup: vi.fn(() => ({ addTo: vi.fn(), clearLayers: vi.fn(), addLayer: vi.fn() })),
    divIcon: vi.fn((opts) => opts),
    marker: vi.fn(() => ({ bindPopup: vi.fn() })),
    polyline: vi.fn(() => ({})),
    latLngBounds: vi.fn(() => ({ pad: vi.fn() })),
  },
}))

describe('MeshMap', () => {
  beforeEach(() => {
    meshState.nodePositions.clear()
    meshState.federatedPositions.clear()
    meshState.peers = []
    meshState.status = null
    meshState.deadmanStatus = null
    meshState.updateSelfPosition.mockClear()
  })

  it('treats denied browser location as optional for peer positions', async () => {
    let errorHandler!: (error: { code: number; message: string }) => void
    const watchPosition = vi.fn((_success, error) => {
      errorHandler = error
      return 7
    })
    const clearWatch = vi.fn()
    const resizeObserver = vi.fn(() => ({
      observe: vi.fn(),
      disconnect: vi.fn(),
    }))
    vi.stubGlobal('navigator', {
      geolocation: { watchPosition, clearWatch },
    })
    vi.stubGlobal('ResizeObserver', resizeObserver)

    const wrapper = mount(MeshMap)

    expect(wrapper.text()).toContain('Waiting for mesh device positions.')

    await wrapper.get('[role="switch"]').trigger('click')
    errorHandler({ code: 1, message: 'denied' })
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('Location permission denied. Peer locations can still appear on the map.')
    expect(wrapper.text()).toContain('Local location is off. Other device positions will appear when received.')
    expect(wrapper.text()).not.toContain('location sharing is required')

    vi.unstubAllGlobals()
  })
})
