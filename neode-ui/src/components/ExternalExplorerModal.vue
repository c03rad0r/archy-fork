<template>
  <!-- z-3600: this consent prompt can be triggered from INSIDE another
       modal (e.g. Wallet Settings → Channels → funding tx), so it must sit
       above the standard modal layer (3000) but below the app overlay (4000). -->
  <BaseModal
    :show="!!explorer.pendingTx.value"
    title="Open on an external explorer?"
    max-width="max-w-md"
    z-index="z-[3600]"
    @close="explorer.cancelPending()"
  >
    <!-- Same visual language as the uninstall keep-your-data warning: amber
         caution card, plain words about exactly what is shared, explicit
         choice — never a silent hand-off to a third-party server. -->
    <div class="p-3 rounded-lg border border-amber-400/25 bg-amber-500/10 text-amber-200/90 text-sm">
      <p class="font-medium mb-1">⚠️ This node doesn't run its own Mempool explorer</p>
      <p class="text-amber-200/75 text-xs leading-relaxed">
        (Pruned Bitcoin nodes can't index the full chain.) Your transaction will open on
        <span class="font-medium">another node's mempool</span> — that server's operator can see
        which transaction you looked up, along with your IP address. Use a server you trust,
        or your own mempool instance if you have one elsewhere.
      </p>
    </div>

    <div class="mt-4">
      <label class="block text-sm text-white/80 mb-1">Explorer to use</label>
      <input
        v-model="url"
        :placeholder="EXPLORER_PLACEHOLDER"
        spellcheck="false"
        class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm font-mono focus:outline-none focus:border-orange-400/60"
      />
      <p class="text-[11px] text-white/40 mt-1">
        Defaults to tx1138.com. Any Mempool-compatible instance works — you can change this
        any time in Settings → System.
      </p>
    </div>

    <label class="flex items-center gap-2 mt-4 text-sm text-white/70 cursor-pointer select-none">
      <input type="checkbox" v-model="dontAskAgain" class="accent-orange-400" />
      Remember my choice and don't ask again
    </label>

    <div class="flex gap-2 mt-6">
      <button class="flex-1 glass-button px-4 py-2 rounded-lg text-sm" @click="explorer.cancelPending()">
        Cancel
      </button>
      <button
        class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium"
        @click="explorer.confirmPending(url, dontAskAgain)"
      >
        Open Explorer
      </button>
    </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import BaseModal from '@/components/BaseModal.vue'
import { useTxExplorer, EXPLORER_PLACEHOLDER } from '@/composables/useTxExplorer'

const explorer = useTxExplorer()
const url = ref(explorer.prefs.value.url)
const dontAskAgain = ref(false)

// Re-seed the input each time the modal opens (prefs may have changed in
// Settings since the last time).
watch(explorer.pendingTx, (tx) => {
  if (tx) {
    url.value = explorer.prefs.value.url
    dontAskAgain.value = false
  }
})
</script>
