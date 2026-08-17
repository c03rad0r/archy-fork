<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import SeedRevealPanel from '@/components/SeedRevealPanel.vue'

// Ecash (Cashu) wallet backup card — the same shape as the node recovery
// phrase and the Lightning seed cards, deliberately: a third reveal pattern
// would be a third thing to learn.
//
// Two things make this one different from those:
//
//  1. Revealing is also *activating*. The node's master seed is encrypted at
//     rest, so this password prompt is the only moment the ecash phrase can be
//     derived from it. Until an operator comes here once, a node that predates
//     NUT-13 mints coins that no phrase can bring back — and the card says so
//     rather than implying a backup already exists.
//  2. These are standard BIP-39 words for a NUT-13 wallet, so they restore in
//     Minibits, Nutstash or cdk-cli. Hence `SeedRevealPanel` without `aezeed`:
//     the SeedQR tab is genuinely useful here.

type SeedStatus = {
  active: boolean
  source: 'node-seed' | 'independent' | null
  can_activate: boolean
}

const status = ref<SeedStatus | null>(null)
const statusLoaded = ref(false)

async function loadStatus() {
  try {
    status.value = await rpcClient.call<SeedStatus>({
      method: 'wallet.ecash-seed-status',
      timeout: 5000,
    })
    statusLoaded.value = true
  } catch {
    // A blip must not hide the card permanently — leave whatever we had.
  }
}

onMounted(loadStatus)

const showRevealModal = ref(false)
const revealPassword = ref('')
const revealCode = ref('')
const revealPassphrase = ref('')
const revealing = ref(false)
const revealError = ref('')
const revealedWords = ref<string[]>([])
const revealedSource = ref<string | null>(null)
const wordsCopied = ref(false)

function openReveal() {
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
  revealError.value = ''
  revealedWords.value = []
  showRevealModal.value = true
}

async function submitReveal() {
  if (revealing.value || !revealPassword.value) return
  revealing.value = true
  revealError.value = ''
  try {
    const params: Record<string, string> = { password: revealPassword.value }
    if (revealCode.value) params.code = revealCode.value
    if (revealPassphrase.value) params.passphrase = revealPassphrase.value
    const res = await rpcClient.call<{ words: string[]; source: string }>({
      method: 'wallet.ecash-seed-reveal',
      params,
    })
    revealedWords.value = res.words || []
    revealedSource.value = res.source ?? null
    // Activation may just have happened — refresh so the card stops offering
    // to set up a backup that now exists.
    void loadStatus()
  } catch (e: unknown) {
    revealError.value = e instanceof Error ? e.message : 'Failed to reveal the ecash phrase'
  } finally {
    revealing.value = false
  }
}

function closeReveal() {
  showRevealModal.value = false
  revealedWords.value = []
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
}

async function copyRevealedWords() {
  try {
    await navigator.clipboard.writeText(revealedWords.value.join(' '))
    wordsCopied.value = true
    setTimeout(() => { wordsCopied.value = false }, 2000)
  } catch { /* clipboard unavailable */ }
}

// ── Restore ────────────────────────────────────────────────────────────────
// The other half of the backup. Safe to run against a working wallet: the
// backend skips coins already held and never re-adds spent ones, so this is
// the button to reach for when the balance looks wrong, not just after a
// disaster.
const restoring = ref(false)
const restoreMsg = ref('')
const restoreError = ref('')

async function restoreFromPhrase() {
  if (restoring.value) return
  restoring.value = true
  restoreMsg.value = ''
  restoreError.value = ''
  try {
    const res = await rpcClient.call<{
      recovered_sats: number
      recovered_proofs: number
      already_spent: number
      keysets_scanned: number
    }>({ method: 'wallet.ecash-restore', timeout: 180000 })
    if (res.recovered_sats > 0) {
      restoreMsg.value = `Recovered ${res.recovered_sats.toLocaleString()} sats (${res.recovered_proofs} coins).`
    } else if (res.already_spent > 0) {
      restoreMsg.value = `Nothing to recover — the ${res.already_spent} coin(s) found at this mint were already spent.`
    } else {
      restoreMsg.value = `Nothing to recover: no coins from this phrase at this mint (${res.keysets_scanned} keyset(s) checked).`
    }
  } catch (e: unknown) {
    restoreError.value = e instanceof Error ? e.message : 'Restore failed'
  } finally {
    restoring.value = false
  }
}
</script>

<template>
  <div
    v-if="statusLoaded"
    class="glass-card px-6 py-6 mb-6"
    :class="!status?.active ? 'border border-orange-400/40' : ''"
  >
    <div v-if="!status?.active" class="flex items-center gap-2 mb-3 text-orange-300 text-sm font-medium" role="alert">
      <svg class="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M10.29 3.86l-8.4 14.55A1.5 1.5 0 003.19 21h17.62a1.5 1.5 0 001.3-2.59l-8.4-14.55a1.5 1.5 0 00-2.62 0z" />
      </svg>
      Your ecash has no backup yet
    </div>

    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h2 class="text-xl font-semibold text-white/96 mb-1">Ecash backup phrase</h2>

        <p v-if="status?.active" class="text-sm text-white/60">
          Your ecash wallet has its own 24-word phrase, derived from this node's recovery
          phrase — so the words you already wrote down cover your ecash too. Reveal it here
          if you want to restore your ecash into another wallet (Minibits, Nutstash,
          <span class="font-mono">cdk-cli</span>) without handing over the node's own seed.
        </p>
        <p v-else class="text-sm text-white/60">
          Ecash is a bearer instrument: the coins live in a file on this node, and right now
          nothing can bring them back if that file is lost. Setting up a backup phrase fixes
          that for every coin minted from then on. It's derived from this node's recovery
          phrase, so there's nothing new to write down.
        </p>

        <p v-if="status?.source === 'independent'" class="mt-2 text-xs text-orange-300/90">
          This wallet's phrase was <strong>not</strong> derived from the node's recovery
          phrase — restoring the node will not bring the ecash back. Write these words down
          separately.
        </p>
        <p v-if="!status?.active && !status?.can_activate" class="mt-2 text-xs text-orange-300/90">
          This node has no encrypted seed backup, so a phrase can't be derived from it.
        </p>
      </div>

      <button
        type="button"
        class="shrink-0 glass-button rounded-lg px-4 py-2 text-sm font-medium"
        :class="!status?.active ? 'bg-orange-500/20 border-orange-400/30' : ''"
        :disabled="!status?.active && !status?.can_activate"
        @click="openReveal"
      >{{ status?.active ? 'Reveal' : 'Set up backup' }}</button>
    </div>

    <div v-if="status?.active" class="mt-4 pt-4 border-t border-white/10">
      <div class="flex items-start justify-between gap-4">
        <p class="text-sm text-white/60 min-w-0">
          <span class="text-white/80 font-medium">Restore from this phrase.</span>
          Asks your mint which coins it has signed for these words and puts back any that
          are still unspent. Safe to run at any time — it never duplicates coins you already
          hold.
        </p>
        <button
          type="button"
          class="shrink-0 glass-button rounded-lg px-4 py-2 text-sm font-medium disabled:opacity-50"
          :disabled="restoring"
          @click="restoreFromPhrase"
        >{{ restoring ? 'Scanning…' : 'Restore' }}</button>
      </div>
      <p v-if="restoreMsg" role="status" aria-live="polite" class="mt-3 text-xs alert-success px-3 py-2 rounded-lg">{{ restoreMsg }}</p>
      <p v-if="restoreError" role="alert" class="mt-3 text-xs alert-error px-3 py-2 rounded-lg">{{ restoreError }}</p>
    </div>
  </div>

  <Teleport to="body">
    <div
      v-if="showRevealModal"
      class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="closeReveal"
    >
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="reveal-ecash-seed-title">
        <h3 id="reveal-ecash-seed-title" class="text-lg font-semibold text-white mb-1">
          {{ status?.active ? 'Reveal ecash phrase' : 'Set up ecash backup' }}
        </h3>

        <template v-if="revealedWords.length === 0">
          <p class="text-sm text-white/60 mb-4">
            Confirm your credentials to
            {{ status?.active ? 'display the 24-word ecash phrase' : 'derive and display your ecash backup phrase' }}.
          </p>
          <form @submit.prevent="submitReveal" class="space-y-3">
            <div>
              <label class="block text-xs text-white/60 mb-1">Password</label>
              <input v-model="revealPassword" type="password" autocomplete="current-password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Your login password" />
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">2FA code <span class="text-white/30">(if enabled)</span></label>
              <input v-model="revealCode" inputmode="numeric" autocomplete="one-time-code" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm font-mono tracking-widest focus:outline-none focus:border-white/30" placeholder="123456" />
            </div>
            <div v-if="!status?.active">
              <label class="block text-xs text-white/60 mb-1">Backup passphrase <span class="text-white/30">(only if different from password)</span></label>
              <input v-model="revealPassphrase" type="password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Leave blank to use password" />
            </div>
            <p v-if="revealError" class="text-xs text-red-300 bg-red-500/10 border border-red-400/20 rounded-lg px-3 py-2">{{ revealError }}</p>
            <div class="flex gap-2 pt-1">
              <button type="button" @click="closeReveal" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">Cancel</button>
              <button type="submit" :disabled="revealing || !revealPassword" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50">
                {{ revealing ? 'Verifying…' : (status?.active ? 'Reveal' : 'Set up') }}
              </button>
            </div>
          </form>
        </template>

        <template v-else>
          <SeedRevealPanel :words="revealedWords" />
          <p class="text-xs text-white/40 mt-3">
            <template v-if="revealedSource === 'node-seed'">
              Derived from this node's recovery phrase — restoring the node restores this
              ecash wallet too. These words also restore it into any NUT-13 wallet.
            </template>
            <template v-else>
              This phrase is independent of the node's recovery phrase. It is the
              <strong>only</strong> way to restore this ecash wallet — write it down.
            </template>
          </p>
          <div class="flex gap-2 pt-4">
            <button type="button" @click="copyRevealedWords" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">{{ wordsCopied ? 'Copied!' : 'Copy' }}</button>
            <button type="button" @click="closeReveal" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30">Done</button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>
