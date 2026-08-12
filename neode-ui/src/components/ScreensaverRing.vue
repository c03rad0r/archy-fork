<template>
  <div class="viz-ring" :class="sizeClass">
    <div
      v-for="(_, i) in segmentCount"
      :key="i"
      class="viz-segment"
      :style="getSegmentStyle(i)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  /** Visual size: 'default' matches the screensaver; 'compact' drops the
   *  min-width breakpoints (useful inside overlays on narrower canvases);
   *  'badge' is the paid-tick size that fits inside a modal card (FED-06). */
  size?: 'default' | 'compact' | 'badge'
  /** Override segment count. Defaults to 48 (screensaver standard). */
  segmentCount?: number
}>(), { size: 'default', segmentCount: 48 })

const sizeClass = computed(() => {
  if (props.size === 'compact') return 'viz-ring-compact'
  if (props.size === 'badge') return 'viz-ring-badge'
  return 'viz-ring-default'
})

function getSegmentStyle(i: number) {
  const deg = (i / props.segmentCount) * 360
  return {
    '--segment-index': i,
    '--segment-deg': `${deg}deg`,
  }
}
</script>

<style scoped>
.viz-ring {
  position: relative;
  pointer-events: none;
}

.viz-ring-default {
  width: 280px;
  height: 280px;
  --viz-radius: 140px;
}
@media (min-width: 640px) {
  .viz-ring-default {
    width: 360px;
    height: 360px;
    --viz-radius: 180px;
  }
}
@media (min-width: 768px) {
  .viz-ring-default {
    width: 400px;
    height: 400px;
    --viz-radius: 200px;
  }
}

.viz-ring-compact {
  width: 240px;
  height: 240px;
  --viz-radius: 120px;
}
@media (min-width: 768px) {
  .viz-ring-compact {
    width: 320px;
    height: 320px;
    --viz-radius: 160px;
  }
}

/* Paid-tick badge (FED-06) — sized to sit inside a modal card without
   clipping, unlike the compact variant (240-320px against a ~112px core). */
.viz-ring-badge {
  width: 160px;
  height: 160px;
  --viz-radius: 80px;
}
@media (min-width: 768px) {
  .viz-ring-badge {
    width: 192px;
    height: 192px;
    --viz-radius: 96px;
  }
}

.viz-segment {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 4px;
  height: 24px;
  margin-left: -2px;
  margin-top: -12px;
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.4), rgba(255, 255, 255, 0.1));
  border-radius: 2px;
  transform-origin: center center;
  transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius)));
  animation: segment-pulse 14s ease-in-out infinite;
  animation-delay: calc(var(--segment-index) * 0.02s);
}

@media (min-width: 768px) {
  .viz-segment {
    height: 28px;
    margin-top: -14px;
  }
}

/* 5 normal loops (10s) then stronger longer expression (4s) — total 14s */
@keyframes segment-pulse {
  0%   { opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  7.1% { opacity: 0.9; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
  14.3%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  21.4%{ opacity: 0.9; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
  28.6%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  35.7%{ opacity: 0.9; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
  42.9%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  50%  { opacity: 0.9; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
  57.1%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  64.3%{ opacity: 0.9; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
  71.4%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  78.6%{ opacity: 1;   transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1.5); }
  85.7%{ opacity: 1;   transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1.5); }
  92.9%{ opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
  100% { opacity: 0.3; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.4); }
}

/* Site-wide reduced-motion convention — applies to every size variant, so
   the ring holds a static, legible pose instead of pulsing. */
@media (prefers-reduced-motion: reduce) {
  .viz-segment {
    animation: none;
    opacity: 0.55;
  }
}
</style>
