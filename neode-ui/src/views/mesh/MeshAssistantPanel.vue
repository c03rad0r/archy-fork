<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useMeshStore } from '@/stores/mesh'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const mesh = useMeshStore()
const saving = ref(false)

const status = computed(() => mesh.assistantStatus)
const enabled = ref(false)
const model = ref('') // '' = use the backend's default model
const policy = ref<'trusted' | 'anyone'>('trusted')
const backend = ref<'claude' | 'ollama'>('claude')
const allowedContacts = ref<string[]>([])

// Preset Claude models offered in the dropdown ('' = backend default = Haiku).
const CLAUDE_MODELS: { value: string; label: string }[] = [
  { value: '', label: 'Default (Claude Haiku 4.5)' },
  { value: 'claude-haiku-4-5-20251001', label: 'Claude Haiku 4.5 — fast & cheap' },
  { value: 'claude-sonnet-4-6', label: 'Claude Sonnet 4.6 — balanced' },
  { value: 'claude-opus-4-8', label: 'Claude Opus 4.8 — most capable' },
]
// Include any non-preset value the node already has so it isn't silently lost.
const claudeModelOptions = computed(() => {
  const opts = [...CLAUDE_MODELS]
  if (model.value && !opts.some((o) => o.value === model.value)) {
    opts.push({ value: model.value, label: `${model.value} (custom)` })
  }
  return opts
})

// Sync local controls from the fetched status.
watch(
  status,
  (s) => {
    if (!s) return
    enabled.value = s.enabled
    model.value = s.model ?? ''
    policy.value = s.trusted_only ? 'trusted' : 'anyone'
    backend.value = s.backend === 'ollama' ? 'ollama' : 'claude'
    allowedContacts.value = [...(s.allowed_contacts ?? [])]
  },
  { immediate: true },
)

// Addressable contacts (have an archipelago/radio pubkey) for the allowlist.
const contactOptions = computed(() =>
  mesh.peers
    .filter((p) => !!p.pubkey_hex)
    .map((p) => ({ pubkey: p.pubkey_hex as string, name: p.advert_name || (p.pubkey_hex as string).slice(0, 10) })),
)

function isAllowed(pubkey: string) {
  return allowedContacts.value.some((k) => k.toLowerCase() === pubkey.toLowerCase())
}
function toggleAllowed(pubkey: string) {
  if (isAllowed(pubkey)) {
    allowedContacts.value = allowedContacts.value.filter((k) => k.toLowerCase() !== pubkey.toLowerCase())
  } else {
    allowedContacts.value = [...allowedContacts.value, pubkey]
  }
  apply({ allowed_contacts: allowedContacts.value })
}

// Manually pasting a raw ed25519 pubkey (hex) — for an allowed asker that
// isn't in the contact list yet (e.g. a phone/meshcore device).
const newPubkey = ref('')
const pubkeyError = ref('')
// Allowlisted keys that aren't one of our known contacts (manually added).
const extraAllowed = computed(() =>
  allowedContacts.value.filter(
    (k) => !contactOptions.value.some((c) => c.pubkey.toLowerCase() === k.toLowerCase()),
  ),
)
function addPubkey() {
  const pk = newPubkey.value.trim().toLowerCase()
  pubkeyError.value = ''
  if (!/^[0-9a-f]{64}$/.test(pk)) {
    pubkeyError.value = 'Enter a 64-character hex ed25519 public key.'
    return
  }
  if (allowedContacts.value.some((k) => k.toLowerCase() === pk)) {
    newPubkey.value = ''
    return
  }
  allowedContacts.value = [...allowedContacts.value, pk]
  newPubkey.value = ''
  apply({ allowed_contacts: allowedContacts.value })
}

// Radio/peers who recently tried `!ai` and were turned away by the policy.
// Surfaced so the operator can one-click allow them instead of digging through
// the journal for the firmware key. Hide any we've since allowed.
const deniedAskers = computed(() =>
  (status.value?.denied_askers ?? []).filter(
    (d) => !d.pubkey_hex || !isAllowed(d.pubkey_hex),
  ),
)
function allowDenied(pubkey: string | null) {
  if (!pubkey) return
  if (!isAllowed(pubkey)) {
    allowedContacts.value = [...allowedContacts.value, pubkey]
    apply({ allowed_contacts: allowedContacts.value })
  }
}

onMounted(() => {
  mesh.fetchAssistantStatus()
})

const claudeReady = computed(() => status.value?.claude_available ?? false)
const ollamaReady = computed(() => status.value?.ollama_detected ?? false)
const availableModels = computed(() => status.value?.models ?? [])
const defaultModel = computed(() =>
  backend.value === 'claude' ? 'Claude Haiku 4.5' : status.value?.default_model ?? 'qwen2.5-coder',
)
// The selected backend is usable when its provider is available.
const backendReady = computed(() =>
  backend.value === 'claude' ? claudeReady.value : ollamaReady.value,
)

async function apply(partial: {
  enabled?: boolean
  model?: string | null
  trusted_only?: boolean
  backend?: string
  allowed_contacts?: string[]
}) {
  saving.value = true
  try {
    await mesh.configureAssistant(partial)
  } finally {
    saving.value = false
  }
}

function onToggle(val: boolean) {
  enabled.value = val
  apply({ enabled: val })
}
function onBackend() {
  // Reset the model override when switching backend (models differ).
  model.value = ''
  apply({ backend: backend.value, model: null })
}
function onModel() {
  apply({ model: model.value === '' ? null : model.value })
}
function onPolicy() {
  apply({ trusted_only: policy.value === 'trusted' })
}
</script>

<template>
  <div class="glass-card mesh-assistant-panel">
    <h3 class="mesh-panel-title">AI Assistant</h3>

    <!-- Backend chooser -->
    <div class="mesh-assistant-field">
      <label class="mesh-bitcoin-label">AI backend</label>
      <select v-model="backend" class="mesh-bitcoin-input mesh-bitcoin-input-sm" @change="onBackend">
        <option value="claude">Claude (shared token — no GPU needed)</option>
        <option value="ollama">Local model (Ollama on this node)</option>
      </select>
    </div>

    <!-- Provider missing -> guidance / deep-link -->
    <div v-if="status && backend === 'ollama' && !ollamaReady" class="mesh-assistant-install">
      <p class="text-sm text-white/70 mb-3">
        Local mode needs the <strong>Ollama</strong> app installed and running.
      </p>
      <RouterLink to="/dashboard/marketplace/ollama" class="glass-button mesh-assistant-install-btn">
        Install AI (Ollama)
      </RouterLink>
    </div>
    <div v-else-if="status && backend === 'claude' && !claudeReady" class="mesh-assistant-install">
      <p class="text-sm text-white/70">
        No Claude API token is configured on this node yet. Add one in Settings to use the shared
        Claude backend, or switch to a local model.
      </p>
    </div>

    <!-- Enable toggle -->
    <button
      type="button"
      class="w-full flex items-center gap-4 p-4 rounded-xl border transition-all text-left"
      :class="enabled ? 'bg-white/10 border-orange-500/40' : 'bg-black/20 border-white/10 hover:border-white/20'"
      :style="!backendReady ? 'opacity:0.5;cursor:not-allowed' : ''"
      @click="backendReady && onToggle(!enabled)"
    >
      <svg class="w-5 h-5 shrink-0" :class="enabled ? 'text-orange-400' : 'text-white/40'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      </svg>
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium" :class="enabled ? 'text-white/95' : 'text-white/70'">
          {{ enabled ? 'Answering mesh AI queries' : 'Answer mesh AI queries' }}
        </p>
        <p class="text-xs text-white/50 mt-0.5">Peers can ask this node's AI over the radio</p>
      </div>
      <ToggleSwitch :model-value="enabled" @click.stop @update:model-value="backendReady && onToggle($event)" />
    </button>

    <template v-if="enabled">
      <div v-if="backend === 'ollama'" class="mesh-assistant-field">
        <label class="mesh-bitcoin-label">Model</label>
        <select v-model="model" class="mesh-bitcoin-input mesh-bitcoin-input-sm" @change="onModel">
          <option value="">Default ({{ defaultModel }})</option>
          <option v-for="m in availableModels" :key="m" :value="m">{{ m }}</option>
        </select>
      </div>
      <div v-else class="mesh-assistant-field">
        <label class="mesh-bitcoin-label">Model</label>
        <select v-model="model" class="mesh-bitcoin-input mesh-bitcoin-input-sm" @change="onModel">
          <option v-for="m in claudeModelOptions" :key="m.value" :value="m.value">{{ m.label }}</option>
        </select>
      </div>

      <div class="mesh-assistant-field">
        <label class="mesh-bitcoin-label">Who can ask</label>
        <select v-model="policy" class="mesh-bitcoin-input mesh-bitcoin-input-sm" @change="onPolicy">
          <option value="trusted">Trusted nodes only</option>
          <option value="anyone">Anyone on the mesh</option>
        </select>
        <p v-if="policy === 'anyone'" class="text-xs text-white/40 mt-1">
          Any peer can spend this node's AI budget + airtime.
        </p>
      </div>

      <!-- Per-contact allowlist: let specific contacts use !ai even when the
           policy is "trusted only" and they aren't federation-trusted. -->
      <div class="mesh-assistant-field">
        <label class="mesh-bitcoin-label">Always allow these contacts</label>
        <div v-if="contactOptions.length === 0" class="text-xs text-white/40">
          No contacts yet — they appear here once you have mesh/federation contacts.
        </div>
        <div v-else class="mesh-assistant-allowlist">
          <label
            v-for="c in contactOptions"
            :key="c.pubkey"
            class="mesh-assistant-allow-row"
          >
            <input type="checkbox" :checked="isAllowed(c.pubkey)" @change="toggleAllowed(c.pubkey)" />
            <span class="mesh-assistant-allow-name">{{ c.name }}</span>
          </label>
          <!-- Manually-added pubkeys not in the contact list -->
          <label
            v-for="pk in extraAllowed"
            :key="pk"
            class="mesh-assistant-allow-row"
          >
            <input type="checkbox" checked @change="toggleAllowed(pk)" />
            <span class="mesh-assistant-allow-name" :title="pk">{{ pk.slice(0, 10) }}… (added)</span>
          </label>
        </div>

        <!-- Add an arbitrary pubkey directly -->
        <div class="mesh-assistant-addkey">
          <input
            v-model="newPubkey"
            class="mesh-bitcoin-input mesh-bitcoin-input-sm"
            placeholder="Paste an ed25519 pubkey (64 hex) to allow"
            @keyup.enter="addPubkey"
          />
          <button type="button" class="glass-button mesh-bitcoin-input-sm" @click="addPubkey">Add</button>
        </div>
        <p v-if="pubkeyError" class="text-xs mt-1" style="color:#f87171">{{ pubkeyError }}</p>
      </div>

      <!-- Recently denied askers: someone tried !ai but the policy turned them
           away. Show who, and offer a one-click Allow when we know their key. -->
      <div v-if="deniedAskers.length > 0" class="mesh-assistant-field">
        <label class="mesh-bitcoin-label">Recently denied</label>
        <p class="text-xs text-white/40 mb-2">
          These tried <code>!ai</code> but the policy turned them away. Allow one to add its key.
        </p>
        <div class="mesh-assistant-allowlist">
          <div
            v-for="d in deniedAskers"
            :key="d.contact_id + (d.pubkey_hex || '')"
            class="mesh-assistant-allow-row"
          >
            <span class="mesh-assistant-allow-name" :title="d.pubkey_hex || ''">
              {{ d.name || ('#' + d.contact_id) }}
              <span v-if="d.pubkey_hex" class="text-white/30">· {{ d.pubkey_hex.slice(0, 10) }}…</span>
            </span>
            <button
              v-if="d.pubkey_hex"
              type="button"
              class="glass-button mesh-bitcoin-input-sm mesh-assistant-allow-btn"
              @click="allowDenied(d.pubkey_hex)"
            >
              Allow
            </button>
            <span v-else class="text-xs text-white/30" title="No archipelago key advertised — switch policy to 'Anyone on the mesh' to admit this device.">
              no key
            </span>
          </div>
        </div>
      </div>

      <p class="text-xs text-white/50 mt-2">
        Ask from any client by sending <code>!ai &lt;question&gt;</code> on the mesh channel.
      </p>
    </template>
  </div>
</template>
