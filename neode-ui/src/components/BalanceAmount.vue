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

// 14 columns × 3 rows, laid out column-first so all three cells of a column
// share a delay and the lit column travels across as a single scan line —
// that is what makes it read as a readout rather than a progress bar.
const COLUMNS = 14
const ROWS = 3
const CELLS = COLUMNS * ROWS
const STEP_MS = 55

/** Delay for cell `i` (1-based), constant within a column. */
function cellDelay(i: number): string {
  return `${Math.floor((i - 1) / ROWS) * STEP_MS}ms`
}

/**
 * Built as one string rather than interpolated around a `<template>`, so the
 * space before the unit cannot be eaten by Vue's whitespace condensing — and
 * so a test reading `.text()` sees exactly what a person reads on screen.
 */
/**
 * Is there a figure to show at all?
 *
 * `null`/`undefined` mean "not known yet" — but so does a NaN or an Infinity,
 * which is what arithmetic on a missing field quietly produces. Those render
 * as the literal text "NaN sats", which is worse than the zero this component
 * exists to prevent: at least a zero looks like a number.
 */
const known = computed(() => props.sats != null && Number.isFinite(props.sats))

const display = computed(() => {
  if (!known.value) return ''
  const figure = (props.sats as number).toLocaleString()
  return props.suffix ? `${figure} ${props.suffix}` : figure
})
</script>

<template>
  <span
    v-if="!known"
    class="balance-pixels"
    role="status"
    aria-live="polite"
    :aria-label="`Loading ${props.label}`"
    :title="`Loading ${props.label}…`"
  >
    <span v-for="i in CELLS" :key="i" class="balance-pixel" :style="{ animationDelay: cellDelay(i) }" />
  </span>
  <span v-else>{{ display }}</span>
</template>

<style scoped>
.balance-pixels {
  display: inline-grid;
  /* Column-first: children fill top-to-bottom, then across, so consecutive
     cells share a column and the delay below scans horizontally. */
  grid-auto-flow: column;
  grid-template-rows: repeat(3, 3px);
  grid-auto-columns: 3px;
  gap: 1px;
  /* Centred on the text it stands in for, so the row height is unchanged when
     the real figure replaces it. */
  vertical-align: 0.05em;
}

.balance-pixel {
  width: 3px;
  height: 3px;
  border-radius: 0.5px;
  background: currentColor;
  opacity: 0.16;
  animation: balance-pixel-scan 1.6s ease-in-out infinite;
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
