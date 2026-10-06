<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DatumSnapshot } from "../types";

const props = defineProps<{ snapshot: DatumSnapshot | null }>();

// Simple sparkline: keep the last N hashrate samples and render as SVG.
const SAMPLES = 60;
const samples = ref<number[]>([]);
let lastFetchedAt = 0;

watch(
  () => props.snapshot,
  (snap) => {
    if (!snap || snap.fetchedAt === lastFetchedAt) return;
    lastFetchedAt = snap.fetchedAt;
    samples.value.push(snap.pool.combinedHashrateThs);
    if (samples.value.length > SAMPLES) samples.value.shift();
  },
);

const path = computed(() => {
  const ys = samples.value;
  if (ys.length < 2) return "";
  const max = Math.max(...ys, 0.001);
  const w = 100;
  const h = 32;
  return ys
    .map((y, i) => {
      const x = (i / (SAMPLES - 1)) * w;
      const yPx = h - (y / max) * h;
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${yPx.toFixed(2)}`;
    })
    .join(" ");
});
</script>

<template>
  <section class="ticker panel">
    <div class="label">hashrate · last {{ SAMPLES }} polls</div>
    <svg viewBox="0 0 100 32" preserveAspectRatio="none">
      <path :d="path" stroke="var(--neon-cyan)" stroke-width="0.6" fill="none" />
    </svg>
  </section>
</template>

<style scoped>
.ticker { display: flex; flex-direction: column; gap: 6px; }
svg { width: 100%; height: 60px; }
</style>
