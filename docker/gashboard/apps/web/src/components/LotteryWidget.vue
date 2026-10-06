<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { LOTTERY_LINES } from "../strings";

const props = defineProps<{ totalThs: number; networkEh: number }>();

const FALLBACK_NETWORK_EH_S = 720;
const SECONDS_PER_BLOCK = 600;
const SECONDS_PER_DAY = 86400;
const activeNetworkEh = computed(() => props.networkEh > 0 ? props.networkEh : FALLBACK_NETWORK_EH_S);

const probPerBlock = computed(() => {
  if (props.totalThs <= 0) return 0;
  // Th/s vs EH/s = 1e6 conversion: 1 EH/s = 1e6 Th/s
  return props.totalThs / (activeNetworkEh.value * 1e6);
});

const probPerDay = computed(() => {
  // Probability of NOT finding in any of (86400/600) = 144 blocks per day
  const blocksPerDay = SECONDS_PER_DAY / SECONDS_PER_BLOCK;
  const p = probPerBlock.value;
  return 1 - Math.pow(1 - p, blocksPerDay);
});

const lineIndex = ref(0);
let timer: number | null = null;
onMounted(() => {
  timer = window.setInterval(() => {
    lineIndex.value = (lineIndex.value + 1) % LOTTERY_LINES.length;
  }, 10_000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
});

const currentLine = computed(() => {
  const fn = LOTTERY_LINES[lineIndex.value];
  if (!fn) return "";
  return fn(probPerDay.value);
});
</script>

<template>
  <section class="lottery panel">
    <div class="label">solo lottery odds</div>
    <div class="source muted">
      calculated against {{ activeNetworkEh.toFixed(0) }} EH/s network hashrate
    </div>
    <div class="line glow-amber">
      <span class="prefix">$</span>
      <span class="text">{{ currentLine }}</span>
    </div>
    <div class="dots">
      <span
        v-for="(_, i) in LOTTERY_LINES"
        :key="i"
        :class="['dot', { active: i === lineIndex }]"
      />
    </div>
  </section>
</template>

<style scoped>
.lottery {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}
.source {
  font-size: 10px;
  overflow-wrap: anywhere;
}
.line {
  font-size: 13px;
  display: flex;
  gap: 10px;
  align-items: baseline;
  min-height: 32px;
  min-width: 0;
}
.prefix { color: var(--neon-amber); font-weight: 700; }
.text {
  line-height: 1.5;
  min-width: 0;
  overflow-wrap: anywhere;
  word-break: break-word;
}
.dots {
  display: flex;
  gap: 5px;
  margin-top: 4px;
  flex-wrap: wrap;
}
.dot {
  width: 5px; height: 5px;
  border-radius: 50%;
  background: var(--line-bright);
  transition: background 0.3s, box-shadow 0.3s;
}
.dot.active {
  background: var(--neon-amber);
  box-shadow: 0 0 8px var(--neon-amber);
}
</style>
