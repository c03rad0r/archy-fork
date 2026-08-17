<script setup lang="ts">
import { computed } from 'vue'
/**
 * A balance figure, or — while it is still unknown — a pixel readout in place
 * of it.
 *
 * The problem this exists for: an unloaded balance used to render as `0`.
 * Zero is not "loading", it is a *number*, and it is the one number that
 * frightens people. Someone opening the dashboard while the RPCs are still in
 * flight was told, in the wallet's own typeface, that their money was gone.
 * There is no formatting fix for that — the fix is to not claim a figure we
 * do not have yet.
 *
 * So `sats` is nullable, and `null` means "not known yet" rather than "none".
 * Callers must keep that distinction alive: a balance ref should start at
 * `null` and only become a number when a call actually succeeds.
 *
 * The placeholder is a small dot-matrix that scans in the rail's own colour —
 * it inherits `currentColor`, so the on-chain row shimmers orange, Lightning
 * yellow, Cashu purple, Fedimint blue and Ark teal with no colour mapping to
 * keep in sync. It is deliberately about as wide as the figure it stands in
 * for, so nothing jumps when the real number lands.
 */

const props = withDefaults(
  defineProps<{
    /** Balance in sats, or null/undefined while it is still unknown. */
    sats: number | null | undefined
    /** Trailing unit. Set to '' for bare figures. */
    suffix?: string
    /** Named for screen readers, e.g. "on-chain balance". */
    label?: string
  }>(),
  { suffix: 'sats', label: 'balance' },
)

// 14 columns × 2 rows. Enough to read as a matrix rather than a spinner, and
// close to the width of a five-figure sat amount.
const CELLS = 28

/**
 * Built as one string rather than interpolated around a `<template>`, so the
 * space before the unit cannot be eaten by Vue's whitespace condensing — and
 * so a test reading `.text()` sees exactly what a person reads on screen.
 */
const display = computed(() => {
  if (props.sats == null) return ''
  const figure = props.sats.toLocaleString()
  return props.suffix ? `${figure} ${props.suffix}` : figure
})
</script>

<template>
  <span
    v-if="props.sats == null"
    class="balance-pixels"
    role="status"
    aria-live="polite"
    :aria-label="`Loading ${props.label}`"
    :title="`Loading ${props.label}…`"
  >
    <span v-for="i in CELLS" :key="i" class="balance-pixel" :style="{ '--i': i }" />
  </span>
  <span v-else>{{ display }}</span>
</template>

<style scoped>
.balance-pixels {
  display: inline-grid;
  grid-template-columns: repeat(14, 3px);
  grid-auto-rows: 3px;
  gap: 1px;
  /* Sit on the text baseline so a row's height doesn't change when the real
     figure replaces this. */
  vertical-align: -1px;
}

.balance-pixel {
  width: 3px;
  height: 3px;
  border-radius: 0.5px;
  background: currentColor;
  opacity: 0.16;
  /* The wave runs left-to-right across columns; the two rows of a column are
     offset slightly so it reads as a scan rather than a marching block. */
  animation: balance-pixel-scan 1.4s ease-in-out infinite;
  animation-delay: calc(var(--i) * 45ms);
}

@keyframes balance-pixel-scan {
  0%, 70%, 100% { opacity: 0.16; }
  25% { opacity: 1; }
  45% { opacity: 0.42; }
}

/* Motion is decoration here — the dimmed matrix still reads as "no figure
   yet", which is the part that carries the meaning. */
@media (prefers-reduced-motion: reduce) {
  .balance-pixel {
    animation: none;
    opacity: 0.35;
  }
}
</style>
