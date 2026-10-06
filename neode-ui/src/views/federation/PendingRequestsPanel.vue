<template>
  <div id="connection-requests" v-if="visibleRequests.length > 0" class="glass-card p-6 mb-6">
    <div class="flex items-center justify-between mb-4">
      <div>
        <h2 class="text-base font-semibold text-white">Nostr Connection Requests</h2>
        <p class="text-xs text-white/60">
          Inbound requests await your approval. Outbound requests show what you've sent.
          Connections become <span class="text-orange-300">Observer</span> peers.
          Approval delivery retries automatically while the other node connects.
        </p>
      </div>
      <button
        class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/80 hover:text-white disabled:opacity-50"
        :disabled="polling"
        @click="$emit('poll')"
      >
        {{ polling ? 'Polling…' : 'Poll Now' }}
      </button>
    </div>

    <div class="space-y-3">
      <div
        v-for="req in visibleRequests"
        :key="req.id"
        class="p-3 bg-white/5 rounded-lg border border-white/10"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2 flex-wrap">
              <span
                class="inline-block px-2 py-0.5 text-[10px] uppercase tracking-wide rounded"
                :class="badgeClass(req)"
              >{{ badgeLabel(req) }}</span>
              <span class="text-sm text-white font-medium truncate">
                {{ req.from_name || shortNpub(req.from_nostr_npub) }}
              </span>
            </div>
            <div class="text-[11px] text-white/50 font-mono truncate">{{ req.from_nostr_npub }}</div>
            <div v-if="req.from_did" class="text-[11px] text-white/40 font-mono truncate">{{ req.from_did }}</div>
            <p v-if="req.message" class="mt-2 text-xs text-white/70 italic">"{{ req.message }}"</p>
            <p class="mt-1 text-[10px] text-white/40">{{ relative(req.received_at) }}</p>
          </div>
          <div v-if="req.state === 'pending' && !req.outbound" class="flex flex-col gap-2 shrink-0">
            <button
              class="px-3 py-1 glass-button glass-button-sm rounded text-xs text-green-300 hover:text-green-200 disabled:opacity-50"
              :disabled="busyId === req.id"
              @click="$emit('approve', req.id)"
            >
              {{ busyId === req.id ? '…' : 'Approve' }}
            </button>
            <button
              class="px-3 py-1 glass-button glass-button-sm rounded text-xs text-red-300 hover:text-red-200 disabled:opacity-50"
              :disabled="busyId === req.id"
              @click="$emit('reject', req.id)"
            >
              Reject
            </button>
          </div>
          <div v-else-if="req.outbound && req.state === 'sent'" class="flex flex-col gap-2 shrink-0">
            <button
              class="px-3 py-1 glass-button glass-button-sm rounded text-xs text-white/70 hover:text-red-300 disabled:opacity-50"
              :disabled="busyId === req.id"
              :title="'Withdraw the request and notify the peer to drop their pending row'"
              @click="$emit('cancel', req.id)"
            >
              {{ busyId === req.id ? '…' : 'Cancel' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { PendingPeerRequest } from '@/api/rpc-client'

const props = defineProps<{
  requests: PendingPeerRequest[]
  polling: boolean
  busyId: string | null
  peerDids?: string[]
}>()

defineEmits<{
  poll: []
  approve: [id: string]
  reject: [id: string]
  cancel: [id: string]
}>()

// Hide already-handled rows older than 24h to keep the panel from growing
// indefinitely. The backend keeps the full audit trail in the file; the UI
// only shows what's actionable or recent.
const visibleRequests = computed(() => {
  const cutoff = Date.now() - 24 * 60 * 60 * 1000
  return props.requests.filter((r) => {
    if (r.state === 'pending' || r.state === 'sent') return true
    if (r.state === 'approved' && !props.peerDids?.includes(r.from_did)) return true
    const ts = new Date(r.received_at).getTime()
    return Number.isFinite(ts) && ts >= cutoff
  })
})

function badgeClass(r: PendingPeerRequest): string {
  if (r.state === 'pending') return 'bg-yellow-500/20 text-yellow-300'
  if (r.state === 'sent') return 'bg-blue-500/20 text-blue-300'
  if (r.state === 'approved') return props.peerDids?.includes(r.from_did)
    ? 'bg-green-500/20 text-green-300' : 'bg-yellow-500/20 text-yellow-300'
  if (r.state === 'rejected') return 'bg-red-500/20 text-red-300'
  return 'bg-white/10 text-white/50'
}

function badgeLabel(r: PendingPeerRequest): string {
  if (r.outbound && r.state === 'sent') return 'Sent'
  if (r.state === 'approved') {
    if (props.peerDids?.includes(r.from_did)) return 'Peer added'
    const age = Date.now() - new Date(r.received_at).getTime()
    if (!Number.isFinite(age) || age > 30 * 24 * 60 * 60 * 1000) return 'Approval expired — reconnect'
    return r.outbound ? 'They approved — connecting' : 'Approved — connecting'
  }
  if (r.outbound && r.state === 'rejected') return 'They rejected'
  if (r.outbound) return r.state
  if (r.state === 'pending') return 'Inbound'
  return r.state
}

function shortNpub(npub: string): string {
  if (!npub || npub.length < 16) return npub
  return `${npub.slice(0, 12)}…${npub.slice(-6)}`
}

function relative(iso: string): string {
  const ts = new Date(iso).getTime()
  if (!Number.isFinite(ts)) return iso
  const diff = Math.max(0, Date.now() - ts)
  const sec = Math.floor(diff / 1000)
  if (sec < 60) return `${sec}s ago`
  const min = Math.floor(sec / 60)
  if (min < 60) return `${min}m ago`
  const hr = Math.floor(min / 60)
  if (hr < 24) return `${hr}h ago`
  const day = Math.floor(hr / 24)
  return `${day}d ago`
}
</script>
