<script setup lang="ts">
import { computed } from "vue";
import type { DatumSnapshot } from "../types";
import { useRotatingCopy } from "../composables/useRotatingCopy";
import { BLOCK_LABELS, HASHRATE_LABELS, NETWORK_LABELS } from "../copy";
import { multiple } from "../format";

const props = defineProps<{ snapshot: DatumSnapshot | null }>();

const totalThs = computed(() => props.snapshot?.pool.combinedHashrateThs ?? 0);
const networkEh = computed(() => props.snapshot?.network.hashrateEh ?? 0);
const networkMultiple = computed(() => networkEh.value > 0 && totalThs.value > 0 ? (networkEh.value * 1_000_000) / totalThs.value : 0);
const networkRoast = computed(() => {
  if (!networkMultiple.value) return "waiting for the grown-up numbers";
  if (networkMultiple.value > 20_000_000) return "the network is a freight train; this shelf is a polite cough.";
  if (networkMultiple.value > 1_000_000) return "big miners brought warehouses; you brought character.";
  return "we are briefly visible on the cosmic spreadsheet.";
});
const subscribed = computed(() => props.snapshot?.pool.totalSubscriptions ?? 0);
const sharesAccepted = computed(() => props.snapshot?.pool.sharesAccepted ?? 0);
const sharesRejected = computed(() => props.snapshot?.pool.sharesRejected ?? 0);
const blockHeight = computed(() => props.snapshot?.job.blockHeight ?? 0);
const hashrateLabel = useRotatingCopy(HASHRATE_LABELS);
const blockLabel = useRotatingCopy(BLOCK_LABELS);
const networkLabel = useRotatingCopy(NETWORK_LABELS);
const ageS = computed(() => {
  const t = props.snapshot?.fetchedAt;
  if (!t) return null;
  return Math.floor((Date.now() - t) / 1000);
});
</script>

<template>
  <section class="hero panel">
    <div class="left">
      <div class="label">{{ hashrateLabel }}</div>
      <div class="number glow-cyan">
        {{ totalThs.toFixed(2) }}
        <span class="unit">Th/s</span>
      </div>
      <div class="network-roast muted">{{ networkRoast }}</div>
    </div>
    <div class="right">
      <div class="cell">
        <div class="label">{{ blockLabel }}</div>
        <div class="value">{{ blockHeight > 0 ? blockHeight.toLocaleString() : "—" }}</div>
      </div>
      <div class="cell">
        <div class="label">{{ networkLabel }}</div>
        <div class="value glow-amber">{{ networkEh > 0 ? `${networkEh.toFixed(0)} EH/s` : "—" }}</div>
      </div>
      <div class="cell">
        <div class="label">network is bigger by</div>
        <div class="value glow-red">{{ networkMultiple > 0 ? multiple(networkMultiple) : "—" }}</div>
      </div>
      <div class="cell">
        <div class="label">subscribed</div>
        <div class="value">{{ subscribed }}</div>
      </div>
      <div class="cell">
        <div class="label">accepted</div>
        <div class="value glow-green">{{ sharesAccepted.toLocaleString() }}</div>
      </div>
      <div class="cell">
        <div class="label">rejected</div>
        <div class="value glow-red">{{ sharesRejected.toLocaleString() }}</div>
      </div>
      <div class="cell">
        <div class="label">last poll</div>
        <div class="value muted">{{ ageS === null ? "—" : `${ageS}s ago` }}</div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero {
  display: grid;
  grid-template-columns: 2fr 3fr;
  gap: 24px;
  align-items: center;
}
.left .number {
  font-size: 64px;
  font-weight: 700;
  letter-spacing: 0.04em;
  line-height: 1;
}
.left .unit {
  font-size: 18px;
  color: var(--fg-2);
}
.network-roast {
  margin-top: 10px;
  font-size: 11px;
  line-height: 1.4;
}
.right {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 12px;
  align-items: stretch;
}
.cell {
  min-height: 74px;
  display: grid;
  grid-template-rows: 34px 1fr;
  align-content: start;
}
.cell .label {
  min-height: 34px;
  display: flex;
  align-items: flex-start;
  line-height: 1.25;
  overflow: hidden;
}
.cell .value {
  font-size: 18px;
  font-weight: 600;
  margin-top: 6px;
  min-height: 28px;
  display: flex;
  align-items: baseline;
  white-space: nowrap;
}
@media (max-width: 800px) {
  .hero { grid-template-columns: 1fr; }
  .right { grid-template-columns: repeat(2, 1fr); }
  .left .number { font-size: 44px; }
}
</style>
