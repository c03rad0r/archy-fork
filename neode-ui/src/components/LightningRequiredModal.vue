<template>
  <!-- z-3600: this is raised from INSIDE another modal (Receive, the Web5
       send/receive sheet, the app launcher's paywall), so it must sit above
       the standard modal layer (3000) but below the app overlay (4000) —
       same reasoning as ExternalExplorerModal. -->
  <BaseModal
    :show="lightning.show.value"
    :title="modalTitle"
    max-width="max-w-md"
    z-index="z-[3600]"
    @close="onClose"
  >
    <p v-if="lightning.status.value === 'no-funds' && lightning.fundingReason.value === 'pending'" class="text-sm text-white/70 leading-relaxed">
      Your new channel is <span class="text-white/90">waiting for its on-chain confirmations</span> —
      that's why the network doesn't see it yet. It unlocks automatically once
      confirmed (usually within about half an hour); nothing is needed from
      you. This screen will work as soon as it lands.
    </p>
    <p v-else-if="lightning.status.value === 'no-funds' && lightning.fundingReason.value === 'far-side'" class="text-sm text-white/70 leading-relaxed">
      <template v-if="lightning.fundingDirection.value === 'receive'">
        You have channels, but <span class="text-white/90">all the balance is on your side</span> —
        you can send, but there's nothing to be paid into right now. Receive a
        payment by spending first, or open another channel to bring inbound
        liquidity in.
      </template>
      <template v-else>
        You have channels, but <span class="text-white/90">all the balance is on the far side</span> —
        you can receive, but there's nothing to send right now. Someone has to
        pay you first (or rebalance the channel), and sending unlocks on its own.
      </template>
    </p>
    <p v-else-if="lightning.status.value === 'no-funds' && lightning.fundingReason.value === 'failed-payment'" class="text-sm text-white/70 leading-relaxed">
      LND couldn't route this payment — most often there's
      <span class="text-white/90">not enough outbound for this amount</span>, or no
      route to the recipient at the fees offered. Smaller amounts sometimes
      get through; check the channels screen to see what's actually spendable.
    </p>
    <p v-else-if="lightning.status.value === 'no-funds'" class="text-sm text-white/70 leading-relaxed">
      Your Lightning node is running, but it has no payment channel yet.
      <template v-if="lightning.fundingDirection.value === 'receive'">
        Receiving needs <span class="text-white/90">inbound liquidity</span> — a
        channel with funds on the far side — otherwise any invoice you create
        is unpayable.
      </template>
      <template v-else>
        Sending needs <span class="text-white/90">outbound liquidity</span> — a
        funded channel to route through.
      </template>
      Open one with <span class="text-white/90">Zeus Olympus</span> from the
      channels screen — it's prefilled there, and needs 150,000–1,500,000
      on-chain sats.
    </p>
    <p v-else-if="lightning.status.value === 'stopped'" class="text-sm text-white/70 leading-relaxed">
      Lightning payments need a Lightning node that's actually running. Yours is
      installed but isn't running right now — start it from My Apps and try
      again.
    </p>
    <p v-else class="text-sm text-white/70 leading-relaxed">
      Lightning payments need a Lightning node running on this Archipelago
      node. You don't have one installed yet — pick an implementation below and
      it'll be installed for you.
    </p>

    <div v-if="lightning.status.value === 'absent'" class="mt-4 space-y-2">
      <div
        v-for="node in nodes"
        :key="node.id"
        class="rounded-xl border border-white/10 bg-white/[0.04] p-3"
      >
        <div class="ln-node-row flex items-start gap-3">
          <div class="ln-node-icon shrink-0">
            <img
              v-if="!failedIcons.has(node.id)"
              :src="node.icon"
              :alt="node.name"
              class="w-full h-full object-contain"
              @error="failedIcons.add(node.id)"
            />
            <!-- No packaged icon yet (Core Lightning): a neutral bolt keeps the
                 row aligned instead of showing a broken-image box. -->
            <svg v-else class="w-6 h-6 text-white/40" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-white">{{ node.name }}</span>
              <span
                v-if="!node.available"
                class="text-[10px] uppercase tracking-wide px-2 py-0.5 rounded-full bg-white/10 text-white/50"
              >Coming soon</span>
            </div>
            <p class="text-xs text-white/50 mt-0.5 leading-relaxed">{{ node.blurb }}</p>
          </div>
          <button
            v-if="node.available"
            :disabled="installing !== null"
            class="ln-node-action glass-button glass-button-warning rounded-lg text-xs font-medium disabled:opacity-50"
            @click="install(node.id)"
          >
            {{ installing === node.id ? 'Installing…' : 'Install' }}
          </button>
          <button
            v-else
            disabled
            class="ln-node-action glass-button rounded-lg text-xs opacity-40 cursor-not-allowed"
          >Install</button>
        </div>
      </div>
    </div>

    <p v-if="error" class="mt-3 alert-error text-sm">{{ error }}</p>

    <p v-if="installing" class="mt-3 text-xs text-white/50 leading-relaxed">
      This takes a few minutes — the image has to be pulled and the node
      started. You can close this and carry on; the install keeps running.
    </p>

    <div class="flex flex-wrap gap-2 mt-6">
      <button class="flex-1 glass-button px-4 py-2 rounded-lg text-sm" @click="onClose">
        {{ installing ? 'Close' : 'Not now' }}
      </button>
      <button
        v-if="lightning.status.value === 'stopped'"
        class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium"
        @click="openApps"
      >Open My Apps</button>
      <template v-else-if="lightning.status.value === 'no-funds'">
        <!-- A confirming channel needs no action at all — offering "open a
             channel" here would send the user to fix a problem they don't
             have (and possibly open a second one). -->
        <template v-if="lightning.fundingReason.value === 'pending'">
          <button
            class="flex-1 glass-button px-4 py-2 rounded-lg text-sm"
            @click="onClose"
          >Got it — I'll wait</button>
        </template>
        <template v-else>
          <button
            class="flex-1 glass-button px-4 py-2 rounded-lg text-sm"
            @click="openSetupGuide"
          >Setup Guide</button>
          <button
            v-if="lightning.fundingReason.value !== 'failed-payment'"
            class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium"
            @click="openLightningSetup"
          >Open a channel</button>
          <button
            v-else
            class="flex-1 glass-button px-4 py-2 rounded-lg text-sm"
            @click="onClose"
          >Close</button>
        </template>
      </template>
    </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import BaseModal from '@/components/BaseModal.vue'
import { useRouter } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { useLightningRequired } from '@/composables/useLightningRequired'

interface NodeChoice {
  id: string
  name: string
  blurb: string
  /** Falls back to a neutral bolt when the asset is missing. */
  icon: string
  /** False renders the row as "Coming soon" with a dead Install button. */
  available: boolean
}

// Core Lightning is listed deliberately while unavailable: the choice is the
// point of this modal, and showing it greyed tells the user the platform is
// not LND-only. When its app id lands in the catalog, flip `available` here
// and add the id to LIGHTNING_NODE_APP_IDS — nothing else changes.
const nodes: NodeChoice[] = [
  {
    id: 'lnd',
    name: 'LND',
    blurb: 'Lightning Network Daemon. The implementation Archipelago ships today — wallet, channels and payments are wired to it.',
    icon: '/assets/img/app-icons/lnd.png',
    available: true,
  },
  {
    id: 'core-lightning',
    name: 'Core Lightning',
    blurb: 'Blockstream\'s implementation. Not packaged yet — it will appear here as a choice once it ships.',
    icon: '/assets/img/app-icons/core-lightning.svg',
    available: false,
  },
]

const router = useRouter()

const modalTitle = computed(() => {
  if (lightningStatusIs('no-funds')) {
    if (lightning.fundingReason.value === 'pending') return 'Channel confirming…'
    if (lightning.fundingReason.value === 'far-side') return 'Balance is on the far side'
    if (lightning.fundingReason.value === 'failed-payment') return 'Payment couldn\u2019t route'
    return 'You need a Lightning channel'
  }
  if (lightningStatusIs('stopped')) return 'Lightning node not running'
  return 'Lightning node required'
})
const appStore = useAppStore()
const lightning = useLightningRequired()

/** App id currently installing, or null. */
/** Icons that 404'd — swapped for the inline bolt. */
const failedIcons = ref(new Set<string>())

const installing = ref<string | null>(null)
const error = ref('')

async function install(id: string) {
  installing.value = id
  error.value = ''
  try {
    await appStore.installPackage(id, '', 'latest')
    // The gate reads install state from the package list, so once the install
    // lands the modal simply stops being raised. Close on success rather than
    // holding the user here watching a spinner.
    lightning.close()
  } catch (err) {
    error.value = `Install failed: ${err instanceof Error ? err.message : 'Unknown error'}`
  } finally {
    installing.value = null
  }
}

function lightningStatusIs(s: string) {
  return lightning.status.value === s
}

/** The Lightning goal already owns funding + channel-opening, so reuse it
 *  rather than duplicating that flow inside this modal. */
function openLightningSetup() {
  lightning.close()
  // The channels screen already has the prefilled Zeus/Olympus open-channel
  // flow, so send the user straight to the thing that solves it rather than
  // to the wizard that would only point here anyway.
  router.push('/dashboard/apps/lnd/channels')
}

/** The guided walkthrough, for someone who wants the whole path explained
 *  rather than to be dropped straight into the open-channel form. */
function openSetupGuide() {
  lightning.close()
  router.push('/dashboard/goals/run-lightning-node')
}

function openApps() {
  lightning.close()
  router.push('/dashboard/apps')
}

function onClose() {
  error.value = ''
  lightning.close()
}
</script>

<style scoped>
.ln-node-icon {
  width: 2.5rem;
  height: 2.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.75rem;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  padding: 0.35rem;
}
.ln-node-action {
  flex-shrink: 0;
  padding: 0.375rem 0.875rem;
}
/* Narrow phones: the icon + name + blurb keep the top row, and the action
   drops to its own full-width line rather than squeezing the blurb into a
   two-word column. */
@media (max-width: 26rem) {
  .ln-node-row {
    flex-wrap: wrap;
  }
  .ln-node-action {
    width: 100%;
    margin-top: 0.625rem;
    padding-top: 0.5rem;
    padding-bottom: 0.5rem;
  }
}
</style>
