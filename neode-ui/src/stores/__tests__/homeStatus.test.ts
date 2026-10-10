import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

// Mock the rpc-client module
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
    vpnStatus: vi.fn(),
  },
}))

import { useHomeStatusStore } from '../homeStatus'
import { rpcClient } from '@/api/rpc-client'
import { PackageState, type PackageDataEntry } from '@/types/api'

const mockedRpc = vi.mocked(rpcClient)

function pkg(state: string): Record<string, PackageDataEntry> {
  return { 'bitcoin-knots': { state } as unknown as PackageDataEntry }
}

describe('homeStatus — B16 bitcoin sync status retain (no vanish, no stale-as-live)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('records a successful poll as available + not stale', async () => {
    const store = useHomeStatusStore()
    mockedRpc.call.mockResolvedValueOnce({ block_height: 800000, sync_progress: 1 })
    await store.refreshBitcoin({})
    expect(store.stats.bitcoinAvailable).toBe(true)
    expect(store.stats.bitcoinSyncPercent).toBe(100)
    expect(store.bitcoinStale).toBe(false)
    expect(store.bitcoinLoadState).toBe('ready')
  })

  it('keeps the tile visible (available) but marks stale when getinfo fails while the container is Running', async () => {
    const store = useHomeStatusStore()
    // First a good poll so we have real sync numbers.
    mockedRpc.call.mockResolvedValueOnce({ block_height: 800000, sync_progress: 0.5 })
    await store.refreshBitcoin(pkg(PackageState.Running))
    expect(store.stats.bitcoinAvailable).toBe(true)

    // Now a transient RPC failure (e.g. RPC busy during heavy IBD) — container still Running.
    mockedRpc.call.mockRejectedValueOnce(new Error('timeout'))
    await store.refreshBitcoin(pkg(PackageState.Running))
    expect(store.stats.bitcoinAvailable).toBe(true) // does NOT vanish
    expect(store.bitcoinStale).toBe(true) // shown as "Updating…", not live
    expect(store.stats.bitcoinSyncPercent).toBe(50) // last-known retained
  })

  it('flips to NOT available (and not stale) when getinfo fails and the container is Stopped — no stale-as-live', async () => {
    const store = useHomeStatusStore()
    mockedRpc.call.mockResolvedValueOnce({ block_height: 800000, sync_progress: 1 })
    await store.refreshBitcoin(pkg(PackageState.Running))
    expect(store.stats.bitcoinAvailable).toBe(true)

    mockedRpc.call.mockRejectedValueOnce(new Error('refused'))
    await store.refreshBitcoin(pkg(PackageState.Stopped))
    expect(store.stats.bitcoinAvailable).toBe(false) // genuinely down → reflect it
    expect(store.bitcoinStale).toBe(false) // not "Updating…": it's authoritatively stopped
  })

  it('retains the last-known available value (marked stale) when package data is momentarily absent', async () => {
    const store = useHomeStatusStore()
    mockedRpc.call.mockResolvedValueOnce({ block_height: 800000, sync_progress: 1 })
    await store.refreshBitcoin(pkg(PackageState.Running))
    expect(store.stats.bitcoinAvailable).toBe(true)

    // getinfo fails AND the packages map has no authoritative bitcoin entry (route change / scan).
    mockedRpc.call.mockRejectedValueOnce(new Error('timeout'))
    await store.refreshBitcoin({})
    expect(store.stats.bitcoinAvailable).toBe(true) // retained, does NOT flash "Not running"
    expect(store.bitcoinStale).toBe(true)
    expect(store.bitcoinLoadState).toBe('ready')
  })

  it('stays unknown (null) without fabricating availability when the first ever poll fails with no package data', async () => {
    const store = useHomeStatusStore()
    mockedRpc.call.mockRejectedValueOnce(new Error('timeout'))
    await store.refreshBitcoin({})
    expect(store.stats.bitcoinAvailable).toBeNull() // nothing known yet — don't invent a tile
    expect(store.bitcoinLoadState).toBe('error')
  })

  it('marks bitcoin available + stale when the first poll times out but the container is Running (syncing node)', async () => {
    const store = useHomeStatusStore()
    // No prior success; getinfo times out during heavy initial sync, but container is up.
    mockedRpc.call.mockRejectedValueOnce(new Error('timeout'))
    await store.refreshBitcoin(pkg(PackageState.Running))
    expect(store.stats.bitcoinAvailable).toBe(true) // tile appears instead of staying hidden
    expect(store.bitcoinStale).toBe(true) // labeled "Updating…" since we have no live numbers yet
  })
})

describe('homeStatus — live disk capacity (dedup disabled for system.stats)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('issues a fresh system.stats request on every refresh instead of a deduplicated snapshot', async () => {
    const store = useHomeStatusStore()
    mockedRpc.call.mockResolvedValue({
      cpu_usage_percent: 5,
      mem_used_bytes: 1,
      mem_total_bytes: 2,
      disk_used_bytes: 1,
      disk_total_bytes: 2,
      uptime_secs: 60,
    })

    await store.refreshSystemStats()
    await store.refreshSystemStats()

    const statsCalls = mockedRpc.call.mock.calls.filter(
      ([opts]) => (opts as { method?: string }).method === 'system.stats'
    )
    expect(statsCalls).toHaveLength(2)
    for (const [opts] of statsCalls) {
      // Persisted/deduplicated snapshots can keep showing the pre-cleanup
      // volume for minutes after files are removed; capacity must be live.
      expect((opts as { dedup?: boolean }).dedup).toBe(false)
    }
    expect(store.stats.diskUsed).toBe(1)
    expect(store.stats.diskTotal).toBe(2)
  })
})
