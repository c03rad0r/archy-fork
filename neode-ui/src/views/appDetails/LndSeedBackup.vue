<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import SeedRevealPanel from '@/components/SeedRevealPanel.vue'

// Lightning seed (aezeed) backup card. Shown on the LND app detail page.
// The backend captures the aezeed at wallet-init time; until the user
// confirms writing it down (`acknowledged`), the card renders as a
// prominent first-launch prompt.

const route = useRoute()
const router = useRouter()

const available = ref(false)
const acknowledged = ref(true)
const statusLoaded = ref(false)

async function loadStatus(): Promise<boolean> {
  try {
    const res = await rpcClient.call<{ available: boolean; acknowledged: boolean }>({
      method: 'lnd.seed-backup-status',
      timeout: 5000,
    })
    available.value = !!res.available
    acknowledged.value = !!res.acknowledged
    statusLoaded.value = true
    return true
  } catch {
    // Leave statusLoaded as-is; a one-off RPC blip must not permanently
    // hide the card (the global banner may have just promised it's here).
    return false
  }
}

onMounted(async () => {
  // Retry a few times — on fresh installs the backend can still be warming
  // up when the user lands here straight from the backup banner.
  for (let attempt = 0; attempt < 4; attempt++) {
    if (await loadStatus()) break
    await new Promise((r) => setTimeout(r, 1500 * (attempt + 1)))
  }
  // Deep link from the "Back up your Lightning seed" banner: open the
  // reveal flow directly so the click visibly does something.
  if (route.query['seed-backup'] === '1') {
    router.replace({ query: { ...route.query, 'seed-backup': undefined } }).catch(() => {})
    if (statusLoaded.value && available.value) openReveal()
  }
})

// Reveal modal — re-auth gated (password + 2FA when enabled), same UX as
// the recovery-phrase reveal in Settings → Backup.
const showRevealModal = ref(false)
const revealPassword = ref('')
const revealCode = ref('')
const revealing = ref(false)
const revealError = ref('')
const revealedWords = ref<string[]>([])
const wordsHidden = ref(true)
const wordsCopied = ref(false)
const acking = ref(false)

function openReveal() {
  revealPassword.value = ''
  revealCode.value = ''
  revealError.value = ''
  revealedWords.value = []
  wordsHidden.value = true
  showRevealModal.value = true
}

async function submitReveal() {
  if (revealing.value || !revealPassword.value) return
  revealing.value = true
  revealError.value = ''
  try {
    const params: Record<string, string> = { password: revealPassword.value }
    if (revealCode.value) params.code = revealCode.value
    const res = await rpcClient.call<{ words: string[] }>({ method: 'lnd.seed-reveal', params })
    revealedWords.value = res.words || []
    wordsHidden.value = true
  } catch (e: unknown) {
    revealError.value = e instanceof Error ? e.message : 'Failed to reveal the Lightning seed'
  } finally {
    revealing.value = false
  }
}

function closeReveal() {
  showRevealModal.value = false
  revealedWords.value = []
  revealPassword.value = ''
  revealCode.value = ''
}

async function copyRevealedWords() {
  try {
    await navigator.clipboard.writeText(revealedWords.value.join(' '))
    wordsCopied.value = true
    setTimeout(() => { wordsCopied.value = false }, 2000)
  } catch { /* clipboard unavailable */ }
}

async function confirmBackedUp() {
  if (acking.value) return
  acking.value = true
  try {
    await rpcClient.call({ method: 'lnd.seed-backup-ack' })
    acknowledged.value = true
    closeReveal()
  } catch { /* keep the modal open; user can retry */ } finally {
    acking.value = false
  }
}
</script>

<template>
  <div v-if="statusLoaded && available" class="glass-card px-6 py-6 mb-6" :class="!acknowledged ? 'border border-orange-400/40' : ''">
    <div v-if="!acknowledged" class="flex items-center gap-2 mb-3 text-orange-300 text-sm font-medium" role="alert">
      <svg class="w-5 h-5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M10.29 3.86l-8.4 14.55A1.5 1.5 0 003.19 21h17.62a1.5 1.5 0 001.3-2.59l-8.4-14.55a1.5 1.5 0 00-2.62 0z" />
      </svg>
      Your Lightning seed hasn't been backed up yet
    </div>
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h2 class="text-xl font-semibold text-white/96 mb-1">Lightning wallet seed</h2>
        <p class="text-sm text-white/60">
          View the 24-word recovery seed for this node's Lightning wallet. You'll need to
          confirm your password (and 2FA code, if enabled). Write it down and store it
          offline — anyone with these words controls your Lightning funds.
        </p>
      </div>
      <button
        type="button"
        class="shrink-0 glass-button rounded-lg px-4 py-2 text-sm font-medium"
        :class="!acknowledged ? 'bg-orange-500/20 border-orange-400/30' : ''"
        @click="openReveal"
      >{{ acknowledged ? 'Reveal' : 'Back up now' }}</button>
    </div>
  </div>

  <Teleport to="body">
    <div v-if="showRevealModal" class="fixed inset-0 z-[3000] flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="closeReveal">
      <div class="glass-card p-6 w-full max-w-md" role="dialog" aria-modal="true" aria-labelledby="reveal-lnd-seed-title">
        <h3 id="reveal-lnd-seed-title" class="text-lg font-semibold text-white mb-1">Reveal Lightning seed</h3>

        <template v-if="revealedWords.length === 0">
          <p class="text-sm text-white/60 mb-4">Confirm your credentials to display the 24-word Lightning seed.</p>
          <form @submit.prevent="submitReveal" class="space-y-3">
            <div>
              <label class="block text-xs text-white/60 mb-1">Password</label>
              <input v-model="revealPassword" type="password" autocomplete="current-password" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-white/30" placeholder="Your login password" />
            </div>
            <div>
              <label class="block text-xs text-white/60 mb-1">2FA code <span class="text-white/30">(if enabled)</span></label>
              <input v-model="revealCode" inputmode="numeric" autocomplete="one-time-code" class="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm font-mono tracking-widest focus:outline-none focus:border-white/30" placeholder="123456" />
            </div>
            <p v-if="revealError" class="text-xs text-red-300 bg-red-500/10 border border-red-400/20 rounded-lg px-3 py-2">{{ revealError }}</p>
            <div class="flex gap-2 pt-1">
              <button type="button" @click="closeReveal" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">Cancel</button>
              <button type="submit" :disabled="revealing || !revealPassword" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50">
                {{ revealing ? 'Verifying…' : 'Reveal' }}
              </button>
            </div>
          </form>
        </template>

        <template v-else>
          <SeedRevealPanel :words="revealedWords" aezeed />

          <div class="flex gap-2 pt-4">
            <button type="button" @click="copyRevealedWords" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium">{{ wordsCopied ? 'Copied!' : 'Copy' }}</button>
            <button type="button" :disabled="acking" @click="confirmBackedUp" class="flex-1 glass-button rounded-lg px-4 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30 disabled:opacity-50">
              {{ acking ? 'Saving…' : "I've backed it up" }}
            </button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>
