<template>
  <main class="min-h-screen text-white p-6">
    <div class="max-w-md mx-auto py-8 space-y-4">
      <p class="text-xs text-orange-200">UI PREVIEW · SAMPLE DATA · NO WALLET ACCESS</p>
      <h1 class="text-2xl font-semibold">Bump a transaction</h1>
      <p class="text-sm text-white/55">Open the transaction list and tap the small Bump button. You can try both supported methods and the custom fee review without spending anything.</p>
      <div class="flex gap-2">
        <button class="rounded-lg px-4 py-2 bg-white/10 text-sm" @click="open('cpfp')">Preview CPFP</button>
        <button class="rounded-lg px-4 py-2 bg-white/10 text-sm" @click="open('rbf')">Preview RBF</button>
      </div>
    </div>
    <TransactionsModal :key="revision" :show="show" :transactions="transactions" @close="show = false" />
  </main>
</template>
<script setup lang="ts">
import { ref } from 'vue'
import TransactionsModal from '@/components/TransactionsModal.vue'
import { choose } from './rpc'
const show = ref(false)
const revision = ref(0)
const transactions = [{ tx_hash: 'a'.repeat(64), amount_sats: 161794, direction: 'outgoing' as const, num_confirmations: 0, time_stamp: Math.floor(Date.now()/1000)-120, total_fees: 144, dest_addresses: [], label: '', block_height: 0 }]
function open(method: string) { choose(method); revision.value++; show.value = true }
</script>
