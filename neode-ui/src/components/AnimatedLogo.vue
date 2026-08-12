<template>
  <div
    class="flex-shrink-0 inline-block overflow-hidden"
    :class="[
      sizeClass,
      !noBorder && 'logo-gradient-border'
    ]"
  >
    <!-- Neode logo - always white -->
    <svg
      class="block w-full h-full logo-svg"
      viewBox="0 0 1024 1024"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-label="Neode"
    >
      <rect width="1024" height="1024" fill="#030202" />
      <rect
        v-for="(r, i) in rects"
        :key="i"
        :x="r.x"
        :y="r.y"
        :width="r.w"
        :height="r.h"
        fill="white"
        class="logo-square"
        :style="{ '--delay': delays[i] + 'ms' }"
      />
    </svg>
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(defineProps<{
  size?: 'sm' | 'lg' | 'xl'
  noBorder?: boolean
  /** When true, fit to container (w-full h-full) instead of fixed size - for use inside logo-gradient-border */
  fit?: boolean
}>(), { size: 'sm', noBorder: false, fit: false })

const sizeClass = props.fit
  ? 'w-full h-full max-w-full max-h-full'
  : props.size === 'xl'
    ? 'w-48 h-48 sm:w-64 sm:h-64 md:w-80 md:h-80'
    : props.size === 'lg'
      ? 'w-32 h-32 sm:w-48 sm:h-48'
      : 'w-14 h-14'

// Parsed from favico-black.svg path - 20 rects
const rects = [
  { x: 357.614, y: 318, w: 71.007, h: 70.936 },
  { x: 436.152, y: 318, w: 72.082, h: 70.936 },
  { x: 515.766, y: 318, w: 72.082, h: 70.936 },
  { x: 595.379, y: 318, w: 71.007, h: 70.936 },
  { x: 595.379, y: 396.46, w: 71.007, h: 72.011 },
  { x: 673.917, y: 396.46, w: 72.083, h: 72.011 },
  { x: 278, y: 475.994, w: 72.083, h: 72.012 },
  { x: 357.614, y: 475.994, w: 71.007, h: 72.012 },
  { x: 436.152, y: 475.994, w: 72.082, h: 72.012 },
  { x: 515.766, y: 475.994, w: 72.082, h: 72.012 },
  { x: 595.379, y: 475.994, w: 71.007, h: 72.012 },
  { x: 673.917, y: 475.994, w: 72.083, h: 72.012 },
  { x: 278, y: 555.529, w: 72.083, h: 70.936 },
  { x: 357.614, y: 555.529, w: 71.007, h: 70.936 },
  { x: 595.379, y: 555.529, w: 71.007, h: 70.936 },
  { x: 673.917, y: 555.529, w: 72.083, h: 70.936 },
  { x: 357.614, y: 633.989, w: 71.007, h: 72.011 },
  { x: 436.152, y: 633.989, w: 72.082, h: 72.011 },
  { x: 515.766, y: 633.989, w: 72.082, h: 72.011 },
  { x: 595.379, y: 633.989, w: 71.007, h: 72.011 },
]

// Stagger delays (ms) - row-by-row top-to-bottom, left-to-right for a clean reveal
const delays = [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600, 1700, 1800, 1900]
</script>

<style scoped>
.logo-svg {
  will-change: auto;
}

.logo-square {
  opacity: 0;
  animation: logo-square-in 3s ease-out infinite;
  animation-delay: var(--delay, 0ms);
  animation-fill-mode: both;
}

/* Fade in only - no scale/position change. Squares stay fixed. */
@keyframes logo-square-in {
  0% { opacity: 0; }
  15% { opacity: 1; }
  100% { opacity: 1; }
}
</style>
