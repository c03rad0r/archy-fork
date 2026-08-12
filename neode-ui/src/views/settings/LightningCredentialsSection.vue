<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { rpcClient, type LndMacaroonStatus, type LndRotationProgress } from '@/api/rpc-client'

// A Lightning macaroon is a bearer token: whoever holds one can spend from this
// node's wallet. Rotating them is the only way to take that ability back from
// anything that has seen one — a lost phone, a shared screenshot, an app that
// ran a version with a published vulnerability. Until now that meant SSHing in
// and running a script, which in practice meant it did not happen.
//
// The screen is deliberately fact-first. Before anyone clicks the button they
// can see when the credentials were issued, which node they belong to, how many
// channels must survive, and whether anything on this node is already out of
// step — because "will this close my channels?" is the question that stops
// people rotating, and the honest answer is on the page.

const status = ref<LndMacaroonStatus | null>(null)
const loading = ref(true)
const loadError = ref('')

const showConfirm = ref(false)
const password = ref('')
const submitting = ref(false)
const confirmError = ref('')

let poll: ReturnType<typeof setInterval> | null = null

/// Epoch ms until which we keep polling even though the node has not reported a
/// running rotation yet.
///
/// Without this the screen can freeze on the one action that most needs to show
/// progress: `rotate()` starts polling, the `load()` right behind it observes a
/// status snapshot that does not yet carry `running: true`, and `syncPolling`
/// cancels the interval. The operator has just invalidated every credential
/// their wallet holds and the page tells them nothing is happening.
///
/// Bounded rather than a plain flag, so a request the node accepted but never
/// acted on stops polling instead of hammering it forever.
const awaitUntil = ref(0)
const AWAIT_START_MS = 120_000

const rotation = computed<LndRotationProgress | null>(() => status.value?.rotation ?? null)
const isRunning = computed(() => rotation.value?.running === true)

/// Ticks while a rotation is being awaited, so `rotationInFlight` re-evaluates
/// as the await window expires instead of holding a stale value until the next
/// poll happens to touch a reactive dependency.
const now = ref(Date.now())

/// Is a rotation happening, INCLUDING the gap between asking for one and the
/// node reporting it?
///
/// Rotation restarts LND, so `status.installed` goes false for a moment
/// mid-rotation. Read literally that says "Lightning is not set up on this
/// node" — which the screen then told the operator, seconds after they
/// rotated, on a node with a working Lightning wallet. The container being
/// briefly absent is what rotating LOOKS like, not evidence it was never
/// there.
const rotationInFlight = computed(() => isRunning.value || now.value < awaitUntil.value)

/** A finished rotation, successful or not. `ok` is null while running. */
const finished = computed(
  () => rotation.value !== null && !rotation.value.running && rotation.value.ok !== null,
)

/** BTCPay embeds a copy of the macaroon inline and cannot self-heal, so it is
 * the one dependency that can silently fall out of step. `false` is the state
 * worth shouting about; `null` just means BTCPay has no internal node. */
const btcpayStale = computed(() => status.value?.btcpay_credential_current === false)

async function load() {
  try {
    status.value = await rpcClient.lndMacaroonStatus()
    loadError.value = ''
    syncPolling()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

/// Poll only while there is something to watch, so an idle Settings tab isn't
/// waking the node every few seconds.
function syncPolling() {
  const running = status.value?.rotation.running === true
  if (running) awaitUntil.value = 0
  now.value = Date.now()
  if (running || Date.now() < awaitUntil.value) startPolling()
  else stopPolling()
}

function startPolling() {
  if (poll) return
  poll = setInterval(load, 4000)
}

function stopPolling() {
  if (poll) {
    clearInterval(poll)
    poll = null
  }
}

function openConfirm() {
  password.value = ''
  confirmError.value = ''
  showConfirm.value = true
}

function closeConfirm() {
  showConfirm.value = false
  password.value = ''
  confirmError.value = ''
}

async function rotate() {
  submitting.value = true
  confirmError.value = ''
  try {
    await rpcClient.lndRotateMacaroons(password.value)
    closeConfirm()
    awaitUntil.value = Date.now() + AWAIT_START_MS
    now.value = Date.now()
    startPolling()
    await load()
  } catch (e) {
    confirmError.value = e instanceof Error ? e.message : String(e)
  } finally {
    submitting.value = false
    password.value = ''
  }
}

function stepIcon(state: string): string {
  switch (state) {
    case 'done':
      return '✓'
    case 'failed':
      return '✕'
    case 'skipped':
      return '–'
    case 'running':
      return '…'
    default:
      return '·'
  }
}

function stepClass(state: string): string {
  switch (state) {
    case 'done':
      return 'text-emerald-400'
    case 'failed':
      return 'text-red-400'
    case 'skipped':
      return 'text-white/40'
    case 'running':
      return 'text-orange-300'
    default:
      return 'text-white/30'
  }
}

/** First 16 characters is plenty to compare two digests by eye, and keeps the
 * line readable on a phone. */
function shortHash(h: string | null): string {
  return h ? `${h.slice(0, 16)}…` : '—'
}

onMounted(load)
onUnmounted(stopPolling)
</script>

<template>
  <div class="glass-card px-6 py-6 mb-6">
    <!-- glass-card, like every other Settings section (AccountSection,
         AIDataAccessSection, NodeCertificateSection, BackupSection …). This
         rendered as bare text on the Settings page twice, because a new
         section carries its own wrapper and nothing about adding it to
         SystemSection.vue's list reminds you. Heading is h2/text-xl to match
         those siblings. Kept INSIDE the root: a leading comment makes the
         component a fragment, which drops the root class and breaks attribute
         inheritance. -->
    <h2 class="text-xl font-semibold text-white/96 mb-1">Lightning credentials</h2>
    <p class="text-sm text-white/60 mb-4">
      Wallet apps like Zeus connect to this node using a Lightning credential — a
      token that lets them spend. Rotating replaces every one of them, so anything
      that copied an old token can no longer use it. Your coins and channels are
      not touched: the node keeps its identity and no channel is closed.
    </p>

    <div v-if="loading" class="text-sm text-white/50">Checking…</div>

    <div
      v-else-if="loadError"
      class="p-3 bg-white/5 border border-white/10 rounded-lg text-sm text-white/70"
    >
      Could not read the Lightning credential state: {{ loadError }}
    </div>

    <!-- `&& !rotationInFlight`: rotating restarts LND, so `installed` reads
         false for a moment mid-rotation. Without the guard this told the
         operator "Lightning is not set up on this node yet" seconds after they
         rotated on a node with a working wallet — and it replaced the progress
         they were watching. A container briefly absent is what rotating looks
         like, not proof Lightning was never installed. -->
    <div
      v-else-if="!status?.installed && !rotationInFlight"
      class="p-3 bg-white/5 border border-white/10 rounded-lg text-sm text-white/70"
    >
      Lightning is not set up on this node yet, so there are no credentials to
      rotate. Install the Lightning app first.
    </div>

    <!-- Mid-rotation with no status to render yet: say what is happening
         rather than falling through to the details block with empty fields. -->
    <div
      v-else-if="!status?.installed"
      class="p-3 bg-white/5 border border-white/10 rounded-lg text-sm text-white/70"
    >
      Rotating credentials — Lightning is restarting. This takes a moment.
    </div>

    <div v-else class="space-y-4">
      <!-- What exists right now -->
      <dl class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
        <div>
          <dt class="text-white/50 text-xs">Issued</dt>
          <dd class="text-white/80">{{ status.issued_at || 'unknown' }}</dd>
        </div>
        <div>
          <dt class="text-white/50 text-xs">Credential fingerprint</dt>
          <dd class="text-white/80 font-mono text-xs break-all">
            {{ shortHash(status.admin_macaroon_sha256) }}
          </dd>
        </div>
        <div>
          <dt class="text-white/50 text-xs">Channels that must survive</dt>
          <dd class="text-white/80">
            <template v-if="status.channels_open !== null">
              {{ status.channels_open }} open<span v-if="status.channels_pending">
                , {{ status.channels_pending }} pending</span
              >
            </template>
            <span v-else class="text-white/50">not readable — Lightning is not answering</span>
          </dd>
        </div>
        <div>
          <dt class="text-white/50 text-xs">Node identity</dt>
          <dd class="text-white/80 font-mono text-xs break-all">
            {{ status.identity_pubkey ? `${status.identity_pubkey.slice(0, 16)}…` : '—' }}
          </dd>
        </div>
      </dl>

      <!-- Lightning has to be answering for a rotation to be verifiable at all,
           so this is a blocker rather than a footnote. -->
      <div
        v-if="status.lnd_error && !isRunning"
        class="p-3 bg-orange-500/10 border border-orange-500/30 rounded-lg text-sm text-orange-100/90"
      >
        <p class="font-medium mb-1">Lightning is not answering right now.</p>
        <p class="text-orange-100/70">
          Rotation is blocked until it is: without a reading from before the
          change there is no way to prove afterwards that your channels came
          back. Wait for Lightning to finish starting and reload this page.
        </p>
        <p class="text-xs text-orange-100/50 mt-2 font-mono break-all">{{ status.lnd_error }}</p>
      </div>

      <!-- The failure this whole feature exists to prevent. -->
      <div
        v-if="btcpayStale"
        class="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-100/90"
      >
        <p class="font-medium mb-1">BTCPay Server is holding an old Lightning credential.</p>
        <p class="text-red-100/70">
          BTCPay keeps its own copy of the credential, and the copy it has no
          longer works — so its Lightning payments will fail even though both
          apps look healthy. Rotating now repairs this as part of the run.
        </p>
      </div>

      <!-- Progress. Shown while running and kept afterwards, because the
           verdict ("same node, same channels") is the reassurance the operator
           came here for. -->
      <div v-if="rotation && (isRunning || finished)" class="p-3 bg-white/5 border border-white/10 rounded-lg">
        <p class="text-sm font-medium text-white/80 mb-2">
          <span v-if="isRunning">Rotating…</span>
          <span v-else-if="rotation.ok" class="text-emerald-400">Rotation complete</span>
          <span v-else class="text-red-400">Rotation failed</span>
        </p>
        <ul class="space-y-1.5">
          <li v-for="step in rotation.steps" :key="step.key" class="text-sm">
            <span class="font-mono mr-2" :class="stepClass(step.state)">{{
              stepIcon(step.state)
            }}</span>
            <span :class="step.state === 'pending' ? 'text-white/40' : 'text-white/80'">{{
              step.label
            }}</span>
            <p v-if="step.detail" class="ml-6 text-xs text-white/50">{{ step.detail }}</p>
          </li>
        </ul>

        <p v-if="rotation.error" class="mt-3 text-xs text-red-300/90 break-words">
          {{ rotation.error }}
        </p>

        <div v-if="finished && rotation.ok" class="mt-3 space-y-2 text-xs text-white/60">
          <p class="text-white/80">
            Re-pair anything that connects to this node — Zeus most importantly.
            Open the Lightning app and scan its pairing QR again; it serves the
            new credential.
          </p>
          <p v-if="rotation.backup_path">
            The old credentials were backed up on the node so a mistake is
            recoverable. That backup is still sensitive. Once every app is
            re-paired, delete it:
            <code class="block mt-1 px-2 py-1 bg-black/30 rounded font-mono break-all"
              >sudo rm -rf {{ rotation.backup_path }}</code
            >
          </p>
        </div>
      </div>

      <button
        :disabled="isRunning || !!status.lnd_error"
        class="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg glass-button glass-button-warning font-medium disabled:opacity-50 disabled:cursor-not-allowed"
        @click="openConfirm"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
          />
        </svg>
        <span>{{ isRunning ? 'Rotating…' : 'Rotate Lightning credentials' }}</span>
      </button>
    </div>
  </div>

  <!-- Confirmation. Teleported to body: a glass-panel ancestor creates a
       transform context that would trap a position:fixed backdrop. -->
  <Teleport to="body">
    <div
      v-if="showConfirm"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md"
      @click.self="closeConfirm"
      @keydown.escape="closeConfirm"
    >
      <div
        class="glass-card p-6 max-w-md w-full"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rotate-macaroon-title"
      >
        <h3 id="rotate-macaroon-title" class="text-lg font-semibold text-white mb-2">
          Rotate Lightning credentials
        </h3>
        <div class="text-sm text-white/70 space-y-2 mb-4">
          <p>
            <strong class="text-white/90">What changes:</strong> every app paired
            with this node stops working until you re-pair it. Zeus and any other
            remote wallet will need to scan a fresh pairing code.
          </p>
          <p>
            <strong class="text-white/90">What does not:</strong> your coins and
            your channels. The node keeps its identity, nothing is closed, and
            this run refuses to report success unless it has confirmed both.
          </p>
          <p>
            Lightning restarts as part of this, which takes a few minutes on a
            busy node. Payments cannot be sent or received during that window.
          </p>
        </div>
        <form class="space-y-4" @submit.prevent="rotate">
          <label class="block">
            <span class="text-xs text-white/60">Confirm with your node password</span>
            <input
              v-model="password"
              type="password"
              required
              autocomplete="current-password"
              class="mt-1 w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              placeholder="Node password"
            />
          </label>
          <p v-if="confirmError" class="text-sm text-red-400 break-words">{{ confirmError }}</p>
          <div class="flex gap-3">
            <button
              type="button"
              class="flex-1 px-4 py-2 rounded-lg glass-button font-medium"
              @click="closeConfirm"
            >
              Cancel
            </button>
            <button
              type="submit"
              :disabled="submitting || !password"
              class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {{ submitting ? 'Starting…' : 'Rotate' }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>
