<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useStatsStore } from "../stores/stats";
import type { HistoryPoint } from "../types";
import Shameboard from "../components/Shameboard.vue";
import MinerRace from "../components/MinerRace.vue";

const stats = useStatsStore();

onMounted(() => stats.start());
onUnmounted(() => stats.stop());

const ranges = [
  { label: "15m", ms: 15 * 60 * 1000 },
  { label: "1h", ms: 60 * 60 * 1000 },
  { label: "6h", ms: 6 * 60 * 60 * 1000 },
  { label: "24h", ms: 24 * 60 * 60 * 1000 },
] as const;

const selectedRange = ref<(typeof ranges)[number]["label"]>("1h");
const activeRange = computed(() => ranges.find((r) => r.label === selectedRange.value) ?? ranges[1]);
const now = computed(() => stats.snapshot?.fetchedAt ?? Date.now());
const points = computed(() => {
  const minT = now.value - activeRange.value.ms;
  const filtered = stats.history.filter((p) => p.t >= minT);
  return filtered.length > 0 ? filtered : stats.history;
});

const current = computed(() => stats.snapshot);
const currentThs = computed(() => current.value?.pool.combinedHashrateThs ?? 0);
const activeMiners = computed(() => current.value?.miners.filter((m) => m.status === "hashing").length ?? 0);
const windowLabel = computed(() => {
  const first = points.value[0];
  const last = points.value.at(-1);
  if (!first || !last) return "collecting";
  const minutes = Math.max(0, Math.round((last.t - first.t) / 60000));
  if (minutes < 60) return `${minutes}m captured`;
  return `${(minutes / 60).toFixed(1)}h captured`;
});

const deltas = computed(() => {
  const first = points.value[0];
  const last = points.value.at(-1);
  if (!first || !last) return { accepted: 0, rejected: 0, total: 0, rejectPct: 0 };
  const accepted = Math.max(0, last.accepted - first.accepted);
  const rejected = Math.max(0, last.rejected - first.rejected);
  const total = accepted + rejected;
  return { accepted, rejected, total, rejectPct: total > 0 ? (rejected / total) * 100 : 0 };
});

const avgThs = computed(() => {
  if (!points.value.length) return 0;
  return points.value.reduce((sum, p) => sum + p.ths, 0) / points.value.length;
});

const highLow = computed(() => {
  if (!points.value.length) return { high: 0, low: 0 };
  return {
    high: Math.max(...points.value.map((p) => p.ths)),
    low: Math.min(...points.value.map((p) => p.ths)),
  };
});

const shareRate = computed(() => {
  const first = points.value[0];
  const last = points.value.at(-1);
  if (!first || !last || last.t <= first.t) return 0;
  return deltas.value.accepted / ((last.t - first.t) / 3600000);
});

const hashratePath = computed(() => linePath(points.value.map((p) => ({ x: p.t, y: p.ths })), 460, 190));
const shareBars = computed(() => buildShareBuckets(points.value, activeRange.value.ms));
const minerRows = computed(() => {
  const latest = current.value?.miners ?? [];
  return latest
    .map((m) => {
      const key = m.authUsername || m.remoteHost || m.nickname;
      const average = averageMinerHashrate(points.value, key);
      const bestWork = bestObservedWork(stats.history, key);
      const pct = currentThs.value > 0 ? (m.hashrateThs / currentThs.value) * 100 : 0;
      return { key, name: m.nickname, model: m.model, current: m.hashrateThs, average, bestWork, pct, status: m.status };
    })
    .sort((a, b) => b.current - a.current);
});

function linePath(items: { x: number; y: number }[], width: number, height: number): string {
  if (items.length < 2) return "";
  const first = items[0];
  if (!first) return "";
  const minX = first.x;
  const maxX = items.at(-1)?.x ?? minX + 1;
  const minY = Math.min(...items.map((i) => i.y), 0);
  const maxY = Math.max(...items.map((i) => i.y), 1);
  const spanX = Math.max(1, maxX - minX);
  const spanY = Math.max(0.001, maxY - minY);
  return items
    .map((item, idx) => {
      const x = ((item.x - minX) / spanX) * width;
      const y = height - ((item.y - minY) / spanY) * height;
      return `${idx === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function buildShareBuckets(history: HistoryPoint[], rangeMs: number): { x: number; accepted: number; rejected: number; h: number }[] {
  if (history.length < 2) return [];
  const bucketCount = rangeMs <= 60 * 60 * 1000 ? 30 : 48;
  const first = history[0];
  if (!first) return [];
  const start = first.t;
  const end = history.at(-1)?.t ?? start + 1;
  const span = Math.max(1, end - start);
  const buckets = Array.from({ length: bucketCount }, (_, i) => ({ x: i, accepted: 0, rejected: 0, h: 0 }));
  for (let i = 1; i < history.length; i += 1) {
    const prev = history[i - 1];
    const next = history[i];
    if (!prev || !next) continue;
    const idx = Math.min(bucketCount - 1, Math.max(0, Math.floor(((next.t - start) / span) * bucketCount)));
    const bucket = buckets[idx];
    if (!bucket) continue;
    bucket.accepted += Math.max(0, next.accepted - prev.accepted);
    bucket.rejected += Math.max(0, next.rejected - prev.rejected);
  }
  const max = Math.max(...buckets.map((b) => b.accepted + b.rejected), 1);
  return buckets.map((b) => ({ ...b, h: ((b.accepted + b.rejected) / max) * 72 }));
}

function averageMinerHashrate(history: HistoryPoint[], key: string): number {
  const values = history.map((p) => p.miners[key]).filter((v): v is number => Number.isFinite(v));
  if (!values.length) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function bestObservedWork(history: HistoryPoint[], key: string): number {
  let best = 0;
  for (let i = 1; i < history.length; i += 1) {
    const prev = history[i - 1];
    const next = history[i];
    if (!prev?.minerWork || !next?.minerWork || !prev.minerShares || !next.minerShares) continue;
    const shareDelta = (next.minerShares[key] ?? 0) - (prev.minerShares[key] ?? 0);
    const workDelta = (next.minerWork[key] ?? 0) - (prev.minerWork[key] ?? 0);
    if (shareDelta > 0 && workDelta > 0) best = Math.max(best, workDelta / shareDelta);
  }
  return best;
}

function fmt(n: number, digits = 2): string {
  return n.toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits });
}
</script>

<template>
  <section class="graph-shell">
    <header class="graph-head">
      <div>
        <div class="label">graph view</div>
        <h1 class="glow-cyan">Hashrate telemetry</h1>
      </div>
      <div class="range-tabs">
        <button
          v-for="range in ranges"
          :key="range.label"
          :class="{ active: selectedRange === range.label }"
          @click="selectedRange = range.label"
        >
          {{ range.label }}
        </button>
      </div>
    </header>

    <MinerRace :snapshot="stats.snapshot" :history="stats.history" />

    <div class="metric-grid">
      <div class="metric panel">
        <div class="label">now</div>
        <strong class="glow-cyan">{{ fmt(currentThs) }}</strong>
        <span>Th/s</span>
      </div>
      <div class="metric panel">
        <div class="label">window avg</div>
        <strong>{{ fmt(avgThs) }}</strong>
        <span>Th/s · {{ windowLabel }}</span>
      </div>
      <div class="metric panel">
        <div class="label">accepted rate</div>
        <strong class="glow-green">{{ fmt(shareRate, 1) }}</strong>
        <span>shares/hour</span>
      </div>
      <div class="metric panel">
        <div class="label">rejects</div>
        <strong :class="deltas.rejectPct > 3 ? 'glow-red' : ''">{{ fmt(deltas.rejectPct, 2) }}%</strong>
        <span>{{ deltas.rejected.toLocaleString() }} / {{ deltas.total.toLocaleString() }}</span>
      </div>
      <div class="metric panel">
        <div class="label">active miners</div>
        <strong>{{ activeMiners }}</strong>
        <span>{{ current?.pool.totalSubscriptions ?? 0 }} subscriptions</span>
      </div>
    </div>

    <div class="chart-grid">
      <section class="panel chart-card wide">
        <div class="chart-title">
          <div>
            <div class="label">combined hashrate</div>
            <strong>{{ fmt(highLow.low) }} → {{ fmt(highLow.high) }} Th/s</strong>
          </div>
          <span class="muted">{{ points.length.toLocaleString() }} samples</span>
        </div>
        <svg class="line-chart" viewBox="0 0 460 190" preserveAspectRatio="none">
          <defs>
            <linearGradient id="hashFill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stop-color="var(--neon-cyan)" stop-opacity="0.22" />
              <stop offset="100%" stop-color="var(--neon-cyan)" stop-opacity="0" />
            </linearGradient>
          </defs>
          <path v-if="hashratePath" :d="`${hashratePath} L460,190 L0,190 Z`" fill="url(#hashFill)" />
          <path v-if="hashratePath" :d="hashratePath" class="hash-line" />
          <text v-else x="230" y="96" text-anchor="middle">collecting samples</text>
        </svg>
      </section>

      <section class="panel chart-card">
        <div class="chart-title">
          <div>
            <div class="label">share flow</div>
            <strong>{{ deltas.accepted.toLocaleString() }} accepted</strong>
          </div>
          <span class="muted">{{ deltas.rejected.toLocaleString() }} rejected</span>
        </div>
        <div class="bars">
          <div v-for="bar in shareBars" :key="bar.x" class="bar-track">
            <span class="bar accepted" :style="{ height: `${bar.h}px` }" />
            <span
              v-if="bar.rejected"
              class="bar rejected"
              :style="{ height: `${Math.max(4, (bar.rejected / Math.max(1, bar.accepted + bar.rejected)) * bar.h)}px` }"
            />
          </div>
        </div>
      </section>

      <section class="panel chart-card">
        <div class="chart-title">
          <div>
            <div class="label">miner mix</div>
            <strong>{{ minerRows.length }} devices</strong>
          </div>
        </div>
        <div class="miner-mix">
          <div v-for="miner in minerRows" :key="miner.key" class="miner-row">
            <div class="miner-meta">
              <span :class="['status-dot', miner.status]" />
              <strong>{{ miner.name }}</strong>
              <span class="muted">{{ fmt(miner.current) }} Th/s</span>
            </div>
            <div class="mix-track">
              <span :style="{ width: `${Math.min(100, miner.pct)}%` }" />
            </div>
            <div class="muted avg">
              avg {{ fmt(miner.average) }} Th/s · best {{ miner.bestWork > 0 ? fmt(miner.bestWork) : "collecting" }}
            </div>
          </div>
        </div>
      </section>
    </div>

    <Shameboard :snapshot="stats.snapshot" :history="stats.history" />
  </section>
</template>

<style scoped>
.graph-shell {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.graph-head,
.chart-title,
.miner-meta {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}
h1 {
  margin: 2px 0 0;
  font-size: 28px;
  letter-spacing: 0;
}
.range-tabs {
  display: flex;
  gap: 8px;
}
.range-tabs button {
  min-width: 52px;
  padding: 7px 10px;
}
.range-tabs button.active {
  border-color: var(--neon-magenta);
  color: var(--neon-magenta);
  box-shadow: var(--shadow-magenta);
}
.metric-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 12px;
}
.metric {
  min-height: 104px;
}
.metric strong {
  display: block;
  margin-top: 8px;
  font-size: 24px;
  line-height: 1.1;
}
.metric span {
  display: block;
  margin-top: 6px;
  color: var(--fg-2);
  font-size: 11px;
}
.chart-grid {
  display: grid;
  grid-template-columns: 3fr 2fr;
  gap: 16px;
}
.chart-card {
  min-height: 260px;
  overflow: hidden;
}
.chart-card.wide {
  grid-column: 1 / -1;
}
.line-chart {
  width: 100%;
  height: 300px;
  margin-top: 12px;
  border-bottom: 1px solid var(--line);
  background:
    linear-gradient(to bottom, transparent 24%, rgba(255,255,255,0.04) 25%, transparent 26%),
    linear-gradient(to bottom, transparent 49%, rgba(255,255,255,0.04) 50%, transparent 51%),
    linear-gradient(to bottom, transparent 74%, rgba(255,255,255,0.04) 75%, transparent 76%);
}
.hash-line {
  fill: none;
  stroke: var(--neon-cyan);
  stroke-width: 2.5;
  vector-effect: non-scaling-stroke;
  filter: drop-shadow(0 0 5px rgba(41, 255, 230, 0.45));
}
.line-chart text {
  fill: var(--fg-2);
  font-size: 12px;
}
.bars {
  height: 170px;
  display: grid;
  grid-template-columns: repeat(48, 1fr);
  gap: 3px;
  align-items: end;
  margin-top: 34px;
  border-bottom: 1px solid var(--line);
}
.bar-track {
  height: 90px;
  position: relative;
  display: flex;
  align-items: end;
}
.bar {
  width: 100%;
  min-height: 2px;
  display: block;
}
.bar.accepted {
  background: var(--neon-green);
  box-shadow: 0 0 8px rgba(108, 255, 140, 0.25);
}
.bar.rejected {
  position: absolute;
  bottom: 0;
  background: var(--neon-red);
}
.miner-mix {
  display: flex;
  flex-direction: column;
  gap: 14px;
  margin-top: 18px;
}
.miner-row {
  display: grid;
  gap: 6px;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fg-2);
  box-shadow: 0 0 8px currentColor;
}
.status-dot.hashing { background: var(--neon-green); }
.status-dot.stale { background: var(--neon-amber); }
.status-dot.idle { background: var(--neon-red); }
.mix-track {
  height: 8px;
  background: var(--bg-2);
  border: 1px solid var(--line);
}
.mix-track span {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, var(--neon-magenta), var(--neon-cyan));
}
.avg {
  font-size: 10px;
}
@media (max-width: 1000px) {
  .metric-grid {
    grid-template-columns: repeat(2, 1fr);
  }
  .chart-grid {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 620px) {
  .graph-head {
    align-items: flex-start;
    flex-direction: column;
  }
  .range-tabs {
    width: 100%;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
  }
  .range-tabs button {
    min-width: 0;
  }
  .metric-grid {
    grid-template-columns: 1fr;
  }
  .line-chart {
    height: 220px;
  }
}
</style>
