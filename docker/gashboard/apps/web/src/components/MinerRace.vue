<script setup lang="ts">
import { computed } from "vue";
import type { DatumSnapshot, HistoryPoint, MinerStat } from "../types";
import { useRotatingCopy } from "../composables/useRotatingCopy";
import { RACE_SCALE_NOTES, RACE_TITLES } from "../copy";
import { multiple } from "../format";

const props = defineProps<{ snapshot: DatumSnapshot | null; history: HistoryPoint[] }>();
const raceTitle = useRotatingCopy(RACE_TITLES);
const scaleNote = useRotatingCopy(RACE_SCALE_NOTES);
const networkEh = computed(() => props.snapshot?.network.hashrateEh ?? 0);
const totalThs = computed(() => props.snapshot?.pool.combinedHashrateThs ?? 0);
const networkMultiple = computed(() => networkEh.value > 0 && totalThs.value > 0 ? (networkEh.value * 1_000_000) / totalThs.value : 0);

const rows = computed(() => {
  const miners = props.snapshot?.miners ?? [];
  const max = Math.max(...miners.map((m) => m.hashrateThs), 0.001);
  const realRows = miners
    .map((miner) => {
      const key = minerKey(miner);
      const samples = props.history.map((p) => p.miners[key]).filter((v): v is number => Number.isFinite(v));
      const velocity = samples.length >= 2 ? samples.at(-1)! - samples[0]! : 0;
      return {
        key,
        miner,
        image: imageFor(miner),
        width: Math.max(8, (miner.hashrateThs / max) * 100),
        trend: velocity,
        place: 0,
      };
    })
    .sort((a, b) => b.miner.hashrateThs - a.miner.hashrateThs)
    .map((row, idx) => ({ ...row, place: idx + 1, joke: "" }));

  return [
    ...realRows,
    {
      key: "boomer-heater",
      miner: {
        nickname: "Boomer Heater",
        model: "Legacy Resistance Heater",
        hashrateThs: 0,
        status: "idle",
      },
      image: "/miners/boomer-heater.svg",
      width: 0,
      trend: 0,
      place: realRows.length + 1,
      joke: "0 TH/s · 0 shares · 0 sats · just fiat and a pension.",
    },
  ];
});

const cosmicRows = computed(() => {
  if (!networkEh.value || !totalThs.value) return [];
  const totalNetworkThs = networkEh.value * 1_000_000;
  const largePoolThs = totalNetworkThs * 0.25;
  return [
    {
      key: "large-pool",
      name: "Large pool-ish bully",
      value: `${(largePoolThs / 1_000_000).toFixed(0)} EH/s`,
      width: 78,
      joke: "illustrative 25% chunk of the monster",
    },
    {
      key: "network",
      name: "Total Bitcoin network",
      value: `${networkEh.value.toFixed(0)} EH/s`,
      width: 100,
      joke: `${multiple(networkMultiple.value)} your shelf. rude but factual.`,
    },
  ];
});

function minerKey(m: MinerStat): string {
  return m.authUsername || m.remoteHost || m.nickname;
}

function imageFor(m: MinerStat): string {
  if (m.nickname === "QU4CK") return "/miners/nerdqaxe.svg";
  if (m.nickname === "P1XEL") return "/miners/bitaxe.svg";
  if (m.nickname === "BigPapa" && /bitaxe/i.test(m.model)) return "/miners/bigpapa-bitaxe.svg";
  if (m.nickname === "N4N0") return "/miners/avalon-nano-3.svg";
  return "/miners/avalon-mini-3.svg";
}
</script>

<template>
  <section class="race panel">
    <header>
      <div>
        <div class="label">hashrate race</div>
        <h2 class="glow-cyan">{{ raceTitle }}</h2>
      </div>
      <span class="muted">{{ scaleNote }}</span>
    </header>

    <div class="track">
      <div class="section-label">Actual miners</div>
      <div v-for="row in rows" :key="row.key" class="lane">
        <div class="rank">#{{ row.place }}</div>
        <img :src="row.image" :alt="row.miner.model" />
        <div class="runner">
          <div class="meta">
            <strong>{{ row.miner.nickname }}</strong>
            <span>{{ row.joke || `${row.miner.hashrateThs.toFixed(2)} TH/s` }}</span>
          </div>
          <div :class="['bar', { dead: row.width <= 0 }]">
            <span :style="{ width: `${row.width}%` }">
              <i>{{ row.width <= 0 ? "lol" : row.trend >= 0 ? "▲" : "▼" }}</i>
            </span>
          </div>
        </div>
      </div>

      <div v-if="cosmicRows.length" class="section-label cosmic-label">
        Big pools / total network · separate scale so the kids remain visible
      </div>
      <div v-for="row in cosmicRows" :key="row.key" class="lane cosmic">
        <div class="rank">∞</div>
        <div class="network-icon">₿</div>
        <div class="runner">
          <div class="meta">
            <strong>{{ row.name }}</strong>
            <span>{{ row.value }} · {{ row.joke }}</span>
          </div>
          <div class="bar cosmic-bar">
            <span :style="{ width: `${row.width}%` }">
              <i>NOPE</i>
            </span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.race {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-bottom: 16px;
}
header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: start;
}
h2 {
  margin: 2px 0 0;
  font-size: 20px;
  letter-spacing: 0;
}
.track {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.section-label {
  color: var(--neon-amber);
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  border-top: 1px solid var(--line);
  padding-top: 10px;
}
.cosmic-label {
  color: var(--neon-red);
}
.lane {
  display: grid;
  grid-template-columns: 46px 86px 1fr;
  gap: 12px;
  align-items: center;
}
.rank {
  color: var(--neon-amber);
  font-weight: 700;
  font-size: 18px;
}
img {
  width: 86px;
  height: 58px;
  object-fit: contain;
}
.network-icon {
  width: 58px;
  height: 58px;
  display: grid;
  place-items: center;
  border: 1px solid var(--neon-red);
  color: var(--neon-amber);
  font-size: 28px;
  box-shadow: 0 0 14px rgba(255, 79, 120, 0.24);
}
.runner {
  min-width: 0;
}
.meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
}
.meta span {
  color: var(--fg-2);
}
.bar {
  height: 18px;
  margin-top: 5px;
  border: 1px solid var(--line);
  background: var(--bg-2);
  overflow: hidden;
}
.bar span {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  min-width: 28px;
  height: 100%;
  background: linear-gradient(90deg, var(--neon-cyan), var(--neon-magenta), var(--heat-color, var(--neon-red)));
  box-shadow: 0 0 14px rgba(255, 79, 120, 0.25);
  transition: width 0.6s ease;
}
.bar.dead span {
  width: 22px !important;
  background: #475569;
  box-shadow: none;
}
.cosmic .bar span {
  background: linear-gradient(90deg, #ff4f78, #ffd84a);
}
.cosmic .meta strong {
  color: var(--neon-red);
}
.bar i {
  padding-right: 6px;
  color: #07090f;
  font-style: normal;
  font-size: 10px;
  font-weight: 700;
}
@media (max-width: 620px) {
  header {
    flex-direction: column;
  }
  .lane {
    grid-template-columns: 36px 64px 1fr;
  }
  img {
    width: 64px;
  }
}
</style>
