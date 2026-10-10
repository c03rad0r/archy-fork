// Focused regression for the 2026-10-10 disk display change: Monitoring's
// disk card must show FREE capacity (live, post-cleanup accurate) rather than
// the used/total pair, with TB-scale formatting.

import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

import Monitoring from '@/views/Monitoring.vue'
import { rpcClient } from '@/api/rpc-client'

const mockedRpc = vi.mocked(rpcClient)
const TB = 1024 ** 4
const GB = 1024 ** 3

beforeEach(() => {
  vi.clearAllMocks()
  mockedRpc.call.mockImplementation(async ({ method }: { method: string }) => {
    switch (method) {
      case 'system.stats':
        return {
          cpu_usage_percent: 5,
          mem_used_bytes: 1,
          mem_total_bytes: 2,
          disk_used_bytes: 512 * GB,
          disk_total_bytes: 2 * TB,
          uptime_secs: 60,
          load_avg_1: 0.1,
          load_avg_5: 0.1,
          load_avg_15: 0.1,
        }
      case 'monitoring.current':
        return {
          timestamp: 0,
          rpc_latency_ms: 1,
          ws_connections: 0,
          system: {
            cpu_percent: 5,
            mem_used_bytes: 1,
            mem_total_bytes: 2,
            disk_used_bytes: 512 * GB,
            disk_total_bytes: 2 * TB,
            net_rx_bytes: 0,
            net_tx_bytes: 0,
            load_avg_1: 0.1,
            load_avg_5: 0.1,
            load_avg_15: 0.1,
          },
          containers: [],
        }
      case 'monitoring.history':
        return { resolution: 'minute', count: 0, data: [] }
      case 'monitoring.alerts':
        return { alerts: [] }
      case 'monitoring.alert-rules':
        return { rules: [] }
      default:
        return {}
    }
  })
})

describe('Monitoring disk display', () => {
  it('shows free capacity with TB units instead of used/total', async () => {
    const wrapper = mount(Monitoring, {
      global: {
        plugins: [createPinia()],
        stubs: { LineChart: true, ToggleSwitch: true, BackButton: true },
      },
    })
    await flushPromises()
    expect(wrapper.text()).toContain('1.5 TB free / 2.0 TB')
    expect(wrapper.text()).not.toContain('512 GB / 2.0 TB')
    wrapper.unmount()
  })
})
