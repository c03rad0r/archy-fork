import { ref, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'

/**
 * Shared bitcoin sync (IBD) tracker with a live time-remaining estimate.
 *
 * Polls `bitcoin.getinfo` while at least one consumer holds an acquire()
 * lease, samples the sync rate, and exposes a ticking countdown so setup
 * screens can show "~2h 14m remaining" that visibly counts down between
 * polls. Module-level singleton — every consumer sees the same state.
 */

/** Sync fraction (as percent) at which we consider IBD done, matching the Home tile */
export const IBD_SYNCED_AT = 99.9

const POLL_MS = 15_000
const TICK_MS = 1_000
/** Ignore rate samples older than this when estimating */
const SAMPLE_WINDOW_MS = 10 * 60_000

export const bitcoinSyncPercent = ref(0)
export const bitcoinBlockHeight = ref(0)
export const bitcoinSyncAvailable = ref(false)
export const bitcoinSyncLoaded = ref(false)
export const bitcoinSynced = computed(() => bitcoinSyncLoaded.value && bitcoinSyncPercent.value >= IBD_SYNCED_AT)

const etaSeconds = ref<number | null>(null)

/** Human countdown like "2h 14m" / "5m 12s" / "less than a minute", or '' while estimating */
export const bitcoinSyncEtaText = computed(() => {
  const s = etaSeconds.value
  if (s === null) return ''
  if (s < 60) return 'less than a minute'
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h}h ${m}m`
  const sec = Math.floor(s % 60)
  return `${m}m ${sec}s`
})

let samples: { t: number; p: number }[] = []
let etaBase: { at: number; secs: number } | null = null
let pollTimer: ReturnType<typeof setInterval> | null = null
let tickTimer: ReturnType<typeof setInterval> | null = null
let leases = 0

async function poll() {
  try {
    const btc = await rpcClient.call<{ block_height: number; sync_progress: number }>({
      method: 'bitcoin.getinfo',
      timeout: 8000,
    })
    const pct = (btc.sync_progress ?? 0) * 100
    bitcoinSyncPercent.value = pct
    bitcoinBlockHeight.value = btc.block_height ?? 0
    bitcoinSyncAvailable.value = true
    bitcoinSyncLoaded.value = true

    const now = Date.now()
    samples.push({ t: now, p: pct })
    samples = samples.filter((s) => now - s.t <= SAMPLE_WINDOW_MS).slice(-50)

    if (pct >= IBD_SYNCED_AT) {
      etaBase = null
      etaSeconds.value = 0
      return
    }
    const first = samples[0]
    if (first && now - first.t >= 10_000 && pct > first.p) {
      const ratePerSec = (pct - first.p) / ((now - first.t) / 1000)
      etaBase = { at: now, secs: (IBD_SYNCED_AT - pct) / ratePerSec }
    }
  } catch {
    bitcoinSyncAvailable.value = false
  }
}

function tick() {
  if (!etaBase) {
    if (!bitcoinSynced.value) etaSeconds.value = null
    return
  }
  etaSeconds.value = Math.max(0, etaBase.secs - (Date.now() - etaBase.at) / 1000)
}

/**
 * Hold a polling lease. Returns a release function — call it on unmount.
 * Polling only runs while at least one lease is held.
 */
export function acquireBitcoinSync(): () => void {
  leases++
  if (leases === 1) {
    void poll()
    pollTimer = setInterval(() => void poll(), POLL_MS)
    tickTimer = setInterval(tick, TICK_MS)
  }
  let released = false
  return () => {
    if (released) return
    released = true
    leases = Math.max(0, leases - 1)
    if (leases === 0) {
      if (pollTimer) clearInterval(pollTimer)
      if (tickTimer) clearInterval(tickTimer)
      pollTimer = null
      tickTimer = null
    }
  }
}
