import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import FleetNodeGrid from '../FleetNodeGrid.vue'
import FleetOverviewCards from '../FleetOverviewCards.vue'
import { normalizeFleetNode } from '../useFleetData'

describe('fleet unavailable readings', () => {
  afterEach(() => vi.useRealTimers())
  it('shows an unavailable reading differently from a real zero and explains offline status', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-10T12:00:00Z'))
    const nodes = [normalizeFleetNode({node_id: 'offline', reported_at: '2026-06-08T12:00:00Z', cpu_pct: 0}), normalizeFleetNode({node_id: 'unknown'})]
    const wrapper = mount(FleetNodeGrid, {
      props: {nodes, sortedNodes: nodes, sortBy: 'status', selectedNodeId: null},
      global: {mocks: {$ver: (v: string) => v}},
    })
    const cards = wrapper.findAll('.fleet-node-card')
    expect(cards).toHaveLength(2)
    expect(cards[0]!.text()).toContain('0%')
    expect(cards[0]!.text()).toContain('—')
    expect(cards[0]!.text()).toContain('Offline · last seen 2d ago')
    expect(cards[1]!.text()).toContain('Status unknown')
    expect(cards[1]!.text()).not.toContain('0%')
    expect(cards[1]!.text()).toContain('Uptime unavailable')
    expect(wrapper.text()).not.toContain('NaN')
    wrapper.unmount()
  })
  it('renders empty averages without claiming zero load', () => {
    const wrapper = mount(FleetOverviewCards, {props: {
      nodeCount: 1, onlineCount: 0, offlineCount: 0, unknownCount: 1,
      fleetHealthPct: 0, healthyCount: 0, avgCpu: null, avgMem: null, avgDisk: null,
    }})
    expect(wrapper.text()).toContain('1 unknown')
    expect(wrapper.findAll('.monitoring-stat-card').slice(2).map(card => card.findAll('p').map(p => p.text()).join(' '))).toEqual([
      'Avg CPU — reporting online nodes', 'Avg RAM — reporting online nodes', 'Avg Disk — reporting online nodes',
    ])
    wrapper.unmount()
  })
})
