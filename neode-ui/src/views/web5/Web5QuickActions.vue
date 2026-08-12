<template>
  <!-- Quick Actions Container -->
  <div class="glass-card p-6 mb-6">
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-6 gap-4 stagger-grid">
      <!-- Networking Profits -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 0">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <span class="text-2xl text-orange-500 font-bold">₿</span>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">{{ t('web5.networkingProfits') }}</p>
            <p class="text-xs text-orange-500 font-medium">{{ networkingProfitsDisplay }}</p>
          </div>
        </div>
        <div v-if="profitsBreakdown" class="text-xs text-white/40 space-y-0.5">
          <p v-if="profitsBreakdown.content_sales_sats > 0">Content: {{ profitsBreakdown.content_sales_sats.toLocaleString() }} sats</p>
          <p v-if="profitsBreakdown.routing_fees_sats > 0">Routing: {{ profitsBreakdown.routing_fees_sats.toLocaleString() }} sats</p>
        </div>
        <button
          @click="router.push('/dashboard/web5/networking-profits')"
          class="w-full mt-auto px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ t('common.settings') }}
        </button>
      </div>

      <!-- Mobile-only toggle: DID Status through Connected Nodes are often not
           needed at a glance, so collapse them by default to save space. -->
      <button
        type="button"
        class="md:hidden flex items-center justify-center gap-1.5 py-2 text-xs font-medium text-white/50 hover:text-white/80 transition-colors"
        @click="mobileCollapsed = !mobileCollapsed"
      >
        {{ mobileCollapsed ? 'Show more' : 'Show less' }}
        <svg class="w-3.5 h-3.5 transition-transform" :class="{ 'rotate-180': !mobileCollapsed }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      <div :class="mobileCollapsed ? 'hidden md:contents' : 'contents'">
      <!-- DID Status -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 1">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="didStatus === 'active' ? 'bg-green-400' : 'bg-yellow-400'"></div>
            <div v-if="didStatus === 'active'" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white">{{ t('web5.didStatus') }}</p>
            <p v-if="userDid" class="text-xs text-white/60 font-mono truncate" :title="userDid">{{ userDid }}</p>
            <p v-else class="text-xs text-white/60 capitalize">{{ didStatus }}</p>
          </div>
        </div>
        <div v-if="userDid" class="flex gap-2 mt-auto">
        <button
          @click="$emit('copyDid')"
          class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ didCopied ? t('common.copiedBang') : t('web5.copyDid') }}
        </button>
          <button
            @click="$emit('showDidDocument')"
            class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
          >
            {{ t('web5.viewDidDocument') }}
          </button>
        </div>
        <button
          v-else
          @click="$emit('createDid')"
          :disabled="creatingDid"
          class="w-full mt-auto px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
        >
          {{ creatingDid ? t('web5.creatingDid') : t('web5.createDid') }}
        </button>
      </div>

      <!-- did:dht Status -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 1.5">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="dhtDid ? 'bg-blue-400' : 'bg-gray-500'"></div>
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white">DHT Identity</p>
            <p v-if="dhtDid" class="text-xs text-white/60 font-mono truncate" :title="dhtDid">{{ dhtDid }}</p>
            <p v-else class="text-xs text-white/60">Not published</p>
          </div>
        </div>
        <div v-if="dhtDid" class="flex gap-2 mt-auto">
        <button
          @click="$emit('copyDhtDid')"
          class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ dhtDidCopied ? 'Copied!' : 'Copy' }}
        </button>
        <button
          @click="$emit('refreshDhtDid')"
          :disabled="publishingDht"
          class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
        >
          {{ publishingDht ? 'Refreshing...' : 'Refresh DHT' }}
        </button>
        </div>
        <button
          v-else-if="userDid"
          @click="$emit('publishDhtDid')"
          :disabled="publishingDht"
          class="w-full mt-auto px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
        >
          {{ publishingDht ? 'Publishing...' : 'Publish to DHT' }}
        </button>
      </div>

      <!-- Wallet Connection -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 2">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="walletConnected ? 'bg-green-400' : 'bg-red-400'"></div>
            <div v-if="walletConnected" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">{{ t('web5.wallet') }}</p>
            <p class="text-xs text-white/60">{{ walletConnected ? t('common.connected') : t('common.disconnected') }}</p>
          </div>
        </div>
        <button
          @click="$emit('connectWallet')"
          class="w-full mt-auto px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
          :disabled="connectingWallet"
        >
          {{ connectingWallet ? t('common.connecting') : walletConnected ? t('common.disconnect') : t('common.connect') }}
        </button>
      </div>

      <!-- Nostr Relay Status -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 3">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="(nostrRelayStats?.connected_count ?? 0) > 0 ? 'bg-green-400' : 'bg-red-400'"></div>
            <div v-if="(nostrRelayStats?.connected_count ?? 0) > 0" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">{{ t('web5.nostrRelays') }}</p>
            <p class="text-xs text-white/60">{{ t('web5.relaysConnected', { count: nostrRelayStats?.connected_count ?? 0 }) }}</p>
          </div>
        </div>
        <button
          @click="$emit('manageRelays')"
          class="w-full mt-auto px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ t('common.manage') }}
        </button>
      </div>

      <!-- Connected Nodes -->
      <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0" style="--stagger-index: 4">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="connectedNodesCount > 0 ? 'bg-green-400' : 'bg-amber-400'"></div>
            <div v-if="connectedNodesCount > 0" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-pulse opacity-75"></div>
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white">{{ t('web5.connectedNodes') }}</p>
            <p class="text-xs text-white/60">{{ t('web5.peersKnown', { count: connectedNodesCount }) }}</p>
          </div>
        </div>
        <div class="flex gap-2 mt-auto">
          <button
            @click="router.push('/dashboard/server/federation')"
            class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
          >
            Nodes
          </button>
          <button
            @click="router.push('/dashboard/mesh')"
            class="flex-1 px-3 py-1.5 glass-button glass-button-sm rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
          >
            Message
          </button>
        </div>
      </div>
      </div>
    </div>
  </div>

  <!-- Hardware Wallet Detected Banner -->
  <div v-if="detectedHwWallets.length > 0" class="mb-6 alert-warning flex items-center gap-3">
    <div class="w-8 h-8 rounded-lg bg-orange-500/20 flex items-center justify-center flex-shrink-0">
      <svg class="w-5 h-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
      </svg>
    </div>
    <div class="flex-1 min-w-0">
      <p class="text-sm font-medium text-orange-400">{{ t('web5.hardwareWalletDetected') }}</p>
      <p class="text-xs text-white/60">
        {{ detectedHwWallets.map(d => `${d.type}${d.product ? ' (' + d.product + ')' : ''}`).join(', ') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { ProfitsData, NostrRelayStatsData, HwWalletDevice } from './types'

const router = useRouter()
const { t } = useI18n()

// DID Status through Connected Nodes are often not needed at a glance on
// mobile, so start collapsed there to save space. Desktop always shows them.
const mobileCollapsed = ref(true)

defineProps<{
  showStagger: boolean
  profitsBreakdown: ProfitsData | null
  networkingProfitsDisplay: string
  userDid: string | null
  didStatus: 'active' | 'inactive' | 'pending'
  didCopied: boolean
  creatingDid: boolean
  dhtDid: string | null
  dhtDidCopied: boolean
  publishingDht: boolean
  walletConnected: boolean
  connectingWallet: boolean
  nostrRelayStats: NostrRelayStatsData | null
  connectedNodesCount: number
  detectedHwWallets: HwWalletDevice[]
}>()

defineEmits<{
  copyDid: []
  showDidDocument: []
  createDid: []
  copyDhtDid: []
  refreshDhtDid: []
  publishDhtDid: []
  connectWallet: []
  manageRelays: []
}>()
</script>
