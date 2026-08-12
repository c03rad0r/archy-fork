// 02-05: caches the Mesh tab's six uncached fetch groups behind
// useCachedResource entries, and bounds the Leaflet map's lifecycle across
// Mesh.vue's activate/deactivate cycle (established in 02-04).
//
// FLAGGED (see 02-05-SUMMARY.md): RESEARCH.md's premise that Mesh.vue owns a
// live D3 force simulation does not hold for this codebase — a full grep for
// `d3`/`forceSimulation`/`simulation` across neode-ui/src turns up nothing in
// Mesh.vue's component tree; the only D3 force simulation belongs to
// NetworkMap3D.vue (Federation.vue's graph, out of this plan's scope). This
// file therefore only covers the six cached fetch groups (Task 1) and the
// Leaflet map's activate/deactivate lifecycle (Task 2, MeshMap.vue) — the
// D3-specific truths are vacuously satisfied (there is nothing to leak).

import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Mesh from '../Mesh.vue'
import RefreshIndicator from '@/components/RefreshIndicator.vue'
import { rpcClient } from '@/api/rpc-client'
import { useMeshStore } from '@/stores/mesh'

vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
}))

vi.mock('@/api/websocket', () => ({
  wsClient: { subscribe: vi.fn(() => vi.fn()) },
}))

function meshStatusPayload(overrides: Record<string, unknown> = {}) {
  return {
    enabled: true,
    device_type: 'meshcore',
    device_path: null,
    device_connected: true,
    firmware_version: null,
    self_node_id: 1,
    self_advert_name: 'Self',
    peer_count: 0,
    channel_name: 'Public',
    messages_sent: 0,
    messages_received: 0,
    ...overrides,
  }
}

// Records every rpcClient.call({method}) invocation so tests can assert per-
// group call counts and start-order without depending on real network I/O.
const rpcCallMock = vi.fn(async ({ method }: { method: string }) => {
  switch (method) {
    case 'mesh.status':
      return meshStatusPayload()
    case 'mesh.peers':
      return { peers: [], count: 0 }
    case 'mesh.messages':
      return { messages: [], count: 0 }
    case 'mesh.deadman-status':
      return { enabled: false }
    case 'mesh.block-headers':
      return { headers: [], latest_height: 0, count: 0 }
    case 'transport.status':
      return { transports: [], mesh_only: false, peer_count: 0 }
    default:
      return {}
  }
})

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: (...args: unknown[]) => (rpcCallMock as unknown as (...a: unknown[]) => unknown)(...args),
    federationListNodes: vi.fn().mockResolvedValue({ nodes: [] }),
    getTorAddress: vi.fn().mockResolvedValue({ tor_address: null }),
    getNodeDid: vi.fn().mockResolvedValue({ did: 'did:key:z6Mkself' }),
    meshContactsList: vi.fn().mockResolvedValue({ contacts: [] }),
  },
}))

const Other = defineComponent({ name: 'Other', render: () => h('div', 'other') })

function mountMeshHost() {
  const Host = defineComponent({
    setup() {
      const show = ref(true)
      return { show }
    },
    render() {
      return h(KeepAlive, null, () =>
        this.show ? h(Mesh, { key: 'mesh' }) : h(Other, { key: 'other' }),
      )
    },
  })
  return mount(Host, {
    global: {
      plugins: [createPinia()],
      stubs: {
        AnimatedLogo: true,
        MeshMap: true,
        MeshBitcoinPanel: true,
        MeshDeadmanPanel: true,
        MeshDevicePanel: true,
        MeshAssistantPanel: true,
        HopVizModal: true,
        MediaLightbox: true,
      },
    },
  })
}

function callCountFor(method: string): number {
  return rpcCallMock.mock.calls.filter(([opts]) => (opts as { method: string }).method === method).length
}

function convenienceCallCounts() {
  return {
    federationListNodes: vi.mocked(rpcClient.federationListNodes).mock.calls.length,
    getTorAddress: vi.mocked(rpcClient.getTorAddress).mock.calls.length,
    getNodeDid: vi.mocked(rpcClient.getNodeDid).mock.calls.length,
    meshContactsList: vi.mocked(rpcClient.meshContactsList).mock.calls.length,
  }
}

async function toggleTab(wrapper: ReturnType<typeof mountMeshHost>, show: boolean) {
  ;(wrapper.vm as unknown as { show: boolean }).show = show
  await wrapper.vm.$nextTick()
}

beforeEach(() => {
  vi.useFakeTimers()
  rpcCallMock.mockClear()
  vi.mocked(rpcClient.federationListNodes).mockClear()
  vi.mocked(rpcClient.getTorAddress).mockClear()
  vi.mocked(rpcClient.getNodeDid).mockClear()
  vi.mocked(rpcClient.meshContactsList).mockClear()
  try {
    sessionStorage.clear()
  } catch { /* unavailable in some envs */ }
})

afterEach(() => {
  vi.useRealTimers()
})

describe('Mesh tab cache (Task 1): six fetch groups', () => {
  it('a cold load fires all six groups concurrently — every RPC has already started before any microtask resolves', async () => {
    const wrapper = mountMeshHost()

    // Synchronously (no await, no flushPromises yet) every one of the six
    // groups' underlying calls must already have fired: armMeshLive() runs
    // Promise.allSettled(meshCachedGroups.map(refreshMeshGroupIfStale)),
    // and none of the intermediate layers (refreshMeshGroupIfStale ->
    // store.refresh -> fetcher -> mesh.refreshAll/refreshFederationNodes/etc
    // -> rpcClient.call) awaits anything before reaching the RPC call — a
    // serialized chain (awaiting one group before starting the next) could
    // not possibly have reached all six yet at this point.
    expect(callCountFor('mesh.status')).toBe(1)
    expect(callCountFor('mesh.peers')).toBe(1)
    expect(callCountFor('mesh.messages')).toBe(1)
    expect(callCountFor('mesh.deadman-status')).toBe(1)
    expect(callCountFor('mesh.block-headers')).toBe(1)
    expect(callCountFor('transport.status')).toBe(1)
    expect(convenienceCallCounts()).toEqual({
      federationListNodes: 1, getTorAddress: 1, getNodeDid: 1, meshContactsList: 1,
    })

    await flushPromises()
    wrapper.unmount()
  })

  it('reactivating inside every group\'s TTL issues zero additional RPCs across all six groups', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()
    const before = {
      status: callCountFor('mesh.status'),
      peers: callCountFor('mesh.peers'),
      messages: callCountFor('mesh.messages'),
      deadman: callCountFor('mesh.deadman-status'),
      headers: callCountFor('mesh.block-headers'),
      transport: callCountFor('transport.status'),
      ...convenienceCallCounts(),
    }

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(3000) // well under every group's TTL (shortest is 10s)
    await toggleTab(wrapper, true)
    await flushPromises()

    expect(callCountFor('mesh.status')).toBe(before.status)
    expect(callCountFor('mesh.peers')).toBe(before.peers)
    expect(callCountFor('mesh.messages')).toBe(before.messages)
    expect(callCountFor('mesh.deadman-status')).toBe(before.deadman)
    expect(callCountFor('mesh.block-headers')).toBe(before.headers)
    expect(callCountFor('transport.status')).toBe(before.transport)
    expect(convenienceCallCounts()).toEqual({
      federationListNodes: before.federationListNodes,
      getTorAddress: before.getTorAddress,
      getNodeDid: before.getNodeDid,
      meshContactsList: before.meshContactsList,
    })

    wrapper.unmount()
  })

  it('reactivating past the short (10s) TTL revalidates the fast-moving groups while the near-static identity groups stay cached', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()
    const statusCallsAtMount = callCountFor('mesh.status')
    const transportCallsAtMount = callCountFor('transport.status')
    const onionCallsAtMount = vi.mocked(rpcClient.getTorAddress).mock.calls.length
    const didCallsAtMount = vi.mocked(rpcClient.getNodeDid).mock.calls.length

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(15000) // past the 10s reachability/transport TTL, well under the 300s identity TTL
    await toggleTab(wrapper, true)
    await flushPromises()

    // Peer/status/transport (reachability-class, 10s TTL) revalidate exactly once.
    expect(callCountFor('mesh.status')).toBe(statusCallsAtMount + 1)
    expect(callCountFor('transport.status')).toBe(transportCallsAtMount + 1)
    // This node's own onion/DID (300s TTL, near-static identity) are still fresh.
    expect(vi.mocked(rpcClient.getTorAddress).mock.calls.length).toBe(onionCallsAtMount)
    expect(vi.mocked(rpcClient.getNodeDid).mock.calls.length).toBe(didCallsAtMount)

    wrapper.unmount()
  })

  it('the previous graph/peer data stays rendered while a stale group revalidates (sticky-ready, no blank frame)', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()
    expect(wrapper.text()).toContain('Mesh Network')

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(15000)
    await toggleTab(wrapper, true)
    // Before the revalidation resolves, prior content must still be present
    // (sticky-ready never regresses to a blank/loading state).
    expect(wrapper.text()).toContain('Mesh Network')
    await flushPromises()
    expect(wrapper.text()).toContain('Mesh Network')

    wrapper.unmount()
  })

  it('one group rejecting leaves the other five unaffected and the fan-out still completes (deep-link/outbox callback runs)', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)
    const meshStore = useMeshStore()
    const rejectingRefreshAll = vi.fn().mockRejectedValue(new Error('boom'))
    // Bypass the store's own internal try/catch entirely so a genuine
    // rejection reaches useCachedResource's fetcher wrapper.
    meshStore.refreshAll = rejectingRefreshAll

    const Host = defineComponent({
      setup() {
        const show = ref(true)
        return { show }
      },
      render() {
        return h(KeepAlive, null, () =>
          this.show ? h(Mesh, { key: 'mesh' }) : h(Other, { key: 'other' }),
        )
      },
    })
    const wrapper = mount(Host, {
      global: {
        plugins: [pinia],
        stubs: {
          AnimatedLogo: true, MeshMap: true, MeshBitcoinPanel: true, MeshDeadmanPanel: true,
          MeshDevicePanel: true, MeshAssistantPanel: true, HopVizModal: true, MediaLightbox: true,
        },
      },
    })
    await flushPromises()

    expect(rejectingRefreshAll).toHaveBeenCalled()
    // The other five groups still ran to completion.
    expect(callCountFor('transport.status')).toBeGreaterThan(0)
    expect(vi.mocked(rpcClient.federationListNodes).mock.calls.length).toBeGreaterThan(0)
    expect(vi.mocked(rpcClient.getTorAddress).mock.calls.length).toBeGreaterThan(0)
    expect(vi.mocked(rpcClient.getNodeDid).mock.calls.length).toBeGreaterThan(0)
    expect(vi.mocked(rpcClient.meshContactsList).mock.calls.length).toBeGreaterThan(0)
    // The .then() callback after the fan-out (refreshOutboxCount -> mesh.outbox) still ran.
    expect(callCountFor('mesh.outbox')).toBeGreaterThan(0)

    wrapper.unmount()
  })

  it('every group carrying peer or self identity data declares persist:false; only the non-identity transport status may persist', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()

    const readSnapshot = (key: string) => {
      try {
        return sessionStorage.getItem(`resource:${key}`)
      } catch {
        return null
      }
    }
    expect(readSnapshot('mesh.refresh-all')).toBeNull() // carries mesh.peers (DIDs/pubkeys)
    expect(readSnapshot('mesh.federation-nodes')).toBeNull() // DID/pubkey/onion
    expect(readSnapshot('mesh.self-onion')).toBeNull() // this node's own onion
    expect(readSnapshot('mesh.self-did')).toBeNull() // this node's own DID
    expect(readSnapshot('mesh.contacts')).toBeNull() // contact records/aliases
    expect(readSnapshot('mesh.transport-status')).not.toBeNull() // non-identity aggregate

    wrapper.unmount()
  })

  it('every fetcher backing the six cache groups is registered with dedup:true', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()

    // Restrict to the six cache groups' own methods — other rpcClient.call
    // traffic incidental to the fan-out (e.g. mesh.outbox, refreshed from the
    // post-fan-out .then()) isn't one of the cached groups and is out of
    // scope for this assertion.
    const cachedMethods = [
      'mesh.status', 'mesh.peers', 'mesh.messages', 'mesh.deadman-status',
      'mesh.block-headers', 'transport.status',
    ]
    const dedupFlags = rpcCallMock.mock.calls
      .filter(([opts]) => cachedMethods.includes((opts as { method: string }).method))
      .map(([opts]) => (opts as { dedup?: boolean }).dedup)
    expect(dedupFlags.length).toBe(cachedMethods.length)
    expect(dedupFlags.every(Boolean)).toBe(true)

    wrapper.unmount()
  })

  it('renders RefreshIndicator wired to whether any of the six groups is refreshing', async () => {
    const wrapper = mountMeshHost()
    await flushPromises()

    const indicator = wrapper.findComponent(RefreshIndicator)
    expect(indicator.exists()).toBe(true)
    // Idle once everything has settled.
    expect(indicator.props('state')).toBe('ready')

    await toggleTab(wrapper, false)
    vi.advanceTimersByTime(15000)
    await toggleTab(wrapper, true)
    // Immediately after reactivation (before the revalidation resolves) the
    // indicator must be visible — peer reachability must never present a
    // frozen state as current without a visible refresh signal (T-02-13).
    await wrapper.vm.$nextTick()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('refreshing')

    await flushPromises()
    expect(wrapper.findComponent(RefreshIndicator).props('state')).toBe('ready')

    wrapper.unmount()
  })
})
