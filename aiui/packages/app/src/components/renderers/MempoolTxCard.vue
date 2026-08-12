<template>
  <div class="rounded-xl bg-white/5 border border-white/10 p-3 space-y-2">
    <div class="flex items-center gap-2">
      <span class="text-xs text-accent/60 uppercase tracking-wider font-bold">Transaction</span>
      <span
        v-if="txData"
        class="text-xs px-1.5 py-0.5 rounded"
        :class="txData.confirmed ? 'bg-emerald-400/15 text-emerald-400/80' : 'bg-yellow-400/15 text-yellow-400/80'"
      >
        {{ txData.confirmed ? `${txData.confirmations} confirmations` : 'Unconfirmed' }}
      </span>
    </div>

    <!-- TXID -->
    <p class="text-xs font-mono text-white/40 break-all select-all">{{ txid }}</p>

    <!-- TX details -->
    <div v-if="txData" class="grid grid-cols-2 gap-2 text-xs">
      <div>
        <p class="text-white/25">Fee</p>
        <p class="text-white/60 tabular-nums">{{ txData.fee.toLocaleString() }} sats</p>
      </div>
      <div>
        <p class="text-white/25">Fee Rate</p>
        <p class="text-white/60 tabular-nums">{{ txData.feeRate }} sat/vB</p>
      </div>
      <div>
        <p class="text-white/25">Size</p>
        <p class="text-white/60 tabular-nums">{{ txData.size }} vB</p>
      </div>
      <div>
        <p class="text-white/25">Block</p>
        <p class="text-white/60 tabular-nums">{{ txData.blockHeight ?? 'Pending' }}</p>
      </div>
    </div>

    <div v-else-if="isLoading" class="py-2">
      <p class="text-xs text-white/30">Loading transaction...</p>
    </div>

    <div v-else-if="error" class="py-2">
      <p class="text-xs text-red-400/60">{{ error }}</p>
    </div>

    <a
      :href="`https://mempool.space/tx/${txid}`"
      target="_blank"
      rel="noopener noreferrer"
      class="block w-full py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
    >
      View on Mempool.space
    </a>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'

const props = defineProps<{
  txid: string
}>()

interface TxInfo {
  fee: number
  feeRate: number
  size: number
  confirmed: boolean
  confirmations: number
  blockHeight: number | null
}

const txData = ref<TxInfo | null>(null)
const isLoading = ref(true)
const error = ref('')

async function fetchTx() {
  isLoading.value = true
  error.value = ''

  try {
    const res = await fetch(`https://mempool.space/api/tx/${props.txid}`)
    if (!res.ok) throw new Error('Transaction not found')

    const data = await res.json()
    const confirmed = !!data.status?.confirmed

    let confirmations = 0
    if (confirmed && data.status?.block_height) {
      const tipRes = await fetch('https://mempool.space/api/blocks/tip/height')
      if (tipRes.ok) {
        const tipHeight = parseInt(await tipRes.text())
        confirmations = tipHeight - data.status.block_height + 1
      }
    }

    txData.value = {
      fee: data.fee ?? 0,
      feeRate: data.weight ? Math.round((data.fee / data.weight) * 4) : 0,
      size: data.weight ? Math.round(data.weight / 4) : data.size ?? 0,
      confirmed,
      confirmations,
      blockHeight: data.status?.block_height ?? null,
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load transaction'
  } finally {
    isLoading.value = false
  }
}

onMounted(() => {
  fetchTx()
})
</script>
