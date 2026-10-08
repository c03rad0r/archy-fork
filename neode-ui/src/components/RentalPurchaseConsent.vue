<template>
  <div v-if="request" class="absolute inset-0 z-[81] flex items-center justify-center bg-black/70 p-3">
    <section ref="modal" role="dialog" aria-modal="true" aria-label="Confirm video rental" :aria-busy="busy" class="glass-card max-h-[90%] w-full max-w-md overflow-y-auto rounded-2xl p-5 text-white">
      <p class="text-xs text-white/50">IndeeHub · Your node wallet</p>
      <h2 class="mt-2 break-words text-xl font-semibold">{{ request.title }}</h2>
      <dl class="my-4 space-y-2 text-sm"><div>Rental: {{ request.terms.priceSats }} sats</div><div>Viewing period: {{ Math.ceil(request.terms.viewingSeconds / 60) }} minutes after you press Start</div><template v-if="quote"><div>Payment: Cashu · {{ quote.network === 'testnet' ? 'Testnet' : 'Mainnet' }}</div><div class="break-all">Mint: {{ quote.mint_url }}</div></template><div v-if="quote" class="font-semibold">Total wallet debit: {{ quote.wallet_debit_sats }} sats</div></dl>
      <p class="text-sm text-white/60">Review the exact wallet debit before paying. If a response is lost, reopen this title to recover the same purchase.</p>
      <p v-if="error" role="alert" class="mt-3 break-words text-sm text-red-200">{{ error }}</p>
      <p v-if="busy" role="status" class="mt-4 text-sm">{{ phase === 'preparing' ? progress : phase === 'paying' ? 'Recovering or completing your payment…' : 'Checking the original purchase and current fees…' }}</p>
      <button type="button" v-if="quote" :disabled="busy" class="mt-3 min-h-11 w-full rounded-lg border border-white/20 text-sm disabled:opacity-40" @click="$emit('cancelUnpaid')">Cancel unpaid quote</button>
      <footer class="mt-5 flex flex-col gap-2 sm:flex-row"><button type="button" class="glass-button min-h-11 flex-1 rounded-lg px-3" @click="$emit('cancel')">Close</button><button type="button" :disabled="busy" class="glass-button min-h-11 flex-1 rounded-lg px-3 text-orange-200 disabled:opacity-40" @click="phase === 'confirm' ? $emit('approve') : $emit('review')">{{ phase === 'confirm' ? `Pay ${quote?.wallet_debit_sats} sats` : 'Check purchase' }}</button></footer>
    </section>
  </div>
</template>
<script setup lang="ts">
import { computed, ref } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import type { RentalOffer } from '@/composables/useRentalPurchaseBridge'
const props = defineProps<{ request: RentalOffer | null; quote: { wallet_debit_sats: number; network: 'mainnet' | 'testnet'; mint_url: string } | null; phase: string; error: string; progress?: string }>()
const emit = defineEmits<{ review: []; approve: []; cancel: []; cancelUnpaid: [] }>()
const modal = ref<HTMLElement | null>(null)
useModalKeyboard(modal, computed(() => Boolean(props.request)), () => emit('cancel'))
const busy = computed(() => props.phase === 'preparing' || props.phase === 'loading' || props.phase === 'paying')
</script>

