<template>
  <!-- Federation Summary -->
  <div class="glass-card p-6">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
        </div>
        <div>
          <h2 class="text-lg font-semibold text-white">Federation</h2>
          <p class="text-xs text-white/60">Federated nodes &amp; peers</p>
        </div>
      </div>
      <div class="web5-card-actions-top items-center gap-2">
        <RouterLink to="/dashboard/server/federation" class="glass-button glass-button-sm px-3 rounded-lg text-sm font-medium inline-flex items-center gap-2 no-underline">
          Connect with Nodes
        </RouterLink>
        <RouterLink to="/dashboard/fleet" class="glass-button glass-button-sm px-3 rounded-lg text-sm font-medium inline-flex items-center gap-2 no-underline">
          Fleet
        </RouterLink>
      </div>
    </div>

    <div class="space-y-3">
      <div v-if="loadingFederation && hasLoadedFederation" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing federation...
      </div>
      <div v-else-if="federationError && hasLoadedFederation" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ federationError }}
      </div>
      <div v-else-if="loadingFederation" class="p-2 text-center text-white/45 text-xs">
        Loading federation...
      </div>

      <!-- Node Count -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <div class="w-2 h-2 rounded-full" :class="nodeCount > 0 ? 'bg-green-400' : 'bg-white/30'" />
          <span class="text-sm text-white/80">Known Nodes</span>
        </div>
        <span class="text-sm font-medium text-white">{{ nodeCount }}</span>
      </div>

      <!-- Online -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <div class="w-2 h-2 rounded-full" :class="onlineCount > 0 ? 'bg-green-400' : 'bg-white/30'" />
          <span class="text-sm text-white/80">Online</span>
        </div>
        <span class="text-sm font-medium" :class="onlineCount > 0 ? 'text-green-400' : 'text-white/40'">{{ onlineCount }}</span>
      </div>

      <!-- Pending Requests -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <div class="w-2 h-2 rounded-full" :class="pendingCount > 0 ? 'bg-orange-400' : 'bg-white/30'" />
          <span class="text-sm text-white/80">Pending Requests</span>
        </div>
        <span class="text-sm font-medium" :class="pendingCount > 0 ? 'text-orange-400' : 'text-white/40'">{{ pendingCount }}</span>
      </div>

      <!-- Self DID -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <span class="text-sm text-white/80">Node DID</span>
        <span class="text-sm font-medium text-white/40 truncate max-w-[140px]">{{ selfDid || 'Not set' }}</span>
      </div>
    </div>

    <div class="web5-card-actions-bottom-grid mt-6 grid-cols-2 gap-3">
      <RouterLink to="/dashboard/server/federation" class="mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors no-underline">
        Connect with Nodes
      </RouterLink>
      <RouterLink to="/dashboard/fleet" class="mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors no-underline">
        Fleet
      </RouterLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { RouterLink } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'

const nodeCount = ref(0)
const onlineCount = ref(0)
const pendingCount = ref(0)
const selfDid = ref('')
const loadingFederation = ref(false)
const hasLoadedFederation = ref(false)
const federationError = ref('')

async function loadFederationSummary() {
  loadingFederation.value = true
  federationError.value = ''
  try {
    const res = await rpcClient.call<{
      nodes: Array<{ last_seen?: string }>
    }>({ method: 'federation.list-nodes', timeout: 5000 })
    const nodes = res.nodes || []
    nodeCount.value = nodes.length
    // list-nodes has no live status field — count a node seen in the
    // last 10 minutes as online
    const cutoff = Date.now() - 10 * 60 * 1000
    onlineCount.value = nodes.filter(n => n.last_seen && new Date(n.last_seen).getTime() > cutoff).length
    try {
      const pend = await rpcClient.call<{ requests: Array<unknown> }>({ method: 'federation.list-pending-requests', timeout: 5000 })
      pendingCount.value = (pend.requests || []).length
    } catch { pendingCount.value = 0 }
    hasLoadedFederation.value = true
  } catch (e: unknown) {
    federationError.value = e instanceof Error ? e.message : 'Federation unavailable'
  }

  try {
    const res = await rpcClient.getNodeDid()
    selfDid.value = res.did || ''
  } catch (e: unknown) {
    if (!federationError.value) federationError.value = e instanceof Error ? e.message : 'Node identity unavailable'
  } finally {
    loadingFederation.value = false
  }
}

onMounted(loadFederationSummary)

defineExpose({ loadFederationSummary })
</script>
