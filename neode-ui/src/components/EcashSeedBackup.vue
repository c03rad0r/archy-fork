<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import SeedRevealPanel from '@/components/SeedRevealPanel.vue'

defineProps<{ setupOnly?: boolean }>()
const emit = defineEmits<{ ready: [] }>()

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
  source: 'node-seed' | 'independent' | 'imported' | null
  can_activate: boolean
  /** Whether a phrase can be *derived* from the node's recovery phrase. When
   *  false the wallet still gets a backup — it is just independent, and the
   *  operator has to keep it themselves. Saying which one they are about to
   *  get, before they write anything down, is the whole point of this flag. */
  derivable_from_node_seed: boolean
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
const showRevealPassphrase = ref(false)
const revealing = ref(false)
const revealError = ref('')
const revealedWords = ref<string[]>([])
const revealedSource = ref<string | null>(null)
const wordsCopied = ref(false)

function openReveal() {
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
  showRevealPassphrase.value = false
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
    const message = e instanceof Error ? e.message : 'Failed to reveal the ecash phrase'
    // Most operators used their login password as the backup passphrase. Do
    // not confront everyone with an unexplained third credential up front;
    // disclose it only when the authenticated password could not decrypt the
    // node seed and a distinct setup-time passphrase may actually exist.
    if (!status.value?.active && /could not decrypt the saved seed/i.test(message)) {
      showRevealPassphrase.value = true
      revealError.value = 'Your login password did not unlock the saved seed. Enter the separate backup passphrase you chose during setup.'
    } else {
      revealError.value = message
    }
  } finally {
    revealing.value = false
  }
}

function closeReveal() {
  const established = revealedWords.value.length > 0
  showRevealModal.value = false
  revealedWords.value = []
  revealPassword.value = ''
  revealCode.value = ''
  revealPassphrase.value = ''
  showRevealPassphrase.value = false
  if (established) emit('ready')
}

async function copyRevealedWords() {
  try {
    await navigator.clipboard.writeText(revealedWords.value.join(' '))
    wordsCopied.value = true
    setTimeout(() => { wordsCopied.value = false }, 2000)
  } catch { /* clipboard unavailable */ }
}

// ── Import ─────────────────────────────────────────────────────────────────
// Bring-your-own: point this wallet at another NUT-13 wallet's derivation, so
// coins held in Minibits, Nutstash or cdk-cli become restorable here.
//
// Replacing an established phrase is the one lossy thing on this screen. The
// coins already held stay spendable — they are proofs, not derivations — but
// they were minted under the old phrase, so a restore will no longer find
// them. Hence the explicit confirmation, and the reminder to write the
// current phrase down first.
const showImportModal = ref(false)
const importWords = ref('')
const importPassword = ref('')
const importCode = ref('')
const importConfirm = ref(false)
const importing = ref(false)
const importError = ref('')
const importDone = ref(false)

const importWordCount = computed(
  () => importWords.value.trim().split(/\s+/).filter(Boolean).length,
)

function openImport() {
  importWords.value = ''
  importPassword.value = ''
  importCode.value = ''
  importConfirm.value = false
  importError.value = ''
  importDone.value = false
  showImportModal.value = true
}

async function submitImport() {
  if (importing.value || !importPassword.value || importWordCount.value === 0) return
  importing.value = true
  importError.value = ''
  try {
    const params: Record<string, string | boolean> = {
      words: importWords.value.trim(),
      password: importPassword.value,
      confirm: importConfirm.value,
    }
    if (importCode.value) params.code = importCode.value
    await rpcClient.call({ method: 'wallet.ecash-seed-import', params })
    importDone.value = true
    importWords.value = ''
    await loadStatus()
  } catch (e: unknown) {
    importError.value = e instanceof Error ? e.message : 'Import failed'
  } finally {
    importing.value = false
  }
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

    <div class="flex flex-col gap-3">
      <div class="min-w-0">
        <h2 class="text-xl font-semibold text-white/96 mb-1">{{ setupOnly ? 'Set up your Cashu Lightning address' : 'Ecash backup phrase' }}</h2>

        <p v-if="status?.active && status?.source === 'node-seed'" class="text-sm leading-relaxed text-white/60">
          Your node's recovery phrase also recovers this ecash phrase. Reveal its 24 words
          to restore in a compatible Cashu wallet without sharing your node's phrase.
        </p>
        <p v-else-if="status?.active" class="text-sm leading-relaxed text-white/60">
          Save your 24-word ecash phrase to restore this wallet here or in another
          compatible Cashu wallet.
        </p>
        <p v-else class="text-sm leading-relaxed text-white/60">
          If this node's coin file is lost, your ecash is lost. Set up a phrase to recover
          future coins; existing coins aren't covered.
          <template v-if="status?.derivable_from_node_seed">
            Your node's recovery phrase will also recover this phrase.
          </template>
          <template v-else>
            This node has no saved seed, so write down and keep the new phrase separately.
          </template>
        </p>

        <p v-if="status?.source === 'independent' || status?.source === 'imported'" class="mt-2 text-xs text-orange-300/90">
          {{ status?.source === 'imported' ? 'This imported phrase' : 'This phrase' }} is separate
          from your node's backup. <strong>Only these words recover this ecash wallet.</strong>
        </p>

      </div>

      <button
        type="button"
        class="w-full glass-button rounded-lg px-4 py-2 text-sm font-medium"
        :class="!status?.active ? 'bg-orange-500/20 border-orange-400/30' : ''"
        @click="openReveal"
      >{{ status?.active ? 'Reveal' : (setupOnly ? 'Set up address' : 'Set up backup') }}</button>
    </div>

    <div v-if="status?.active && !setupOnly" class="mt-4 pt-4 border-t border-white/10">
      <div class="flex flex-col gap-3">
        <p class="text-sm leading-relaxed text-white/60 min-w-0">
          <span class="text-white/80 font-medium">Restore from this phrase.</span>
          Recover unspent coins from your mint. Safe to repeat; coins you already hold
          won't be duplicated.
        </p>
        <button
          type="button"
          class="w-full glass-button rounded-lg px-4 py-2 text-sm font-medium disabled:opacity-50"
          :disabled="restoring"
          @click="restoreFromPhrase"
        >{{ restoring ? 'Scanning…' : 'Restore' }}</button>
      </div>
      <p v-if="restoreMsg" role="status" aria-live="polite" class="mt-3 text-xs alert-success px-3 py-2 rounded-lg">{{ restoreMsg }}</p>
      <p v-if="restoreError" role="alert" class="mt-3 text-xs alert-error px-3 py-2 rounded-lg">{{ restoreError }}</p>
    </div>

    <div v-if="!setupOnly" class="mt-4 pt-4 border-t border-white/10">
      <div class="flex flex-col gap-3">
        <p class="text-sm leading-relaxed text-white/60 min-w-0">
          <span class="text-white/80 font-medium">Use a phrase from another wallet.</span>
          Import a phrase from Minibits, Nutstash or <span class="font-mono">cdk-cli</span>
          to restore its coins here.
        </p>
        <button
          type="button"
          class="w-full glass-button rounded-lg px-4 py-2 text-sm font-medium"
          @click="openImport"
        >Import</button>
      </div>
    </div>
  </div>

  <Teleport to="body">
    <div
      v-if="showImportModal"
      class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="showImportModal = false"
    >
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="import-ecash-seed-title">
        <h3 id="import-ecash-seed-title" class="text-lg font-semibold text-white mb-1">Import an ecash phrase</h3>

        <template v-if="importDone">
          <p class="text-sm text-white/70 my-4">
            Imported. This wallet now derives its coins from that phrase — run
            <span class="text-white/90 font-medium">Restore</span> to pull in the coins it
            owns at your mint.
          </p>
          <button type="button" @click="showImportModal = false" class="w-full glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30">Done</button>
        </template>

        <template v-else>
          <p class="text-sm leading-relaxed text-white/60 mb-4">
            Paste the 24-word phrase from the other wallet. The coins already in this wallet
            stay spendable either way.
          </p>
          <form @submit.prevent="submitImport" class="space-y-3">
            <div>
              <label class="block text-xs text-white/60 mb-1">
                Recovery phrase
                <span class="text-white/30">({{ importWordCount }} word{{ importWordCount === 1 ? '' : 's' }})</span>
              </label>
              <textarea v-model="importWords" rows="3" spellcheck="false" autocapitalize="none" autocomplete="off" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm font-mono focus:outline-none focus:border-white/30" placeholder="abandon abandon abandon …"></textarea>
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">Password</label>
              <input v-model="importPassword" type="password" autocomplete="current-password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Your login password" />
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">2FA code <span class="text-white/30">(if enabled)</span></label>
              <input v-model="importCode" inputmode="numeric" autocomplete="one-time-code" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm font-mono tracking-widest focus:outline-none focus:border-white/30" placeholder="123456" />
            </div>

            <label v-if="status?.active" class="flex items-start gap-2 text-xs text-orange-300/90 bg-orange-500/10 border border-orange-400/20 rounded-lg px-3 py-2">
              <input type="checkbox" v-model="importConfirm" class="mt-0.5 shrink-0" />
              <span>
                Replace this wallet's current phrase. Coins minted under the old one stay
                spendable but a restore will no longer find them — reveal and write the
                current phrase down first. The old phrase is archived on the node, not deleted.
              </span>
            </label>

            <p v-if="importError" role="alert" class="text-xs text-red-300 bg-red-500/10 border border-red-400/20 rounded-lg px-3 py-2">{{ importError }}</p>
            <div class="flex gap-2 pt-1">
              <button type="button" @click="showImportModal = false" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">Cancel</button>
              <button
                type="submit"
                :disabled="importing || !importPassword || importWordCount === 0 || (status?.active && !importConfirm)"
                class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50"
              >{{ importing ? 'Importing…' : 'Import' }}</button>
            </div>
          </form>
        </template>
      </div>
    </div>
  </Teleport>

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
          <p class="text-sm leading-relaxed text-white/60 mb-4">
            Confirm your credentials to {{ status?.active ? 'reveal' : 'set up' }} your ecash phrase.
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
            <div v-if="showRevealPassphrase">
              <label class="block text-xs text-white/60 mb-1">Separate backup passphrase</label>
              <input v-model="revealPassphrase" type="password" autocomplete="off" autofocus class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Passphrase chosen during setup" />
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
              Your node's recovery phrase recovers this ecash wallet too. Use these words
              separately in a compatible Cashu (NUT-13) wallet.
            </template>
            <template v-else>
              Write these words down. They are the <strong>only</strong> way to recover
              this ecash wallet; your node's phrase won't recover it.
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
