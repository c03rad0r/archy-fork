<template>
  <div>
    <!-- Sync Results -->
    <div v-if="syncResults.length > 0" class="glass-card p-6 mb-6">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-white">Sync Results</h2>
        <button @click="$emit('clear-sync-results')" class="text-white/40 hover:text-white/70 transition-colors text-sm">Dismiss</button>
      </div>
      <div class="space-y-2">
        <div v-for="r in syncResults" :key="r.did" class="flex items-center gap-3 p-3 bg-white/5 rounded-lg">
          <div class="w-2 h-2 rounded-full shrink-0" :class="r.status === 'ok' ? 'bg-green-400' : 'bg-red-400'"></div>
          <span class="text-sm text-white/80 truncate" :title="r.did">{{ nodeNameFromDid(r.did, nodes) }}</span>
          <span v-if="r.status === 'ok'" class="text-xs text-green-400">{{ r.apps }} apps</span>
          <span v-else class="text-xs text-red-400 truncate">{{ r.error }}</span>
        </div>
      </div>
    </div>

    <!-- Error Display -->
    <div v-if="error" class="glass-card p-4 mb-6 border-red-400/30">
      <p class="text-sm text-red-400">{{ error }}</p>
    </div>

    <!-- Two-column: Your Nodes + Peers -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">

      <!-- Your Nodes (Trusted) -->
      <div class="glass-card p-6 max-h-[60vh] flex flex-col">
        <h2 class="text-lg font-semibold text-white mb-4">Your Nodes <span v-if="trustedNodes.length > 0" class="text-sm font-normal text-white/50">({{ trustedNodes.length }})</span></h2>

        <div v-if="loading && nodes.length === 0" class="flex items-center gap-3 py-8 justify-center">
          <div class="w-5 h-5 border-2 border-white/20 border-t-orange-400 rounded-full animate-spin"></div>
          <span class="text-white/60 text-sm">Loading nodes...</span>
        </div>

        <div v-else-if="nodes.length === 0" class="text-center py-12">
          <svg class="w-16 h-16 text-white/20 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
          </svg>
          <p class="text-white/50 text-sm mb-2">No federated nodes yet</p>
          <p class="text-white/30 text-xs">Generate an invite code or join an existing federation</p>
        </div>

        <div v-else class="space-y-3 overflow-y-auto">
          <div
            v-for="node in trustedNodes"
            :key="node.did"
            class="bg-black/20 rounded-xl border border-white/10 p-4 cursor-pointer hover:border-white/20 transition-colors"
            @click="$emit('select-node', node)"
          >
            <div class="flex items-center justify-between mb-2">
              <div class="flex items-center gap-3 min-w-0">
                <div class="w-2.5 h-2.5 rounded-full shrink-0" :class="isOnline(node) ? 'bg-green-400' : 'bg-white/30'"></div>
                <span class="text-sm font-medium text-white truncate" :title="node.did">{{ nodeName(node) }}</span>
                <span
                  v-if="transportBadge(node)"
                  class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0"
                  :class="transportBadge(node)!.cls"
                  :title="transportBadge(node)!.title"
                >{{ transportBadge(node)!.label }}</span>
                <span
                  v-if="syncErrorBadge(node)"
                  data-testid="sync-error-badge"
                  class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0 truncate bg-red-500/20 text-red-300 ring-1 ring-red-400/40"
                  :title="syncErrorBadge(node)!.title"
                >SYNC</span>
              </div>
              <span
                class="text-xs px-2 py-0.5 rounded-full shrink-0"
                :class="trustBadgeClass(node.trust_level)"
              >{{ node.trust_level }}</span>
            </div>

            <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs text-white/50">
              <div>
                <span class="text-white/30">Apps:</span>
                {{ node.last_state?.apps?.length ?? '--' }}
              </div>
              <div>
                <span class="text-white/30">CPU:</span>
                {{ node.last_state?.cpu_usage_percent != null ? node.last_state.cpu_usage_percent.toFixed(1) + '%' : '--' }}
              </div>
              <div class="flex items-center gap-1">
                <span class="text-white/30">DWN:</span>
                <span class="w-1.5 h-1.5 rounded-full" :class="dwnSyncDotClass"></span>
              </div>
              <div>
                <span class="text-white/30">Seen:</span>
                {{ node.last_seen ? timeAgo(node.last_seen) : 'never' }}
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Peers (Observer level) -->
      <div class="glass-card p-6 max-h-[60vh] flex flex-col">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-semibold text-white">Peers <span v-if="peerNodes.length > 0" class="text-sm font-normal text-white/50">({{ peerNodes.length }})</span></h2>
          <button
            v-if="nodes.some(n => !isOnline(n) && n.last_seen === 'never')"
            @click="$emit('cleanup-dead')"
            :disabled="cleaningNodes"
            class="glass-button px-3 py-1.5 rounded-lg text-xs text-red-300"
          >
            {{ cleaningNodes ? 'Removing...' : 'Remove Dead Nodes' }}
          </button>
        </div>
        <div v-if="peerNodes.length === 0" class="text-center py-6">
          <p class="text-white/50 text-sm">No peers yet</p>
          <p class="text-white/30 text-xs mt-1">Invite a peer to share public content</p>
        </div>
        <div v-else class="space-y-3 overflow-y-auto">
          <div
            v-for="node in peerNodes"
            :key="node.did"
            class="bg-black/20 rounded-xl border border-white/10 p-4 cursor-pointer hover:border-white/20 transition-colors"
            @click="$emit('select-node', node)"
          >
            <div class="flex items-center justify-between mb-2">
              <div class="flex items-center gap-3 min-w-0">
                <div class="w-2.5 h-2.5 rounded-full shrink-0" :class="isOnline(node) ? 'bg-green-400' : 'bg-white/30'"></div>
                <span class="text-sm font-medium text-white truncate" :title="node.did">{{ nodeName(node) }}</span>
                <span
                  v-if="transportBadge(node)"
                  class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0"
                  :class="transportBadge(node)!.cls"
                  :title="transportBadge(node)!.title"
                >{{ transportBadge(node)!.label }}</span>
                <span
                  v-if="syncErrorBadge(node)"
                  data-testid="sync-error-badge"
                  class="text-[10px] font-semibold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0 truncate bg-red-500/20 text-red-300 ring-1 ring-red-400/40"
                  :title="syncErrorBadge(node)!.title"
                >SYNC</span>
              </div>
              <span class="text-xs px-2 py-0.5 rounded-full shrink-0" :class="trustBadgeClass(node.trust_level)">{{ node.trust_level }}</span>
            </div>
            <div class="text-xs text-white/40">
              <span>Seen: {{ node.last_seen ? formatTimeAgo(node.last_seen) : 'never' }}</span>
            </div>
          </div>
        </div>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { FederatedNode, SyncResult } from './types'
import { nodeName, nodeNameFromDid, timeAgo, formatTimeAgo, trustBadgeClass, isOnline } from './utils'

const props = defineProps<{
  nodes: FederatedNode[]
  loading: boolean
  error: string
  syncResults: SyncResult[]
  dwnSyncDotClass: string
  cleaningNodes: boolean
}>()

defineEmits<{
  'select-node': [node: FederatedNode]
  'clear-sync-results': []
  'cleanup-dead': []
}>()

const trustedNodes = computed(() => props.nodes.filter(n => n.trust_level === 'trusted'))
const peerNodes = computed(() => props.nodes.filter(n => n.trust_level !== 'trusted'))

// Badge showing the actual transport the most recent reach used —
// NOT a prediction. If we've never reached the peer, return null so
// the badge stays hidden rather than lying. When the transport is
// fips, the tooltip also shows how recent the reading is so stale
// data is visible at a glance.
function transportBadge(node: FederatedNode): { label: string; cls: string; title: string } | null {
  if (!node.last_transport) return null
  const age = node.last_transport_at ? timeAgo(node.last_transport_at) : 'unknown'
  switch (node.last_transport) {
    case 'fips':
      return {
        label: 'FIPS',
        cls: 'bg-cyan-500/20 text-cyan-300 ring-1 ring-cyan-400/40',
        title: `Last reached via FIPS mesh · ${age}`,
      }
    case 'tor':
      return {
        label: 'TOR',
        cls: 'bg-purple-500/20 text-purple-300 ring-1 ring-purple-400/40',
        title: `Last reached via Tor · ${age}`,
      }
    case 'lan':
      return {
        label: 'LAN',
        cls: 'bg-green-500/20 text-green-300 ring-1 ring-green-400/40',
        title: `Last reached via LAN · ${age}`,
      }
    case 'mesh':
      return {
        label: 'MESH',
        cls: 'bg-orange-500/20 text-orange-300 ring-1 ring-orange-400/40',
        title: `Last reached via mesh radio · ${age}`,
      }
    default:
      return null
  }
}

// FED-02: the most recent sync attempt with this peer failed. Before this,
// the failure existed only as a `debug!` line on the node, so a peer that
// hadn't synced in days looked identical on this screen to one that synced a
// minute ago. Returns null when the peer's last attempt succeeded — the
// backend clears last_sync_error on success, so the badge disappears on
// recovery rather than sticking around.
//
// The row must stay single-line, so the badge itself is a fixed short label
// and the daemon's message plus when it happened go in the tooltip (the same
// truncate + :title treatment the node-name span uses).
function syncErrorBadge(node: FederatedNode): { title: string } | null {
  if (!node.last_sync_error) return null
  const age = node.last_sync_error_at ? timeAgo(node.last_sync_error_at) : 'unknown'
  return { title: `Last sync failed · ${age} · ${node.last_sync_error}` }
}
</script>
