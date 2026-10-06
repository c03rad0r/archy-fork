import { defineStore } from "pinia";
import { ref } from "vue";
import * as api from "../services/api";
import type { DatumSnapshot, HistoryPoint } from "../types";

const POLL_MS = 5000;
const HISTORY_KEY = "gashboard.history.v1";
const MAX_HISTORY_AGE_MS = 24 * 60 * 60 * 1000;
const MAX_HISTORY_POINTS = 18_000;

function loadHistory(): HistoryPoint[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as HistoryPoint[];
    if (!Array.isArray(parsed)) return [];
    const minT = Date.now() - MAX_HISTORY_AGE_MS;
    return parsed.filter((p) => Number.isFinite(p.t) && p.t >= minT);
  } catch {
    return [];
  }
}

function asHistoryPoint(snap: DatumSnapshot): HistoryPoint {
  const miners: Record<string, number> = {};
  const minerWork: Record<string, number> = {};
  const minerShares: Record<string, number> = {};
  const minerStatus: Record<string, DatumSnapshot["miners"][number]["status"]> = {};
  for (const miner of snap.miners) {
    const key = miner.authUsername || miner.remoteHost || miner.nickname;
    miners[key] = miner.hashrateThs;
    minerWork[key] = miner.diffAcceptedSum;
    minerShares[key] = miner.diffAcceptedCount;
    minerStatus[key] = miner.status;
  }
  return {
    t: snap.fetchedAt,
    ths: snap.pool.combinedHashrateThs,
    accepted: snap.pool.sharesAccepted,
    rejected: snap.pool.sharesRejected,
    connections: snap.pool.totalConnections,
    subscriptions: snap.pool.totalSubscriptions,
    height: snap.job.blockHeight,
    miners,
    minerWork,
    minerShares,
    minerStatus,
  };
}

export const useStatsStore = defineStore("stats", () => {
  const snapshot = ref<DatumSnapshot | null>(null);
  const history = ref<HistoryPoint[]>(loadHistory());
  const error = ref<string | null>(null);
  const loading = ref(false);
  let timer: number | null = null;
  let persistTimer: number | null = null;

  function remember(snap: DatumSnapshot): void {
    if (!snap.ok) return;
    const last = history.value.at(-1);
    if (last?.t === snap.fetchedAt) return;
    history.value.push(asHistoryPoint(snap));
    const minT = Date.now() - MAX_HISTORY_AGE_MS;
    history.value = history.value.filter((p) => p.t >= minT).slice(-MAX_HISTORY_POINTS);
    if (persistTimer !== null) window.clearTimeout(persistTimer);
    persistTimer = window.setTimeout(() => {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value));
      persistTimer = null;
    }, 250);
  }

  async function fetchOnce(): Promise<void> {
    try {
      loading.value = true;
      const next = await api.fetchSnapshot();
      snapshot.value = next;
      remember(next);
      error.value = null;
    } catch (e) {
      error.value = e instanceof Error ? e.message : "Fetch failed";
    } finally {
      loading.value = false;
    }
  }

  function start(): void {
    if (timer !== null) return;
    void fetchOnce();
    timer = window.setInterval(() => void fetchOnce(), POLL_MS);
  }

  function stop(): void {
    if (timer !== null) {
      window.clearInterval(timer);
      timer = null;
    }
  }

  return { snapshot, history, error, loading, fetchOnce, start, stop };
});
