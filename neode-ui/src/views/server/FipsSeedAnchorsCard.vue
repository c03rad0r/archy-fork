<template>
  <div data-controller-container tabindex="0" class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1 relative">
    <button
      v-if="closable"
      type="button"
      class="absolute top-4 right-4 p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors z-10"
      aria-label="Close"
      @click="$emit('close')"
    >
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
    <div class="flex items-start gap-4 mb-4 shrink-0" :class="{ 'pr-10': closable }">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <!-- Radio/broadcast icon — three concentric arcs radiating from a
             dot. Reads as mesh, signal, anchor-reaching-peers. -->
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.288 15.038a5.25 5.25 0 017.424 0M5.106 11.856c3.807-3.808 9.98-3.808 13.788 0M1.924 8.674c5.565-5.565 14.587-5.565 20.152 0" />
          <circle cx="12" cy="18" r="1.25" fill="currentColor" stroke="none" />
        </svg>
      </div>
      <div class="flex-1">
        <div class="flex items-start justify-between gap-4 mb-2">
          <h2 class="text-xl font-semibold text-white">FIPS Seed Anchors</h2>
          <button
            type="button"
            class="text-xs px-3 py-1.5 rounded-md bg-white/5 hover:bg-white/10 text-white/70 hover:text-white transition-colors disabled:opacity-60"
            :disabled="applying"
            :title="applying ? 'Applying…' : 'Re-dial every anchor in the list'"
            @click="applyAll"
          >
            {{ applying ? 'Applying…' : 'Apply now' }}
          </button>
        </div>
        <p class="text-white/70 text-sm mb-4">
          Peers this node dials to bootstrap the FIPS mesh. A cluster with its own anchors doesn't depend on the global public anchor — if one is down, the next seeds the DHT instead.
        </p>
      </div>
    </div>

    <div v-if="statusMessage" class="mb-3 p-3 rounded-lg text-xs" :class="statusIsError ? 'bg-red-400/10 text-red-300' : 'bg-green-400/10 text-green-300'">{{ statusMessage }}</div>

    <div v-if="anchors.length === 0" class="p-4 rounded-lg bg-white/5 text-sm text-white/60 mb-3">
      <p>No seed anchors configured. The daemon will fall back to whatever the upstream FIPS build dials on its own — usually the single public anchor, which is fine until it isn't.</p>
      <p class="mt-2 text-white/50">Add at least one known-reachable peer (e.g. your VPS or a home node with port-forwarded UDP 8668) to make this cluster self-anchoring.</p>
    </div>

    <ul v-else class="space-y-2 mb-3">
      <li v-for="a in anchors" :key="a.npub" class="p-3 bg-white/5 rounded-lg flex items-start gap-3">
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium text-white truncate">{{ a.label || 'Unlabeled anchor' }}</p>
          <p class="text-xs text-white/60 font-mono break-all">{{ a.npub.slice(0, 20) }}…{{ a.npub.slice(-8) }}</p>
          <p class="text-xs text-white/50 mt-0.5">{{ a.address }} · {{ a.transport }}</p>
        </div>
        <button
          type="button"
          class="shrink-0 text-xs px-2 py-1 rounded-md text-red-300 hover:bg-red-400/10 transition-colors"
          :title="`Remove ${a.label || a.npub.slice(0, 12)}`"
          @click="removeAnchor(a.npub)"
        >Remove</button>
      </li>
    </ul>

    <form class="grid grid-cols-1 sm:grid-cols-2 gap-2 mt-auto pt-3 border-t border-white/10 shrink-0" @submit.prevent="addAnchor">
      <label class="flex flex-col gap-1 sm:col-span-2">
        <span class="text-xs text-white/60">Anchor npub</span>
        <input v-model="draft.npub" type="text" placeholder="npub1…" class="px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none" />
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-xs text-white/60">Address (host:port)</span>
        <input v-model="draft.address" type="text" placeholder="185.18.221.160:8443" class="px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none" />
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-xs text-white/60">Transport</span>
        <select v-model="draft.transport" class="px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none">
          <option value="tcp">tcp (public anchor / over internet)</option>
          <option value="udp">udp (same LAN)</option>
        </select>
      </label>
      <label class="flex flex-col gap-1 sm:col-span-2">
        <span class="text-xs text-white/60">Label (optional)</span>
        <input v-model="draft.label" type="text" placeholder="Public anchor" class="px-3 py-2 rounded-md bg-white/5 border border-white/10 text-sm text-white focus:border-white/30 focus:outline-none" />
      </label>
      <button type="submit" class="sm:col-span-2 min-h-[44px] glass-button rounded-lg text-sm font-medium disabled:opacity-60" :disabled="adding || !draft.npub || !draft.address">{{ adding ? 'Adding…' : 'Add anchor' }}</button>
    </form>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'

defineProps<{ closable?: boolean }>()
defineEmits<{ (e: 'close'): void }>()

interface SeedAnchor {
  npub: string
  address: string
  transport: string
  label: string
}

interface ApplyResult {
  npub: string
  ok: boolean
  message: string
}

const anchorsRes = useCachedResource<SeedAnchor[]>({
  key: 'server.fips-seed-anchors',
  fetcher: async (signal) => {
    const res = await rpcClient.call<{ seed_anchors: SeedAnchor[] }>({
      method: 'fips.list-seed-anchors', signal, dedup: true, maxRetries: 1,
    })
    return res.seed_anchors
  },
  persist: false, // SeedAnchor carries an npub (peer identity key material) — T-02-01
})
const anchors = computed(() => anchorsRes.data.value ?? [])
const adding = ref(false)
const applying = ref(false)
const statusMessage = ref('')
const statusIsError = ref(false)

const draft = reactive<Pick<SeedAnchor, 'npub' | 'address' | 'transport' | 'label'>>({
  npub: '',
  address: '',
  transport: 'tcp',
  label: '',
})

function flash(msg: string, isError = false) {
  statusMessage.value = msg
  statusIsError.value = isError
  setTimeout(() => { statusMessage.value = '' }, 6000)
}

async function addAnchor() {
  if (!draft.npub.trim() || !draft.address.trim()) return
  adding.value = true
  try {
    const res = await rpcClient.call<{ seed_anchors: SeedAnchor[]; apply: ApplyResult[] }>({
      method: 'fips.add-seed-anchor',
      params: {
        npub: draft.npub.trim(),
        address: draft.address.trim(),
        transport: draft.transport,
        label: draft.label.trim(),
      },
    })
    anchorsRes.optimistic(() => res.seed_anchors) // authoritative post-add list
    draft.npub = ''
    draft.address = ''
    draft.label = ''
    const applied = res.apply.find(r => r.ok)
    flash(applied ? 'Anchor added and dialed.' : 'Anchor saved — dial failed, will retry on the next apply cycle.', !applied)
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Add failed: ${msg}`, true)
  } finally {
    adding.value = false
  }
}

async function removeAnchor(npub: string) {
  try {
    const res = await rpcClient.call<{ seed_anchors: SeedAnchor[] }>({
      method: 'fips.remove-seed-anchor',
      params: { npub },
    })
    anchorsRes.optimistic(() => res.seed_anchors)
    flash('Anchor removed.')
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Remove failed: ${msg}`, true)
  }
}

async function applyAll() {
  applying.value = true
  try {
    const res = await rpcClient.call<{ applied: number; results: ApplyResult[] }>({ method: 'fips.apply-seed-anchors' })
    const ok = res.results.filter(r => r.ok).length
    flash(`${ok} of ${res.applied} anchor${res.applied === 1 ? '' : 's'} dialed.`, ok === 0 && res.applied > 0)
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Apply failed: ${msg}`, true)
  } finally {
    applying.value = false
  }
}
</script>
