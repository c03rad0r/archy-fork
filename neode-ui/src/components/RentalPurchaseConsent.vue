<template>
  <div v-if="request" class="absolute inset-0 z-[81] flex items-center justify-center bg-black/80 p-3 backdrop-blur-sm">
    <section ref="modal" role="dialog" aria-modal="true" aria-label="Confirm video rental" :aria-busy="busy" class="glass-card max-h-[90%] w-full max-w-lg overflow-y-auto rounded-2xl border border-white/15 p-5 text-white shadow-2xl sm:p-7">
      <header class="flex items-start gap-3">
        <div class="min-w-0 flex-1"><p class="text-xs uppercase tracking-widest text-white/45">IndeeHub · Node rental</p><h2 class="mt-2 break-words text-2xl font-bold">{{ request.title }}</h2></div>
        <button type="button" aria-label="Close rental" class="min-h-11 min-w-11 rounded-lg text-white/60 hover:bg-white/10 hover:text-white" @click="$emit('cancel')">✕</button>
      </header>
      <div class="my-6 rounded-xl border border-white/10 bg-white/5 p-5">
        <div class="flex items-start justify-between gap-4"><div><h3 class="text-lg font-semibold">Rental details</h3><p class="mt-1 text-sm text-white/60">{{ Math.ceil(request.terms.viewingSeconds / 60) }} minutes from when you press Start viewing</p></div><div class="shrink-0 text-right"><p class="text-2xl font-bold">{{ request.terms.priceSats.toLocaleString() }} <span class="text-sm font-normal text-white/55">sats</span></p><p class="text-xs text-white/50">Film price</p></div></div>
        <div v-if="quote" class="mt-5 space-y-2 border-t border-white/10 pt-4 text-sm"><div class="flex justify-between gap-3"><span class="text-white/60">Payment method</span><span>Node wallet · Cashu</span></div><div class="flex justify-between gap-3"><span class="text-white/60">Network</span><span>{{ quote.network === 'testnet' ? 'Testnet' : 'Mainnet' }}</span></div><div class="flex justify-between gap-3 border-t border-white/10 pt-3 font-semibold"><span>Total wallet debit, including fees</span><span>{{ quote.wallet_debit_sats.toLocaleString() }} sats</span></div><details class="pt-2 text-xs text-white/45"><summary class="cursor-pointer">Mint details</summary><p class="mt-2 break-all">{{ quote.mint_url }}</p></details></div>
      </div>
      <p class="text-sm leading-relaxed text-white/60">No viewing time starts until you press Start viewing. Reopening this title recovers the same purchase; it must not charge you twice.</p>
      <p v-if="error" role="alert" class="mt-4 rounded-lg border border-red-400/25 bg-red-500/10 p-3 break-words text-sm text-red-200">{{ error }}</p>
      <p v-if="busy" role="status" class="mt-4 text-sm text-white/70">{{ phase === 'preparing' ? progress : phase === 'paying' ? 'Completing or recovering the original payment…' : 'Checking the original purchase and current fees…' }}</p>
      <button type="button" v-if="quote" :disabled="busy" class="mt-4 min-h-11 w-full rounded-lg border border-white/20 text-sm text-white/70 disabled:opacity-40" @click="$emit('cancelUnpaid')">Cancel unpaid quote</button>
      <footer class="mt-6 flex flex-col-reverse gap-3 sm:flex-row"><button type="button" class="glass-button min-h-11 flex-1 rounded-lg px-4" @click="$emit('cancel')">Close</button><button type="button" :disabled="busy" class="glass-button min-h-11 flex-1 rounded-lg border border-orange-400/30 px-4 font-semibold text-orange-200 disabled:opacity-40" @click="phase === 'confirm' ? $emit('approve') : $emit('review')">{{ phase === 'confirm' ? `Pay ${quote?.wallet_debit_sats} sats from node wallet` : 'Check purchase' }}</button></footer>
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

