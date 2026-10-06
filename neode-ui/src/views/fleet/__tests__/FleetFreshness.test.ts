import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { rpcClient } from '@/api/rpc-client'
import { useFleetData } from '../useFleetData'

vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
afterEach(() => { vi.useRealTimers(); sessionStorage.clear(); vi.clearAllMocks() })

it('ages cached reports while refresh is paused and cleans up its clock', async () => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-10-06T12:00:00Z'))
  sessionStorage.setItem('archipelago.fleet.cache.v2', JSON.stringify({ nodes: [{
    node_id: 'node', reported_at: '2026-10-06T11:30:10Z', cpu_pct: 20, mem_pct: 30, disk_pct: 40,
  }] }))
  vi.mocked(rpcClient.call).mockRejectedValue(new Error('Offline'))
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  await flushPromises()
  expect(fleet.onlineCount.value).toBe(1)
  expect(fleet.avgCpu.value).toBe(20)
  fleet.toggleAutoRefresh()
  const calls = vi.mocked(rpcClient.call).mock.calls.length
  await vi.advanceTimersByTimeAsync(15_000)
  expect(fleet.onlineCount.value).toBe(0)
  expect(fleet.offlineCount.value).toBe(1)
  expect(fleet.avgCpu.value).toBeNull()
  expect(rpcClient.call).toHaveBeenCalledTimes(calls)
  wrapper.unmount()
  expect(vi.getTimerCount()).toBe(0)
})

it('does not rearm timers when requests finish after unmount', async () => {
  vi.useFakeTimers()
  const intervals = vi.spyOn(globalThis, 'setInterval')
  const clear = vi.spyOn(globalThis, 'clearInterval')
  const completions: Array<(value: unknown) => void> = []
  vi.mocked(rpcClient.call).mockImplementation(() => new Promise(done => { completions.push(done) }) as never)
  const wrapper = mount(defineComponent({ setup() { useFleetData(); return () => null } }))
  const started = intervals.mock.results.map(result => result.value)
  wrapper.unmount()
  for (const timer of started) expect(clear).toHaveBeenCalledWith(timer)
  const intervalCalls = intervals.mock.calls.length
  completions.forEach(done => done({ nodes: [], alerts: [] }))
  await flushPromises()
  expect(intervals).toHaveBeenCalledTimes(intervalCalls)
  intervals.mockRestore(); clear.mockRestore()
})

it('keeps a late node history response out of the newly selected node', async () => {
  const history = new Map<string, (value: unknown) => void>()
  vi.mocked(rpcClient.call).mockImplementation(({ method, params }) => {
    if (method === 'telemetry.fleet-node-history') return new Promise(done => { history.set((params as { node_id: string }).node_id, done) }) as never
    return Promise.resolve({ nodes: [], alerts: [] }) as never
  })
  let fleet!: ReturnType<typeof useFleetData>
  const wrapper = mount(defineComponent({ setup() { fleet = useFleetData(); return () => null } }))
  await flushPromises()
  fleet.selectNode('first'); await flushPromises()
  fleet.selectNode('second'); await flushPromises()
  history.get('first')!({ history: [{ timestamp: 'old', cpu_pct: 99 }] })
  await flushPromises()
  expect(fleet.nodeHistory.value).toEqual([])
  expect(fleet.nodeHistoryLoading.value).toBe(true)
  history.get('second')!({ history: [{ timestamp: 'current', cpu_pct: 10 }] })
  await flushPromises()
  expect(fleet.nodeHistory.value[0]?.timestamp).toBe('current')
  expect(fleet.nodeHistoryLoading.value).toBe(false)
  wrapper.unmount()
})
