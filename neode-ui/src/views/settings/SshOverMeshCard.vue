<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { useToast } from '@/composables/useToast'

interface SshOverMeshStatus {
  enabled: boolean
  sources: string[]
  scope: 'any' | 'list'
  preflights?: {
    sshd_active: boolean
    sshd_ipv6_listen: boolean
    password_auth: boolean | null
  }
  applied?: boolean
  reloaded?: boolean
}

const toast = useToast()
const status = ref<SshOverMeshStatus | null>(null)
const loading = ref(true)
const error = ref('')
const saving = ref(false)
const confirmAnyPeer = ref(false)
const pendingEnableAny = ref(false)
const sourcesText = ref('')
const fipsNpub = ref<string | null>(null)

async function refresh() {
  loading.value = true
  error.value = ''
  try {
    status.value = (await rpcClient.call({ method: 'fips.ssh-over-mesh.get' })) as SshOverMeshStatus
    sourcesText.value = (status.value.sources || []).join('\n')
    if (!fipsNpub.value) {
      try {
        const s = (await rpcClient.call<{ npub?: string }>({ method: 'fips.status', dedup: true, maxRetries: 1 }))
        fipsNpub.value = s.npub ?? null
      } catch { /* npub hint is optional */ }
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load SSH-over-mesh state'
  } finally {
    loading.value = false
  }
}

/** The unrestricted scope opens port 22 to every mesh peer — demand an
 *  explicit confirmation before applying it, like the other danger zones. */
async function toggle(enabled: boolean) {
  const sources = enabled && status.value?.scope === 'list'
    ? sourcesText.value.split(/[\n,]+/).map(s => s.trim()).filter(Boolean)
    : (status.value?.sources ?? [])
  if (enabled && sources.length === 0) {
    pendingEnableAny.value = true
    confirmAnyPeer.value = true
    return
  }
  await apply(enabled, sources)
}

async function apply(enabled: boolean, sources: string[]) {
  saving.value = true
  try {
    status.value = (await rpcClient.call({
      method: 'fips.ssh-over-mesh.set',
      params: { enabled, sources },
      timeout: 30_000,
    })) as SshOverMeshStatus
    sourcesText.value = (status.value.sources || []).join('\n')
    toast.success(enabled ? 'SSH over mesh allowed' : 'SSH over mesh blocked')
    if (enabled && status.value.reloaded === false) {
      toast.error('Rule saved, but the firewall reload failed — reconnect or check logs')
    }
  } catch (e) {
    toast.error(e instanceof Error ? e.message : 'Failed to apply')
  } finally {
    saving.value = false
    pendingEnableAny.value = false
    confirmAnyPeer.value = false
  }
}

function copyFipssh() {
  if (!fipsNpub.value) return
  const cmd = `fipssh <user>@${fipsNpub.value}`
  navigator.clipboard?.writeText(cmd).then(
    () => toast.success('Copied: ' + cmd),
    () => toast.error('Copy failed'),
  )
}

onMounted(refresh)
</script>

<template>
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-start justify-between gap-4 mb-2">
      <div>
        <h2 class="text-xl font-semibold text-white/96">SSH over mesh</h2>
        <p class="text-sm text-white/60 mt-1">
          Let the phone's FIPS mesh reach this node's SSH (port 22). Off by default — the mesh
          firewall refuses SSH until you allow it here.
        </p>
      </div>
      <button
        class="path-action-button path-action-button--continue px-4 py-2 text-sm shrink-0"
        :disabled="saving || loading"
        :class="{ '!bg-orange-500/30 !border-orange-400/50': status?.enabled }"
        @click="toggle(!status?.enabled)"
      >
        {{ status?.enabled ? 'Allowed — block again' : 'Allow SSH over mesh' }}
      </button>
    </div>

    <div v-if="loading" class="text-sm text-white/50 py-3">Loading…</div>
    <div v-else-if="error" class="text-sm text-red-300 py-3">{{ error }}
      <button class="ml-2 underline" @click="refresh">Retry</button>
    </div>
    <template v-else-if="status">
      <!-- Preflights: explain the toggle rather than gating it -->
      <div class="mt-3 space-y-1.5 text-sm">
        <div class="flex items-center gap-2">
          <span :class="status.preflights?.sshd_active ? 'text-green-400' : 'text-orange-300'">●</span>
          <span class="text-white/80">sshd {{ status.preflights?.sshd_active ? 'is running' : 'is NOT running' }}</span>
        </div>
        <div class="flex items-center gap-2">
          <span :class="status.preflights?.sshd_ipv6_listen ? 'text-green-400' : 'text-orange-300'">●</span>
          <span class="text-white/80">
            {{ status.preflights?.sshd_ipv6_listen ? 'listens on IPv6 (mesh-reachable)' : 'does not listen on IPv6 — the mesh cannot reach it' }}
          </span>
        </div>
        <div class="flex items-center gap-2">
          <span :class="status.preflights?.password_auth === false ? 'text-green-400' : 'text-orange-300'">●</span>
          <span class="text-white/80">
            {{ status.preflights?.password_auth == null
              ? 'PasswordAuthentication unknown'
              : status.preflights?.password_auth
                ? 'password login allowed — keys-only is the safer pairing for the firewall rule'
                : 'keys-only login (recommended)' }}
          </span>
        </div>
      </div>

      <!-- Source restriction -->
      <div class="mt-4">
        <p class="text-xs text-white/50 mb-1">Restrict to specific mesh addresses (one per line), or leave empty to allow any mesh peer.</p>
        <textarea
          v-model="sourcesText"
          rows="2"
          class="w-full bg-white/10 border border-white/20 rounded-lg p-2 text-sm text-white/90 font-mono placeholder-white/30"
          placeholder="fd79:… phone mesh address"
          :disabled="!status.enabled || saving"
          @change="apply(true, sourcesText.split(/[\n,]+/).map(s => s.trim()).filter(Boolean))"
        />
      </div>

      <!-- Phone-side hint: the npub is the durable address -->
      <div v-if="status.enabled && fipsNpub" class="mt-3 flex items-center gap-2 text-sm">
        <span class="text-white/60">From Termux:</span>
        <code class="text-orange-200 truncate max-w-[55%]">fipssh &lt;user&gt;@{{ fipsNpub.slice(0, 12) }}…</code>
        <button class="glass-button px-2 py-1 text-xs" @click="copyFipssh">Copy</button>
      </div>
    </template>

    <!-- Danger-zone confirmation for the unrestricted scope -->
    <Teleport to="body">
      <div v-if="confirmAnyPeer" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="confirmAnyPeer = false">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div class="glass-card p-6 max-w-md w-full relative z-10">
          <h3 class="text-lg font-semibold text-white mb-3">Allow SSH from ANY mesh peer?</h3>
          <p class="text-sm text-white/70 mb-4">
            Anyone who can route to this node over the FIPS mesh will reach port 22. Restricting
            to your phone's mesh address above is the safer pairing. Only continue if you
            understand port 22 will face every mesh peer.
          </p>
          <div class="flex gap-3">
            <button class="flex-1 glass-button px-4 py-2 text-sm" @click="confirmAnyPeer = false">Restrict instead</button>
            <button
              class="flex-1 px-4 py-2 rounded-lg text-sm border border-orange-400/50 bg-orange-500/20 text-orange-200 hover:bg-orange-500/30"
              :disabled="saving || pendingEnableAny === false"
              @click="apply(true, [])"
            >I understand — allow any</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
