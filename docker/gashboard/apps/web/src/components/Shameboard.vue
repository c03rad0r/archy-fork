<script setup lang="ts">
import { computed } from "vue";
import type { DatumSnapshot, HistoryPoint, MinerStat } from "../types";
import { multiple, tinyPercentAsOdds } from "../format";

const props = defineProps<{ snapshot: DatumSnapshot | null; history: HistoryPoint[] }>();

const HASHES_PER_TH = 1e12;
const DIFF_ONE_HASHES = 2 ** 32;
const BLOCKS_PER_DAY = 144;
const DAYS_PER_YEAR = 365;
const SUBSIDY_SATS = 312_500_000;

const miners = computed(() => props.snapshot?.miners ?? []);
const totalThs = computed(() => props.snapshot?.pool.combinedHashrateThs ?? 0);
const networkEh = computed(() => props.snapshot?.network.hashrateEh ?? 0);
const networkMultiple = computed(() => networkEh.value > 0 && totalThs.value > 0 ? (networkEh.value * 1_000_000) / totalThs.value : 0);
const totalWatts = computed(() => miners.value.reduce((sum, m) => sum + m.watts, 0));
const rewardSats = computed(() => props.snapshot?.job.blockValueSats || SUBSIDY_SATS);
const difficulty = computed(() => props.snapshot?.job.difficulty || 0);
const bestByMiner = computed(() => bestWorkByMiner(props.history));
const poolBest = computed(() => Math.max(0, ...Object.values(bestByMiner.value)));
const windowHours = computed(() => {
  const first = props.history[0];
  const last = props.history.at(-1);
  if (!first || !last || last.t <= first.t) return 0;
  return (last.t - first.t) / 3600000;
});
const shareVelocity = computed(() => {
  const first = props.history[0];
  const last = props.history.at(-1);
  if (!first || !last || windowHours.value <= 0) return 0;
  return Math.max(0, last.accepted - first.accepted) / windowHours.value;
});
const expectedSecondsToBlock = computed(() => {
  if (!difficulty.value || totalThs.value <= 0) return 0;
  return (difficulty.value * DIFF_ONE_HASHES) / (totalThs.value * HASHES_PER_TH);
});
const expectedBlocksYear = computed(() => expectedSecondsToBlock.value > 0 ? (DAYS_PER_YEAR * 86400) / expectedSecondsToBlock.value : 0);
const expectedSatsDay = computed(() => expectedBlocksYear.value > 0 ? (expectedBlocksYear.value / DAYS_PER_YEAR) * rewardSats.value : 0);
const heatBtu = computed(() => totalWatts.value * 3.412);
const poolEfficiency = computed(() => totalWatts.value > 0 ? totalThs.value / totalWatts.value : 0);
const connectionHealth = computed(() => {
  const pool = props.snapshot?.pool;
  if (!pool) return "collecting";
  if (pool.totalSubscriptions < miners.value.length) return "missing subscriptions";
  if (miners.value.some((m) => m.status !== "hashing")) return "somebody wandered off";
  if (pool.activeThreads <= 0) return "threads unknown";
  return "nominal, suspiciously";
});

const minerRows = computed(() => {
  return miners.value
    .map((m) => {
      const key = minerKey(m);
      const samples = props.history.map((p) => p.miners[key]).filter((v): v is number => Number.isFinite(v));
      const avg = average(samples);
      const sd = stddev(samples, avg);
      const dominance = totalThs.value > 0 ? (m.hashrateThs / totalThs.value) * 100 : 0;
      const expectedPct = m.expectedHashrateThs > 0 ? (m.hashrateThs / m.expectedHashrateThs) * 100 : 0;
      const idlePct = statusPct(props.history, key, "idle") + statusPct(props.history, key, "stale");
      const best = bestByMiner.value[key] ?? 0;
      return {
        key,
        miner: m,
        avg,
        high: samples.length ? Math.max(...samples) : 0,
        low: samples.length ? Math.min(...samples) : 0,
        sd,
        dominance,
        expectedPct,
        idlePct,
        best,
        bestDifficultyPct: difficulty.value > 0 ? (best / difficulty.value) * 100 : 0,
        thPerWatt: m.watts > 0 ? m.hashrateThs / m.watts : 0,
        heatBtu: m.watts * 3.412,
        sharesPerHour: minerShareVelocity(props.history, key),
        line: personaLine(m, dominance, expectedPct, sd, idlePct),
      };
    })
    .sort((a, b) => b.best - a.best || b.miner.hashrateThs - a.miner.hashrateThs);
});

const leaders = computed(() => {
  const rows = minerRows.value;
  return [
    { label: "carrier", name: [...rows].sort((a, b) => b.dominance - a.dominance)[0]?.miner.nickname ?? "-" },
    { label: "best swing", name: [...rows].sort((a, b) => b.best - a.best)[0]?.miner.nickname ?? "-" },
    { label: "most stable", name: [...rows].sort((a, b) => a.sd - b.sd)[0]?.miner.nickname ?? "-" },
    { label: "reject pain", name: [...miners.value].sort((a, b) => b.rejectPct - a.rejectPct)[0]?.nickname ?? "-" },
  ];
});

function bestWorkByMiner(history: HistoryPoint[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (let i = 1; i < history.length; i += 1) {
    const prev = history[i - 1];
    const next = history[i];
    if (!prev?.minerWork || !next?.minerWork || !prev.minerShares || !next.minerShares) continue;
    for (const [key, work] of Object.entries(next.minerWork)) {
      const shares = (next.minerShares[key] ?? 0) - (prev.minerShares[key] ?? 0);
      const workDelta = work - (prev.minerWork[key] ?? work);
      if (shares > 0 && workDelta > 0) out[key] = Math.max(out[key] ?? 0, workDelta / shares);
    }
  }
  return out;
}

function minerKey(m: MinerStat): string {
  return m.authUsername || m.remoteHost || m.nickname;
}

function average(values: number[]): number {
  if (!values.length) return 0;
  return values.reduce((sum, n) => sum + n, 0) / values.length;
}

function stddev(values: number[], avg: number): number {
  if (values.length < 2) return 0;
  return Math.sqrt(values.reduce((sum, n) => sum + (n - avg) ** 2, 0) / values.length);
}

function statusPct(history: HistoryPoint[], key: string, status: MinerStat["status"]): number {
  const seen = history.filter((p) => p.minerStatus?.[key]);
  if (!seen.length) return 0;
  return (seen.filter((p) => p.minerStatus[key] === status).length / seen.length) * 100;
}

function minerShareVelocity(history: HistoryPoint[], key: string): number {
  const first = history.find((p) => Number.isFinite(p.minerShares?.[key]));
  const last = [...history].reverse().find((p) => Number.isFinite(p.minerShares?.[key]));
  if (!first || !last || last.t <= first.t) return 0;
  return Math.max(0, (last.minerShares[key] ?? 0) - (first.minerShares[key] ?? 0)) / ((last.t - first.t) / 3600000);
}

function personaLine(m: MinerStat, dominance: number, expectedPct: number, sd: number, idlePct: number): string {
  if (idlePct > 30) return `${m.nickname} has spent too much of this window being decorative.`;
  if (m.rejectPct > 5) return `${m.nickname} is donating work to the void with confidence.`;
  if (expectedPct > 110) return `${m.nickname} is carrying the group project and will be insufferable about it.`;
  if (dominance > 55) return `${m.nickname} is the adult in the room, which is bleak for everyone else.`;
  if (sd > Math.max(0.4, m.hashrateThs * 0.25)) return `${m.nickname} hashes like a shopping trolley with one bad wheel.`;
  return `${m.nickname} is doing its tiny job. Nobody alert the institutions.`;
}

function ageLabel(seconds: number | null): string {
  if (seconds === null) return "never";
  if (seconds < 60) return `${seconds.toFixed(0)}s`;
  if (seconds < 3600) return `${(seconds / 60).toFixed(1)}m`;
  return `${(seconds / 3600).toFixed(1)}h`;
}

function duration(seconds: number): string {
  if (!seconds || !Number.isFinite(seconds)) return "collecting";
  const years = seconds / 31536000;
  if (years >= 1e9) return `${(years / 1e9).toFixed(2)}B years`;
  if (years >= 1e6) return `${(years / 1e6).toFixed(2)}M years`;
  if (years >= 1000) return `${(years / 1000).toFixed(2)}K years`;
  if (years >= 1) return `${years.toFixed(1)} years`;
  const days = seconds / 86400;
  return days >= 1 ? `${days.toFixed(1)} days` : `${(seconds / 3600).toFixed(1)}h`;
}

function compact(n: number, digits = 2): string {
  if (!Number.isFinite(n)) return "-";
  if (Math.abs(n) >= 1e12) return `${(n / 1e12).toFixed(digits)}T`;
  if (Math.abs(n) >= 1e9) return `${(n / 1e9).toFixed(digits)}B`;
  if (Math.abs(n) >= 1e6) return `${(n / 1e6).toFixed(digits)}M`;
  if (Math.abs(n) >= 1e3) return `${(n / 1e3).toFixed(digits)}K`;
  return n.toFixed(digits);
}

function pct(n: number, digits = 4): string {
  if (!Number.isFinite(n) || n <= 0) return "0%";
  if (n < 0.001) return tinyPercentAsOdds(n);
  return `${n.toFixed(digits)}%`;
}
</script>

<template>
  <section class="shameboard panel">
    <header>
      <div>
        <div class="label">leaderboard / shameboard</div>
        <h2 class="glow-magenta">Receipts from the hashing cupboard</h2>
      </div>
      <div class="health">
        <span class="label">connection</span>
        <strong>{{ connectionHealth }}</strong>
      </div>
    </header>

    <div class="summary">
      <div>
        <span class="label">best pool swing</span>
        <strong class="glow-amber">{{ poolBest > 0 ? compact(poolBest) : "collecting" }}</strong>
        <small>{{ difficulty > 0 ? `${pct((poolBest / difficulty) * 100)} of difficulty` : "difficulty unavailable" }}</small>
      </div>
      <div>
        <span class="label">expected block</span>
        <strong>{{ duration(expectedSecondsToBlock) }}</strong>
        <small>{{ compact(expectedBlocksYear, 8) }} blocks/year</small>
      </div>
      <div>
        <span class="label">imaginary payday</span>
        <strong class="glow-green">{{ compact(expectedSatsDay, 4) }}</strong>
        <small>sats/day expected</small>
      </div>
      <div>
        <span class="label">share velocity</span>
        <strong>{{ compact(shareVelocity, 1) }}</strong>
        <small>accepted/hour</small>
      </div>
      <div>
        <span class="label">space heater mode</span>
        <strong>{{ compact(totalWatts, 0) }} W</strong>
        <small>{{ compact(heatBtu, 0) }} BTU/hr · {{ compact(poolEfficiency, 4) }} TH/W</small>
      </div>
      <div>
        <span class="label">network bullying</span>
        <strong class="glow-red">{{ networkMultiple > 0 ? multiple(networkMultiple) : "collecting" }}</strong>
        <small>{{ networkEh > 0 ? `${compact(networkEh, 0)} EH/s globally. easy version: the planet brought a warehouse.` : "tx1138 mempool warming up" }}</small>
      </div>
    </div>

    <div class="leader-strip">
      <div v-for="leader in leaders" :key="leader.label">
        <span class="label">{{ leader.label }}</span>
        <strong>{{ leader.name }}</strong>
      </div>
    </div>

    <div class="table">
      <div class="row head">
        <span>miner</span>
        <span>best</span>
        <span>share</span>
        <span>stability</span>
        <span>power</span>
        <span>last</span>
      </div>
      <div v-for="row in minerRows" :key="row.key" class="row">
        <div class="who">
          <strong>{{ row.miner.nickname }}</strong>
          <small>{{ row.line }}</small>
        </div>
        <span>
          {{ row.best > 0 ? compact(row.best) : "collecting" }}
          <small>{{ difficulty > 0 ? pct(row.bestDifficultyPct) : "diff ?" }}</small>
        </span>
        <span>
          {{ compact(row.dominance, 1) }}%
          <small>{{ compact(row.sharesPerHour, 1) }}/hr</small>
        </span>
        <span>
          {{ compact(row.avg) }} TH/s
          <small>{{ compact(row.low) }}-{{ compact(row.high) }} · sd {{ compact(row.sd) }}</small>
        </span>
        <span>
          {{ compact(row.thPerWatt, 4) }} TH/W
          <small>{{ row.miner.watts }} W · {{ compact(row.heatBtu, 0) }} BTU/hr</small>
        </span>
        <span>
          {{ ageLabel(row.miner.lastShareAgeS) }}
          <small>{{ compact(row.idlePct, 1) }}% idle/stale</small>
        </span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.shameboard {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin: 16px 0;
}
header,
.summary,
.leader-strip,
.row {
  display: grid;
  gap: 12px;
}
header {
  grid-template-columns: 1fr auto;
  align-items: start;
}
h2 {
  margin: 2px 0 0;
  font-size: 20px;
  letter-spacing: 0;
}
.health {
  text-align: right;
}
.health strong {
  display: block;
  color: var(--neon-cyan);
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.summary {
  grid-template-columns: repeat(6, 1fr);
}
.summary > div,
.leader-strip > div {
  border: 1px solid var(--line);
  background: rgba(0, 0, 0, 0.18);
  padding: 10px;
  min-height: 88px;
}
strong {
  display: block;
  font-size: 16px;
}
small {
  display: block;
  color: var(--fg-2);
  font-size: 10px;
  line-height: 1.35;
}
.leader-strip {
  grid-template-columns: repeat(4, 1fr);
}
.table {
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--line);
}
.row {
  grid-template-columns: minmax(180px, 1.6fr) repeat(5, minmax(96px, 1fr));
  align-items: center;
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
}
.row.head {
  color: var(--fg-2);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.12em;
}
.who strong {
  color: var(--neon-magenta);
}
@media (max-width: 1100px) {
  .summary {
    grid-template-columns: repeat(2, 1fr);
  }
  .leader-strip {
    grid-template-columns: repeat(2, 1fr);
  }
  .row {
    grid-template-columns: 1fr 1fr;
  }
  .row.head {
    display: none;
  }
}
@media (max-width: 640px) {
  header,
  .summary,
  .leader-strip,
  .row {
    grid-template-columns: 1fr;
  }
  .health {
    text-align: left;
  }
}
</style>
