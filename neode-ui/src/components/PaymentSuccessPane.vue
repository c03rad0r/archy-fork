<template>
  <div class="text-center py-4">
    <div class="send-success-badge mx-auto mb-6">
      <ScreensaverRing size="badge" />
      <div class="send-success-burst">
        <div class="burst-core">
          <svg class="w-14 h-14 text-green-400 burst-check" fill="none" stroke="currentColor" stroke-width="3" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </div>
      </div>
    </div>

    <div v-if="amount > 0" class="text-5xl font-black text-green-400 mb-1">
      {{ amount.toLocaleString() }}<span class="text-2xl font-bold text-green-400/70"> sats</span>
    </div>
    <div class="text-2xl font-bold tracking-widest text-white mb-1">{{ verb }}</div>
    <p v-if="methodLabel" class="text-sm text-white/50 mb-6">{{ methodLabel }}</p>

    <!-- Everything a person needs to hand to whoever can help them if this
         payment is later disputed or goes missing. Ecash has no public
         ledger to look anything up in afterwards, so if it isn't copyable
         here it is gone. -->
    <div v-if="rows.length || note" class="p-4 bg-white/5 rounded-xl text-left space-y-4 mb-6">
      <div v-for="row in rows" :key="row.label">
        <p class="text-xs text-white/50 mb-1">{{ row.label }}</p>
        <div class="flex items-center gap-2">
          <p class="flex-1 text-xs font-mono text-white/80 break-all" :class="row.truncate ? 'line-clamp-3' : ''">{{ row.value }}</p>
          <CopyButton class="shrink-0" :value="row.value" />
        </div>
        <p v-if="row.hint" class="text-[11px] text-white/40 mt-1">{{ row.hint }}</p>
      </div>
      <p v-if="note" class="text-xs text-white/60">{{ note }}</p>
    </div>

    <div class="flex gap-3">
      <button
        v-if="againLabel"
        type="button"
        class="flex-1 glass-button px-4 py-3 rounded-xl text-sm font-medium"
        @click="emit('again')"
      >{{ againLabel }}</button>
      <button
        type="button"
        class="flex-1 glass-button glass-button-warning px-4 py-3 rounded-xl text-sm font-semibold"
        @click="emit('done')"
      >{{ t('common.done') || 'Done' }}</button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * The payment "moment" screen, shared by every money flow.
 *
 * Extracted from SendBitcoinModal so Cashu and Fedimint show the *same*
 * screen rather than a lookalike, and so the copyable-identifier row is
 * defined once. Each caller supplies whatever identifiers its protocol has
 * — a payment hash, a txid, a mint URL, a quote id, or the token itself.
 */
import { useI18n } from 'vue-i18n'
import CopyButton from '@/components/CopyButton.vue'
import ScreensaverRing from '@/components/ScreensaverRing.vue'

export interface SuccessRow {
  label: string
  value: string
  /** Extra context under the value, e.g. why someone would need it. */
  hint?: string
  /** Clamp very long values (a whole ecash token) instead of flooding the pane. */
  truncate?: boolean
}

withDefaults(
  defineProps<{
    amount: number
    /** SENT / RECEIVED / REDEEMED … */
    verb: string
    methodLabel?: string
    rows?: SuccessRow[]
    note?: string
    againLabel?: string
  }>(),
  { methodLabel: '', rows: () => [], note: '', againLabel: '' },
)

const emit = defineEmits<{ again: []; done: [] }>()
const { t } = useI18n()
</script>

<style scoped>
/* Success badge (FED-06) — the branded ScreensaverRing carries the motion,
   with the emerald pop-in check centred over it. */
.send-success-badge {
  position: relative;
  width: 160px;
  height: 160px;
  display: flex;
  align-items: center;
  justify-content: center;
}
@media (min-width: 768px) {
  .send-success-badge {
    width: 192px;
    height: 192px;
  }
}
.send-success-burst {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 7rem;
  height: 7rem;
}
.burst-core {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: rgba(16, 185, 129, 0.12);
  box-shadow: 0 0 48px rgba(16, 185, 129, 0.3);
  animation: burst-pop 0.5s cubic-bezier(0.175, 0.885, 0.32, 1.4) both;
}
.burst-check {
  stroke-dasharray: 32;
  stroke-dashoffset: 32;
  animation: burst-draw 0.45s ease-out 0.25s forwards;
}
@keyframes burst-pop {
  from { transform: scale(0.3); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
@keyframes burst-draw {
  to { stroke-dashoffset: 0; }
}
@media (prefers-reduced-motion: reduce) {
  .burst-core, .burst-check { animation: none; }
  .burst-check { stroke-dashoffset: 0; }
}
</style>
