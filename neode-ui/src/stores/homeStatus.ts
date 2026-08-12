import { defineStore } from 'pinia'
import { computed, reactive, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { PackageState, type PackageDataEntry } from '@/types/api'

type LoadState = 'idle' | 'loading' | 'ready' | 'error'

interface SystemStatsSnapshot {
  cpuPercent: number
  memUsed: number
  memTotal: number
  memPercent: number
  diskUsed: number
  diskTotal: number
  diskPercent: number
  uptimeSecs: number
  loadAvg1: number
  loadAvg5: number
  loadAvg15: number
  bitcoinSyncPercent: number
  bitcoinBlockHeight: number
  bitcoinAvailable: boolean | null
}

const emptyStats = (): SystemStatsSnapshot => ({
  cpuPercent: 0,
  memUsed: 0,
  memTotal: 0,
  memPercent: 0,
  diskUsed: 0,
  diskTotal: 0,
  diskPercent: 0,
  uptimeSecs: 0,
  loadAvg1: 0,
  loadAvg5: 0,
  loadAvg15: 0,
  bitcoinSyncPercent: 0,
  bitcoinBlockHeight: 0,
  bitcoinAvailable: null,
})

export const useHomeStatusStore = defineStore('homeStatus', () => {
  const stats = reactive<SystemStatsSnapshot>(emptyStats())
  const systemLoadState = ref<LoadState>('idle')
  const bitcoinLoadState = ref<LoadState>('idle')
  // True when we're showing a retained (last-known) bitcoin value because the
  // latest poll failed transiently — the UI renders an "Updating…" badge so the
  // figure is never presented as live, and the tile never vanishes mid-sync.
  const bitcoinStale = ref(false)
  const vpnLoadState = ref<LoadState>('idle')
  const fipsLoadState = ref<LoadState>('idle')
  const tollgateLoadState = ref<LoadState>('idle')
  const lastSystemRefreshAt = ref<number | null>(null)
  const lastBitcoinRefreshAt = ref<number | null>(null)
  const lastVpnRefreshAt = ref<number | null>(null)
  const lastFipsRefreshAt = ref<number | null>(null)
  const lastTollgateRefreshAt = ref<number | null>(null)

  const vpnStatus = ref<{
    connected: boolean | null
    provider: string
  }>({ connected: null, provider: '' })

  const fipsStatus = ref<{
    installed: boolean
    service_active: boolean
    key_present: boolean
    anchor_connected?: boolean
    authenticated_peer_count?: number
  } | null>(null)

  // null = no OpenWrt router configured at all (tile row shows "Not configured").
  const tollgateStatus = ref<{ installed: boolean; enabled: boolean } | null>(null)

  const systemStatsLoaded = computed(() => systemLoadState.value === 'ready')
  const bitcoinKnown = computed(() => stats.bitcoinAvailable !== null)
  const vpnKnown = computed(() => vpnStatus.value.connected !== null)

  async function refreshSystemStats() {
    systemLoadState.value = systemLoadState.value === 'ready' ? 'ready' : 'loading'
    try {
      const res = await rpcClient.call<{
        cpu_usage_percent: number
        mem_used_bytes: number
        mem_total_bytes: number
        disk_used_bytes: number
        disk_total_bytes: number
        uptime_secs: number
        load_avg_1?: number
        load_avg_5?: number
        load_avg_15?: number
      }>({ method: 'system.stats', dedup: true })
      stats.cpuPercent = res.cpu_usage_percent
      stats.memUsed = res.mem_used_bytes
      stats.memTotal = res.mem_total_bytes
      stats.memPercent = res.mem_total_bytes > 0 ? (res.mem_used_bytes / res.mem_total_bytes) * 100 : 0
      stats.diskUsed = res.disk_used_bytes
      stats.diskTotal = res.disk_total_bytes
      stats.diskPercent = res.disk_total_bytes > 0 ? (res.disk_used_bytes / res.disk_total_bytes) * 100 : 0
      stats.uptimeSecs = res.uptime_secs
      stats.loadAvg1 = res.load_avg_1 ?? 0
      stats.loadAvg5 = res.load_avg_5 ?? 0
      stats.loadAvg15 = res.load_avg_15 ?? 0
      systemLoadState.value = 'ready'
      lastSystemRefreshAt.value = Date.now()
    } catch {
      systemLoadState.value = stats.uptimeSecs > 0 ? 'ready' : 'error'
    }
  }

  async function refreshBitcoin(packages: Record<string, PackageDataEntry>) {
    bitcoinLoadState.value = bitcoinLoadState.value === 'ready' ? 'ready' : 'loading'
    try {
      const btc = await rpcClient.call<{ block_height: number; sync_progress: number }>({
        method: 'bitcoin.getinfo',
        timeout: 5000,
        dedup: true,
      })
      stats.bitcoinSyncPercent = (btc.sync_progress ?? 0) * 100
      stats.bitcoinBlockHeight = btc.block_height ?? 0
      stats.bitcoinAvailable = true
      bitcoinStale.value = false
      bitcoinLoadState.value = 'ready'
      lastBitcoinRefreshAt.value = Date.now()
    } catch {
      const btcPkg = packages['bitcoin-knots'] || packages['bitcoin-core'] || packages.bitcoin
      if (btcPkg?.state === PackageState.Running) {
        // Container is up but the RPC call failed (busy during heavy IBD, etc.).
        // Keep the tile visible with the last-known figures, marked as updating.
        stats.bitcoinAvailable = true
        bitcoinStale.value = true
        bitcoinLoadState.value = 'ready'
        lastBitcoinRefreshAt.value = Date.now()
        return
      }

      if (btcPkg && (btcPkg.state === PackageState.Stopped || btcPkg.state === PackageState.Exited)) {
        // Authoritatively down — reflect it (do NOT keep showing stale data as live).
        stats.bitcoinAvailable = false
        bitcoinStale.value = false
        bitcoinLoadState.value = 'ready'
        lastBitcoinRefreshAt.value = Date.now()
        return
      }

      // No authoritative package data yet. Keep the previous known value
      // rather than flashing "Not running" during route changes/scans; if we
      // had a value, surface it as "updating" instead of presenting it as live.
      if (stats.bitcoinAvailable !== null) bitcoinStale.value = true
      bitcoinLoadState.value = stats.bitcoinAvailable === null ? 'error' : 'ready'
    }
  }

  async function refreshVpn(packages: Record<string, PackageDataEntry>) {
    vpnLoadState.value = vpnLoadState.value === 'ready' ? 'ready' : 'loading'
    try {
      const status = await rpcClient.vpnStatus()
      vpnStatus.value = {
        connected: status.connected,
        provider: status.provider ?? status.configured_provider ?? '',
      }
      vpnLoadState.value = 'ready'
      lastVpnRefreshAt.value = Date.now()
    } catch {
      const tailscale = packages.tailscale
      if (tailscale?.state === PackageState.Running) {
        vpnStatus.value = { connected: true, provider: 'tailscale' }
        vpnLoadState.value = 'ready'
        lastVpnRefreshAt.value = Date.now()
        return
      }
      vpnLoadState.value = vpnStatus.value.connected === null ? 'error' : 'ready'
    }
  }

  async function refreshFips() {
    fipsLoadState.value = fipsLoadState.value === 'ready' ? 'ready' : 'loading'
    try {
      const status = await rpcClient.call<{
        installed: boolean
        service_active: boolean
        key_present: boolean
        anchor_connected?: boolean
        authenticated_peer_count?: number
      }>({ method: 'fips.status', dedup: true })
      fipsStatus.value = status
      fipsLoadState.value = 'ready'
      lastFipsRefreshAt.value = Date.now()
    } catch {
      fipsLoadState.value = fipsStatus.value ? 'ready' : 'error'
    }
  }

  async function refreshTollgate() {
    tollgateLoadState.value = tollgateLoadState.value === 'ready' ? 'ready' : 'loading'
    try {
      const res = await rpcClient.call<{ tollgate: { installed: boolean; enabled?: boolean } }>({
        method: 'openwrt.get-status',
        timeout: 15000,
        dedup: true,
      })
      tollgateStatus.value = { installed: res.tollgate.installed, enabled: res.tollgate.enabled ?? false }
      tollgateLoadState.value = 'ready'
      lastTollgateRefreshAt.value = Date.now()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (msg.includes('No router configured')) {
        // Not an error — most nodes simply don't have an OpenWrt gateway set up.
        tollgateStatus.value = null
        tollgateLoadState.value = 'ready'
        lastTollgateRefreshAt.value = Date.now()
      } else {
        // Transient failure (SSH hiccup, router rebooting) — keep last-known state.
        tollgateLoadState.value = tollgateStatus.value ? 'ready' : 'error'
      }
    }
  }

  async function refresh(packages: Record<string, PackageDataEntry>) {
    await Promise.all([
      refreshSystemStats(),
      refreshBitcoin(packages),
      refreshVpn(packages),
      refreshFips(),
      refreshTollgate(),
    ])
  }

  return {
    stats,
    systemLoadState,
    bitcoinLoadState,
    bitcoinStale,
    vpnLoadState,
    fipsLoadState,
    tollgateLoadState,
    systemStatsLoaded,
    bitcoinKnown,
    vpnKnown,
    vpnStatus,
    fipsStatus,
    tollgateStatus,
    lastSystemRefreshAt,
    lastBitcoinRefreshAt,
    lastVpnRefreshAt,
    lastFipsRefreshAt,
    lastTollgateRefreshAt,
    refresh,
    refreshSystemStats,
    refreshBitcoin,
    refreshVpn,
    refreshFips,
    refreshTollgate,
  }
})
