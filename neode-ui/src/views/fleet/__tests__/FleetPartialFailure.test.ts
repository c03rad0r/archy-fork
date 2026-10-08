import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { rpcClient } from '@/api/rpc-client'
import { useFleetData, normalizeFleetNode } from '../useFleetData'
import FleetAlerts from '../FleetAlerts.vue'
import FleetNodeDetail from '../FleetNodeDetail.vue'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
afterEach(() => { sessionStorage.clear(); vi.clearAllMocks() })

it.each(['Unauthorized', 'Method not found', 'Timeout'])('keeps mixed-version nodes usable when alerts fail: %s', async reason => {
  vi.mocked(rpcClient.call).mockImplementation(({ method }) => method === 'telemetry.fleet-status'
    ? Promise.resolve({ nodes: [{ node_id: 'legacy', version: 'old' }, { node_id: 'current', cpu_pct: 0, mem_pct: 0, disk_pct: 0 }] }) as never
    : Promise.reject(new Error(reason)))
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  try {
    await flushPromises()
    expect(fleet.nodes.value.map(n => n.node_id)).toEqual(['legacy', 'current'])
    expect(fleet.nodes.value[0]?.cpu_pct).toBeNull()
    expect(fleet.nodes.value[1]?.cpu_pct).toBe(0)
    expect(fleet.errorMessage.value).toBe('')
    expect(fleet.alertsErrorMessage.value).toContain('unavailable')
    expect(fleet.alertsLoading.value).toBe(false)
    const panel = mount(FleetAlerts, { props: { alerts: [], alertsLoading: false, errorMessage: fleet.alertsErrorMessage.value } })
    expect(panel.text()).not.toContain('No alerts across the fleet')
    expect(panel.get('[role="status"]').text()).toContain('unavailable')
    panel.unmount()
    vi.mocked(rpcClient.call).mockResolvedValue({ nodes: [], alerts: [] })
    await fleet.refreshAll()
    expect(fleet.alertsErrorMessage.value).toBe('')
  } finally { wrapper.unmount() }
})

it('retains previous alerts on malformed refresh without calling them current', async () => {
  const alert = { node_id: 'node', rule: 'cpu_high', message: 'Earlier alert', timestamp: '2026-10-07T12:00:00Z' }
  sessionStorage.setItem('archipelago.fleet.cache.v2', JSON.stringify({ fleetAlerts: [alert] }))
  vi.mocked(rpcClient.call).mockResolvedValue({ nodes: [], alerts: [null] })
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  try {
    await flushPromises()
    expect(fleet.fleetAlerts.value).toEqual([alert])
    expect(fleet.alertsErrorMessage.value).toContain('last received alerts')
    const panel = mount(FleetAlerts, { props: { alerts: fleet.fleetAlerts.value, alertsLoading: false, errorMessage: fleet.alertsErrorMessage.value } })
    expect(panel.text()).toContain('Earlier alert')
    expect(panel.get('[role="status"]').text()).toContain('last received alerts')
    panel.unmount()
  } finally { wrapper.unmount() }
})

it('does not apply selected-node history after unmount', async () => {
  let finish!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(({ method }) => method === 'telemetry.fleet-node-history'
    ? new Promise(done => { finish = done }) as never
    : Promise.resolve({ nodes: [], alerts: [] }) as never)
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  await flushPromises()
  fleet.selectNode('node'); await flushPromises()
  wrapper.unmount()
  finish({ entries: [{ timestamp: 'late', cpu_pct: 80 }] })
  await flushPromises()
  expect(fleet.nodeHistory.value).toEqual([])
})

it('ignores an older alerts failure after a newer successful refresh', async () => {
  let failOld!: (reason: Error) => void
  let alertCalls = 0
  vi.mocked(rpcClient.call).mockImplementation(({ method }) => {
    if (method === 'telemetry.fleet-status') return Promise.resolve({ nodes: [] }) as never
    if (++alertCalls === 1) return new Promise((_, reject) => { failOld = reject }) as never
    return Promise.resolve({ alerts: [] }) as never
  })
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  try {
    await fleet.refreshAll()
    failOld(new Error('Old permission denial'))
    await flushPromises()
    expect(fleet.alertsErrorMessage.value).toBe('')
    expect(fleet.alertsLoading.value).toBe(false)
  } finally { wrapper.unmount() }
})

it('reports explicitly unavailable history and clears the message when changing nodes', async () => {
  vi.mocked(rpcClient.call).mockImplementation(({ method, params }) => {
    if (method !== 'telemetry.fleet-node-history') return Promise.resolve({ nodes: [], alerts: [] }) as never
    return Promise.resolve((params as { node_id: string }).node_id === 'trusted'
      ? { history_available: false, entries: [{ timestamp: 'must-not-render', cpu_pct: 99 }] }
      : { entries: [{ timestamp: 'legacy-available', cpu_pct: 12 }] }) as never
  })
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  try {
    await flushPromises()
    fleet.selectNode('trusted'); await flushPromises()
    expect(fleet.nodeHistoryMessage.value).toContain('unavailable')
    expect(fleet.nodeHistory.value).toEqual([])
    const detail = mount(FleetNodeDetail, { props: {
      node: normalizeFleetNode({ node_id: 'trusted' }), nodeId: 'trusted',
      historyLoading: false, historyMessage: fleet.nodeHistoryMessage.value,
      historyLabels: [], cpuDatasets: [], memDatasets: [], diskDatasets: [], chartWidth: 300,
    }, global: { mocks: { $ver: (v: string) => v }, stubs: { LineChart: true } } })
    expect(detail.get('[role="status"]').text()).toContain('History is unavailable')
    expect(detail.text()).not.toContain('CPU History')
    detail.unmount()
    fleet.selectNode('legacy'); await flushPromises()
    expect(fleet.nodeHistoryMessage.value).toBe('')
    expect(fleet.nodeHistory.value[0]?.timestamp).toBe('legacy-available')
  } finally { wrapper.unmount() }
})
