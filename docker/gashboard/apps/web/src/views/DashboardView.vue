<script setup lang="ts">
import { computed, onMounted, onUnmounted } from "vue";
import { useStatsStore } from "../stores/stats";
import MinerCard from "../components/MinerCard.vue";
import PoolHero from "../components/PoolHero.vue";
import LotteryWidget from "../components/LotteryWidget.vue";
import ShareTicker from "../components/ShareTicker.vue";
import BlockCelebration from "../components/BlockCelebration.vue";
import Shameboard from "../components/Shameboard.vue";

const stats = useStatsStore();

onMounted(() => stats.start());
onUnmounted(() => stats.stop());

const totalThs = computed(() => stats.snapshot?.pool.combinedHashrateThs ?? 0);
const networkEh = computed(() => stats.snapshot?.network.hashrateEh ?? 0);
const miners = computed(() => stats.snapshot?.miners ?? []);
const errorMsg = computed(() => stats.error);
const upstreamErr = computed(() => stats.snapshot?.error);

const bestWorkByMiner = computed(() => {
  const out: Record<string, number> = {};
  for (let i = 1; i < stats.history.length; i += 1) {
    const prev = stats.history[i - 1];
    const next = stats.history[i];
    if (!prev?.minerWork || !next?.minerWork || !prev.minerShares || !next.minerShares) continue;
    for (const [key, work] of Object.entries(next.minerWork)) {
      const shareDelta = (next.minerShares[key] ?? 0) - (prev.minerShares[key] ?? 0);
      const workDelta = work - (prev.minerWork[key] ?? work);
      if (shareDelta <= 0 || workDelta <= 0) continue;
      out[key] = Math.max(out[key] ?? 0, workDelta / shareDelta);
    }
  }
  return out;
});
const poolBestWork = computed(() => Math.max(0, ...Object.values(bestWorkByMiner.value)));
const contributionRows = computed(() => {
  return miners.value
    .filter((m) => (m.contributionAcceptedWork ?? m.diffAcceptedSum) > 0)
    .map((m) => ({
      key: minerKey(m),
      name: m.nickname,
      work: m.contributionAcceptedWork ?? m.diffAcceptedSum,
      pct: m.contributionPct ?? 0,
      connected: m.currentlyConnected ?? true,
    }))
    .sort((a, b) => b.work - a.work);
});
const boomerHeater = {
  authUsername: "fiat.heat",
  remoteHost: "wall-socket",
  nickname: "Boomer Heater",
  model: "Legacy Resistance Heater",
  location: "The Past",
  expectedHashrateThs: 0,
  watts: 2000,
  hashrateThs: 0,
  hashrateAgeS: null,
  lastShareAgeS: null,
  diffAcceptedSum: 0,
  diffAcceptedCount: 0,
  diffRejectedSum: 0,
  diffRejectedCount: 0,
  rejectPct: 0,
  vdiff: 0,
  userAgent: "CentralHeating/fiat-only",
  subscribed: false,
  status: "idle",
} as const;

function minerKey(m: { nickname: string; authUsername: string; remoteHost: string }): string {
  return m.authUsername || m.remoteHost || m.nickname;
}
</script>

<template>
  <BlockCelebration :snapshot="stats.snapshot" />

  <div v-if="errorMsg" class="banner err">
    ✗ {{ errorMsg }}
  </div>
  <div v-else-if="upstreamErr" class="banner warn">
    ⚠ datum: {{ upstreamErr.code }} — {{ upstreamErr.message }}
  </div>

  <PoolHero :snapshot="stats.snapshot" />

  <section class="best panel">
    <div>
      <div class="label">best accepted calculation observed</div>
      <strong class="glow-amber">
        {{ poolBestWork > 0 ? poolBestWork.toLocaleString(undefined, { maximumFractionDigits: 2 }) : "collecting" }}
      </strong>
    </div>
    <span class="muted">derived from accepted difficulty jumps while this browser has history</span>
  </section>

  <section class="split panel">
    <div class="split-head">
      <div>
        <div class="label">block reward split</div>
        <strong class="glow-green">accepted-work ledger</strong>
      </div>
      <span class="muted">persisted after this deploy</span>
    </div>
    <div v-if="contributionRows.length" class="split-rows">
      <div v-for="row in contributionRows" :key="row.key" class="split-row">
        <span>
          {{ row.name }}
          <small v-if="!row.connected" class="muted">offline</small>
        </span>
        <strong>{{ row.pct.toFixed(2) }}%</strong>
        <em class="muted">{{ row.work.toLocaleString(undefined, { maximumFractionDigits: 2 }) }}</em>
      </div>
    </div>
    <div v-else class="muted">waiting for accepted work</div>
  </section>

  <div class="grid-row">
    <LotteryWidget :total-ths="totalThs" :network-eh="networkEh" />
    <ShareTicker :snapshot="stats.snapshot" />
  </div>

  <Shameboard :snapshot="stats.snapshot" :history="stats.history" />

  <div class="miners">
    <MinerCard
      v-for="m in miners"
      :key="m.authUsername || m.nickname"
      :miner="m"
      :best-work="bestWorkByMiner[minerKey(m)]"
    />
    <MinerCard
      key="boomer-heater"
      :miner="boomerHeater"
      :best-work="0"
    />
    <div v-if="!miners.length && !errorMsg" class="empty panel muted">
      waiting for the first poll · the boards are warming up
    </div>
  </div>
</template>

<style scoped>
.banner {
  padding: 10px 14px;
  margin-bottom: 16px;
  font-size: 12px;
  border: 1px solid var(--line);
}
.banner.err { border-color: var(--neon-red); color: var(--neon-red); }
.banner.warn { border-color: var(--neon-amber); color: var(--neon-amber); }
.grid-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 16px;
  margin: 16px 0;
}
.best {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: center;
  margin-top: 16px;
  min-width: 0;
}
.best strong {
  display: block;
  margin-top: 4px;
  font-size: 24px;
}
.best span {
  font-size: 11px;
  text-align: right;
  overflow-wrap: anywhere;
}
.split {
  margin-top: 16px;
}
.split-head,
.split-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: 14px;
  align-items: center;
}
.split-head {
  margin-bottom: 10px;
}
.split-rows {
  display: grid;
  gap: 8px;
}
.split-row {
  padding-top: 8px;
  border-top: 1px solid var(--line);
  font-size: 13px;
}
.split-row small {
  margin-left: 8px;
  font-size: 10px;
  text-transform: uppercase;
}
.split-row em {
  font-style: normal;
  font-size: 11px;
}
.miners {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 16px;
}
.empty {
  padding: 40px;
  text-align: center;
  font-style: italic;
}
@media (max-width: 800px) {
  .grid-row { grid-template-columns: 1fr; }
  .best {
    align-items: flex-start;
    flex-direction: column;
  }
  .best span {
    text-align: left;
  }
  .split-head,
  .split-row {
    grid-template-columns: minmax(0, 1fr) auto;
  }
  .split-head > span,
  .split-row em {
    grid-column: 1 / -1;
  }
}
</style>
