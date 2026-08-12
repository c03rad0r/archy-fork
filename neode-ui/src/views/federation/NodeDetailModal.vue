<template>
  <Teleport to="body">
  <div v-if="node" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="handleClose">
    <div class="glass-card p-6 w-full max-w-lg max-h-[80vh] overflow-y-auto">
      <div class="flex items-center justify-between mb-6">
        <h2 class="text-xl font-semibold text-white">Node Details</h2>
        <button @click="handleClose" class="text-white/40 hover:text-white/70 transition-colors">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="space-y-4">
        <div class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-1">DID</p>
          <p class="text-sm text-white/80 font-mono break-all">{{ node.did }}</p>
        </div>
        <div class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-1">Onion Address</p>
          <p v-if="node.trust_level === 'trusted'" class="text-sm text-white/80 font-mono break-all">{{ node.onion }}</p>
          <p v-else class="text-sm text-white/30 italic">Not visible to peers</p>
        </div>
        <div class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-1">Trust Level</p>
          <div class="flex items-center gap-2 mt-1">
            <select
              :value="node.trust_level"
              @change="onTrustChange"
              class="bg-black/30 text-white text-sm rounded px-2 py-1 border border-white/10"
            >
              <option value="trusted">Trusted</option>
              <option value="observer">Observer</option>
              <option value="untrusted">Blocked</option>
            </select>
          </div>
          <p class="text-xs text-white/40 mt-2">
            <span class="text-white/30">Granted via:</span> {{ trustSourceLabel }}
          </p>
          <p v-if="actionError" class="text-xs text-red-400 mt-2" role="alert">{{ actionError }}</p>
        </div>
        <div class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-1">Added</p>
          <p class="text-sm text-white/80">{{ node.added_at }}</p>
        </div>

        <div v-if="node.trust_level === 'trusted' && node.last_state" class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-2">Resource Usage</p>
          <div class="grid grid-cols-2 gap-2 text-sm text-white/70">
            <div>CPU: {{ node.last_state.cpu_usage_percent?.toFixed(1) ?? '--' }}%</div>
            <div>Uptime: {{ node.last_state.uptime_secs ? formatUptime(node.last_state.uptime_secs) : '--' }}</div>
            <div>RAM: {{ formatBytes(node.last_state.mem_used_bytes) }} / {{ formatBytes(node.last_state.mem_total_bytes) }}</div>
            <div>Disk: {{ formatBytes(node.last_state.disk_used_bytes) }} / {{ formatBytes(node.last_state.disk_total_bytes) }}</div>
          </div>
        </div>

        <div v-if="node.last_state?.apps?.length && node.trust_level === 'trusted'" class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-2">Apps ({{ node.last_state.apps.length }})</p>
          <div class="space-y-1">
            <div v-for="app in node.last_state.apps" :key="app.id" class="flex items-center justify-between text-sm">
              <span class="text-white/80">{{ app.id }}</span>
              <span class="text-xs" :class="app.status === 'running' ? 'text-green-400' : 'text-white/40'">{{ app.status }}</span>
            </div>
          </div>
        </div>

        <!-- Deploy App (trusted only) -->
        <div v-if="node.trust_level === 'trusted'" class="bg-white/5 rounded-lg p-3">
          <p class="text-xs text-white/40 mb-2">Deploy App</p>
          <div class="flex gap-2">
            <input
              v-model="deployAppId"
              placeholder="App ID (e.g. bitcoin)"
              class="flex-1 bg-black/30 text-white text-sm rounded px-2 py-1.5 border border-white/10 focus:border-orange-400/50 focus:outline-none"
            />
            <button
              @click="handleDeploy"
              class="px-3 py-1.5 glass-button rounded text-xs text-white/90 font-medium disabled:opacity-50"
              :disabled="deploying || !deployAppId.trim()"
            >
              {{ deploying ? 'Deploying...' : 'Deploy' }}
            </button>
          </div>
          <p v-if="deployResult" class="text-xs mt-2" :class="deployResult.startsWith('Error') ? 'text-red-400' : 'text-green-400'">{{ deployResult }}</p>
        </div>

        <!-- DWN Sync -->
        <div class="bg-white/5 rounded-lg p-3">
          <div class="flex items-center justify-between mb-2">
            <p class="text-xs text-white/40">DWN Sync</p>
            <div class="flex items-center gap-1.5">
              <span class="w-1.5 h-1.5 rounded-full" :class="dwnSyncDotClass"></span>
              <span class="text-xs text-white/50">{{ dwnSyncLabel }}</span>
            </div>
          </div>
          <div class="grid grid-cols-2 gap-2 text-sm text-white/70 mb-3">
            <div><span class="text-white/30">Messages:</span> {{ dwnMessageCount }}</div>
            <div><span class="text-white/30">Last sync:</span> {{ dwnLastSync }}</div>
          </div>
          <button
            @click="emit('dwn-sync')"
            class="px-3 py-1.5 glass-button rounded text-xs text-white/90 font-medium disabled:opacity-50"
            :disabled="dwnSyncing"
          >
            {{ dwnSyncing ? 'Syncing...' : 'Sync Now' }}
          </button>
        </div>

        <div v-if="!confirmRemove">
          <button
            @click="confirmRemove = true"
            class="w-full mt-4 px-4 py-2 rounded text-sm glass-button glass-button-danger transition-colors"
          >
            Remove from Federation
          </button>
        </div>
        <div v-else class="mt-4 p-3 bg-red-400/10 rounded-lg border border-red-400/20">
          <p class="text-sm text-red-400 mb-3">Are you sure? This node will be removed from your federation.</p>
          <div class="flex gap-3">
            <button
              @click="confirmRemove = false"
              class="flex-1 px-3 py-1.5 glass-button rounded text-sm text-white/70"
            >Cancel</button>
            <button
              @click="emit('remove-node', node!.did)"
              class="flex-1 px-3 py-1.5 rounded text-sm glass-button glass-button-danger transition-colors font-medium"
            >Confirm Remove</button>
          </div>
        </div>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { FederatedNode } from './types'
import { formatBytes, formatUptime } from './utils'

const props = defineProps<{
  node: FederatedNode | null
  dwnSyncDotClass: string
  dwnSyncLabel: string
  dwnMessageCount: string
  dwnLastSync: string
  dwnSyncing: boolean
  deploying: boolean
  deployResult: string
  /** Failure from an action taken INSIDE this modal (e.g. the trust dropdown).
   *  Federation.vue used to route these to the page-level banner in NodeList,
   *  which sits behind this modal — so a failed trust change looked like
   *  nothing happening at all. */
  actionError?: string
}>()

const emit = defineEmits<{
  close: []
  'change-trust': [did: string, level: string]
  'remove-node': [did: string]
  'deploy-app': [did: string, appId: string]
  'dwn-sync': []
}>()

const confirmRemove = ref(false)
const deployAppId = ref('')

const TRUST_SOURCE_LABELS: Record<string, string> = {
  invite: 'An invite you minted',
  'uninvited-join': 'Joined without an invite — capped at Observer',
  'transitive-merge': 'Advertised by another peer — capped at Observer',
  manual: 'You set it here',
}

/** Unknown provenance is stated plainly rather than hidden: a peer recorded
 *  before this was tracked is precisely the one worth a second look. */
const trustSourceLabel = computed(
  () => TRUST_SOURCE_LABELS[props.node?.trust_source ?? ''] ?? 'Unknown — recorded before this was tracked',
)

/** Snap the select back to the node's actual level immediately. Promoting to
 *  Trusted asks for the node password, and the operator may cancel or get it
 *  wrong — without this the dropdown would keep displaying a level the node
 *  never accepted. On success the parent reloads and the prop drives the new
 *  value back in. */
function onTrustChange(event: Event) {
  const select = event.target as HTMLSelectElement
  const level = select.value
  if (!props.node) return
  select.value = props.node.trust_level
  emit('change-trust', props.node.did, level)
}

function handleClose() {
  confirmRemove.value = false
  deployAppId.value = ''
  emit('close')
}

function handleDeploy() {
  if (props.node && deployAppId.value.trim()) {
    emit('deploy-app', props.node.did, deployAppId.value.trim())
    deployAppId.value = ''
  }
}
</script>
