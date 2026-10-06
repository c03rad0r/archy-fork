<script setup lang="ts">
import { computed } from "vue";
import type { MinerStat } from "../types";
import { ROASTS, STATUS_LINES, STALE_LINES } from "../strings";

const props = defineProps<{ miner: MinerStat; bestWork: number | undefined }>();

const isBoomerHeater = computed(() => props.miner.nickname === "Boomer Heater");

const slug = computed(() => {
  // Map nickname → roast key
  const map: Record<string, string> = {
    QU4CK: "nerdqaxe",
    P1XEL: "bitaxe",
    BigPapa: "bitaxe",
    N4N0: "avalon-nano-3",
    M1N1: "avalon-mini-3",
  };
  return map[props.miner.nickname] ?? "unknown";
});

const roast = computed(() =>
  isBoomerHeater.value ? "The dumb shit your fiat friends have." : (ROASTS[slug.value] ?? ROASTS["unknown"]),
);

const statusLabel = computed(() => STATUS_LINES[props.miner.status]);
const displayStatusLabel = computed(() => isBoomerHeater.value ? "dead" : statusLabel.value);
const statusMarker = computed(() => isBoomerHeater.value ? "☠" : "");
const statusGlow = computed(() => {
  if (isBoomerHeater.value) return "glow-red";
  switch (props.miner.status) {
    case "hashing": return "glow-green";
    case "stale": return "glow-amber";
    case "idle": return "glow-red";
  }
});

const lastShareLabel = computed(() => {
  if (isBoomerHeater.value) return "Nothing";
  const s = props.miner.lastShareAgeS;
  if (s === null) return "no shares yet";
  if (s < 60) return `${s.toFixed(0)}s ago`;
  if (s < 3600) return `${(s / 60).toFixed(1)}m ago`;
  return `${(s / 3600).toFixed(1)}h ago`;
});

const lifetimeShares = computed(() => props.miner.diffAcceptedCount);
const acceptedWork = computed(() => isBoomerHeater.value ? "Zero. Obviously." : formatBig(props.miner.diffAcceptedSum));
const contributionWork = computed(() =>
  isBoomerHeater.value ? "Zero. Obviously." : formatBig(props.miner.contributionAcceptedWork ?? props.miner.diffAcceptedSum),
);
const contributionShare = computed(() =>
  isBoomerHeater.value ? "0.00%" : `${(props.miner.contributionPct ?? 0).toFixed(2)}%`,
);
const bestWork = computed(() => {
  if (isBoomerHeater.value) return "Fuck all";
  return props.bestWork && props.bestWork > 0 ? formatBig(props.bestWork) : "collecting";
});

const staleNote = computed(() => {
  if (isBoomerHeater.value) return "warming a room while contributing nothing to civilization.";
  if (props.miner.status !== "stale") return "";
  const idx = Math.floor(Math.random() * STALE_LINES.length);
  return STALE_LINES[idx];
});

function formatBig(n: number): string {
  if (n < 1_000) return n.toFixed(0);
  if (n < 1e6) return `${(n / 1e3).toFixed(1)}K`;
  if (n < 1e9) return `${(n / 1e6).toFixed(1)}M`;
  if (n < 1e12) return `${(n / 1e9).toFixed(1)}B`;
  return `${(n / 1e12).toFixed(1)}T`;
}
</script>

<template>
  <div class="card panel" :data-status="miner.status">
    <header>
      <div class="nickname glow-magenta">{{ miner.nickname }}</div>
      <div :class="['status', statusGlow]">
        <span v-if="statusMarker" class="dead-mark">{{ statusMarker }}</span>
        <span v-else class="dot pulse" />
        {{ displayStatusLabel }}
      </div>
    </header>

    <div class="model">{{ miner.model }}</div>
    <div class="roast muted">{{ roast }}</div>

    <div class="stats">
      <div class="stat">
        <div class="label">hashrate</div>
        <div class="value glow-cyan">
          {{ isBoomerHeater ? "No hashes" : miner.hashrateThs.toFixed(2) }}
          <span class="unit">Th/s</span>
        </div>
      </div>
      <div class="stat">
        <div class="label">last share</div>
        <div class="value">{{ lastShareLabel }}</div>
      </div>
      <div class="stat">
        <div class="label">tickets bought</div>
        <div class="value">{{ isBoomerHeater ? "None" : lifetimeShares.toLocaleString() }}</div>
      </div>
      <div class="stat">
        <div class="label">best calc</div>
        <div class="value glow-amber">{{ bestWork }}</div>
      </div>
      <div class="stat">
        <div class="label">reward share</div>
        <div class="value glow-green">{{ contributionShare }}</div>
      </div>
      <div class="stat">
        <div class="label">reward work</div>
        <div class="value">{{ contributionWork }}</div>
      </div>
      <div class="stat">
        <div class="label">reject %</div>
        <div :class="['value', miner.rejectPct > 5 ? 'glow-red' : '']">
          {{ isBoomerHeater ? "not even rejected" : `${miner.rejectPct.toFixed(2)}%` }}
        </div>
      </div>
      <div class="stat">
        <div class="label">accepted work</div>
        <div class="value">{{ acceptedWork }}</div>
      </div>
    </div>

    <footer>
      <span class="ua muted" :title="miner.userAgent">{{ miner.userAgent || "—" }}</span>
      <span class="loc muted">@{{ miner.location }}</span>
    </footer>

    <div v-if="miner.status === 'stale' || isBoomerHeater" class="stale-banner">
      {{ staleNote }}
    </div>
  </div>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  position: relative;
  overflow: hidden;
}
.card[data-status="idle"] {
  opacity: 0.6;
}
.card[data-status="stale"] {
  border-color: var(--neon-amber);
}
.card[data-status="hashing"] {
  border-color: var(--neon-green);
}
header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.nickname {
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 0.16em;
}
.status {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  display: flex;
  gap: 6px;
  align-items: center;
}
.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 8px currentColor;
}
.dead-mark {
  font-size: 13px;
  line-height: 1;
  text-shadow: 0 0 8px currentColor;
}
.model {
  font-size: 12px;
  color: var(--fg-1);
  letter-spacing: 0.06em;
}
.roast {
  font-size: 11.5px;
  font-style: italic;
  line-height: 1.4;
}
.stats {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px 14px;
  padding: 10px 0;
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
}
.stat .label {
  font-size: 10px;
}
.stat .value {
  font-size: 14px;
  font-weight: 500;
}
.stat .unit {
  font-size: 10px;
  color: var(--fg-2);
}
footer {
  display: flex;
  justify-content: space-between;
  font-size: 10.5px;
}
.ua { max-width: 65%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stale-banner {
  font-size: 10.5px;
  color: var(--neon-amber);
  border-top: 1px dashed var(--neon-amber);
  padding-top: 8px;
  font-style: italic;
}
</style>
