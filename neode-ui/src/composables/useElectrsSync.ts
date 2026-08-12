import { ref, computed, onUnmounted } from 'vue'

/** Shape of GET /electrs-status (see core electrs_status.rs ElectrsSyncStatus). */
export interface ElectrsSyncStatus {
  indexed_height: number
  bitcoin_height: number
  network_height: number
  progress_pct: number
  status: string // "starting" | "waiting" | "syncing" | "indexing" | "synced" | "error"
  stale: boolean
  error: string | null
  index_size: string | null
  tor_onion: string | null
}

/**
 * Polls GET /electrs-status while active. Used to show an ElectrumX sync screen
 * *before* the real Electrum UI — the Electrum server only accepts client
 * connections (and the UI only works) once the on-chain index is built, which
 * is a long initial process.
 *
 * Fails OPEN: if the status can't be fetched we report not-syncing, so a status
 * outage never blocks the normal iframe path. We only gate the UI when we
 * positively know the index is still being built (status !== "synced").
 */
export function useElectrsSync() {
  const status = ref<ElectrsSyncStatus | null>(null)
  let timer: ReturnType<typeof setInterval> | null = null

  async function poll() {
    try {
      const res = await fetch('/electrs-status', { cache: 'no-store' })
      if (res.ok) status.value = (await res.json()) as ElectrsSyncStatus
    } catch {
      /* keep last known value; fail open */
    }
  }

  function start() {
    if (timer) return
    void poll()
    timer = setInterval(() => void poll(), 8000)
  }

  function stop() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  /** True only when we know ElectrumX is still building its index. */
  const syncing = computed(() => !!status.value && status.value.status !== 'synced')

  onUnmounted(stop)
  return { status, syncing, start, stop }
}
