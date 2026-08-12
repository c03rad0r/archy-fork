<script setup lang="ts">
import { ref, watch } from 'vue'
import { useMeshStore } from '@/stores/mesh'
import { rpcClient } from '@/api/rpc-client'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const mesh = useMeshStore()

// Bitcoin-headers-over-mesh send/receive toggles (issue #28). Initialized from
// mesh.status (which now carries the persisted prefs) and saved via mesh.configure.
const announceHeaders = ref(false)
const receiveHeaders = ref(true)
const headersSaving = ref(false)
watch(
  () => mesh.status,
  (s) => {
    if (!s) return
    if (typeof s.announce_block_headers === 'boolean') announceHeaders.value = s.announce_block_headers
    if (typeof s.receive_block_headers === 'boolean') receiveHeaders.value = s.receive_block_headers
  },
  { immediate: true, deep: true }
)
async function setAnnounceHeaders(v: boolean) {
  announceHeaders.value = v
  headersSaving.value = true
  try { await mesh.configure({ announce_block_headers: v }) } catch { /* surfaced via store error */ } finally { headersSaving.value = false }
}
async function setReceiveHeaders(v: boolean) {
  receiveHeaders.value = v
  headersSaving.value = true
  try { await mesh.configure({ receive_block_headers: v }) } catch { /* surfaced via store error */ } finally { headersSaving.value = false }
}

const txHexInput = ref('')
const bolt11Input = ref('')
const bolt11AmountInput = ref('')
const relayingTx = ref(false)
const relayingLn = ref(false)
const relayResult = ref('')
const meshSendAddr = ref('')
const meshSendAmount = ref('')
const relayMode = ref<'archy' | 'broadcast'>('archy')
const sendTab = ref<'onchain' | 'lightning'>('onchain')

function pollRelayStatus(requestId: number) {
  let attempts = 0
  const maxAttempts = 30
  const interval = setInterval(async () => {
    attempts++
    try {
      const res = await mesh.relayStatus(requestId)
      if (res.status === 'confirmed' && res.txid) {
        relayResult.value = `TX broadcast! txid: ${res.txid.slice(0, 8)}...${res.txid.slice(-8)}`
        clearInterval(interval)
      } else if (res.status === 'failed') {
        const code = res.error_code ? ` [${res.error_code}]` : ''
        relayResult.value = `Relay failed${code}: ${res.error || 'unknown error'}`
        clearInterval(interval)
      } else if (attempts >= maxAttempts) {
        relayResult.value += ' (timed out waiting for confirmation)'
        clearInterval(interval)
      }
    } catch {
      if (attempts >= maxAttempts) clearInterval(interval)
    }
  }, 3000)
}

async function handleMeshSendBitcoin() {
  if (!meshSendAddr.value.trim() || !meshSendAmount.value) return
  relayingTx.value = true
  relayResult.value = ''
  try {
    relayResult.value = 'Creating signed transaction...'
    const rawRes = await rpcClient.call<{ raw_tx_hex: string; amount_sats: number }>({
      method: 'lnd.create-raw-tx',
      params: { addr: meshSendAddr.value.trim(), amount_sats: parseInt(meshSendAmount.value) },
    })
    relayResult.value = relayMode.value === 'broadcast'
      ? 'Broadcasting via mesh network...'
      : 'Sending to Archy peers (encrypted)...'
    const relayRes = await mesh.relayTransaction(rawRes.raw_tx_hex, relayMode.value)
    relayResult.value = `Sent via mesh! Request #${relayRes.request_id} — waiting for relay peer to broadcast...`
    meshSendAddr.value = ''
    meshSendAmount.value = ''
    pollRelayStatus(relayRes.request_id)
  } catch (err: unknown) {
    relayResult.value = err instanceof Error ? err.message : 'Send failed'
  } finally {
    relayingTx.value = false
  }
}

async function handleRelayTx() {
  if (!txHexInput.value.trim()) return
  relayingTx.value = true
  relayResult.value = ''
  try {
    const res = await mesh.relayTransaction(txHexInput.value.trim())
    relayResult.value = `TX queued (request #${res.request_id})`
    txHexInput.value = ''
  } catch (err: unknown) {
    relayResult.value = err instanceof Error ? err.message : 'Relay failed'
  } finally {
    relayingTx.value = false
  }
}

async function handleRelayLightning() {
  if (!bolt11Input.value.trim() || !bolt11AmountInput.value) return
  relayingLn.value = true
  relayResult.value = ''
  try {
    const res = await mesh.relayLightning(bolt11Input.value.trim(), parseInt(bolt11AmountInput.value))
    relayResult.value = `Lightning relay queued (request #${res.request_id})`
    bolt11Input.value = ''
    bolt11AmountInput.value = ''
  } catch (err: unknown) {
    relayResult.value = err instanceof Error ? err.message : 'Relay failed'
  } finally {
    relayingLn.value = false
  }
}
</script>

<template>
  <div class="glass-card mesh-bitcoin-panel">
    <h3 class="mesh-panel-title">Off-Grid Bitcoin</h3>
    <p class="mesh-panel-sub">Relay transactions and receive block headers via mesh radio</p>

    <!-- Relay status notification -->
    <div v-if="relayResult" class="mesh-relay-result" :class="relayResult.includes('failed') || relayResult.includes('Failed') ? 'error' : 'success'">
      {{ relayResult }}
    </div>

    <!-- Block Headers -->
    <div class="mesh-bitcoin-section">
      <div class="mesh-bitcoin-section-header">
        <span class="mesh-bitcoin-label">Latest Block</span>
        <span v-if="mesh.latestBlockHeight > 0" class="mesh-bitcoin-height">#{{ mesh.latestBlockHeight.toLocaleString() }}</span>
        <span v-else class="mesh-bitcoin-height mesh-muted">No headers yet</span>
      </div>
      <div v-if="mesh.blockHeaders.length" class="mesh-block-list">
        <div v-for="h in mesh.blockHeaders.slice(0, 2)" :key="h.height" class="mesh-block-row">
          <span class="mesh-block-height">#{{ h.height.toLocaleString() }}</span>
          <span class="mesh-block-hash">{{ h.hash.slice(0, 12) }}...{{ h.hash.slice(-8) }}</span>
        </div>
      </div>

      <!-- Header send/receive toggles (issue #28) -->
      <div class="flex items-center justify-between gap-3 pt-3 mt-2 border-t border-white/10">
        <div class="min-w-0">
          <span class="mesh-bitcoin-label">Send headers</span>
          <small class="mesh-bitcoin-hint">Broadcast new block headers to mesh peers (needs internet)</small>
        </div>
        <ToggleSwitch :model-value="announceHeaders" :disabled="headersSaving" @update:model-value="setAnnounceHeaders" />
      </div>
      <div class="flex items-center justify-between gap-3 pt-3">
        <div class="min-w-0">
          <span class="mesh-bitcoin-label">Receive headers</span>
          <small class="mesh-bitcoin-hint">Accept block headers relayed by peers</small>
        </div>
        <ToggleSwitch :model-value="receiveHeaders" :disabled="headersSaving" @update:model-value="setReceiveHeaders" />
      </div>
    </div>

    <!-- On-Chain / Lightning tabs -->
    <div class="mesh-send-tabs">
      <button class="mesh-send-tab" :class="{ active: sendTab === 'onchain' }" @click="sendTab = 'onchain'">On-Chain</button>
      <button class="mesh-send-tab" :class="{ active: sendTab === 'lightning' }" @click="sendTab = 'lightning'">Lightning</button>
    </div>

    <!-- On-Chain tab -->
    <div v-if="sendTab === 'onchain'" class="mesh-bitcoin-section">
      <p class="mesh-bitcoin-hint">Creates a signed transaction locally and relays via mesh peers</p>
      <input v-model="meshSendAddr" class="mesh-bitcoin-input" placeholder="Bitcoin address (bc1...)" />
      <input v-model="meshSendAmount" class="mesh-bitcoin-input mesh-bitcoin-input-sm" type="number" placeholder="Amount (sats)" min="546" />
      <div class="mesh-relay-mode">
        <label class="mesh-relay-mode-option" :class="{ active: relayMode === 'archy' }">
          <input type="radio" v-model="relayMode" value="archy" />
          <span>Archy Peers <small>(E2E encrypted, direct)</small></span>
        </label>
        <label class="mesh-relay-mode-option" :class="{ active: relayMode === 'broadcast' }">
          <input type="radio" v-model="relayMode" value="broadcast" />
          <span>Mesh Broadcast <small>(multi-hop, wider reach)</small></span>
        </label>
      </div>
      <button class="glass-button" :disabled="!meshSendAddr.trim() || !meshSendAmount || relayingTx" @click="handleMeshSendBitcoin">
        {{ relayingTx ? 'Sending...' : 'Send via Mesh' }}
      </button>
      <details class="mesh-bitcoin-advanced">
        <summary class="mesh-bitcoin-label">Raw TX Relay</summary>
        <div style="margin-top: 8px;">
          <textarea v-model="txHexInput" class="mesh-bitcoin-input" placeholder="Paste raw transaction hex..." rows="3" />
          <button class="glass-button" :disabled="!txHexInput.trim() || relayingTx" @click="handleRelayTx">
            {{ relayingTx ? 'Relaying...' : 'Relay Raw TX' }}
          </button>
        </div>
      </details>
    </div>

    <!-- Lightning tab -->
    <div v-if="sendTab === 'lightning'" class="mesh-bitcoin-section">
      <p class="mesh-bitcoin-hint">Relays a Lightning invoice to an internet-connected peer for payment</p>
      <input v-model="bolt11Input" class="mesh-bitcoin-input" placeholder="lnbc... (bolt11 invoice)" />
      <input v-model="bolt11AmountInput" class="mesh-bitcoin-input mesh-bitcoin-input-sm" type="number" placeholder="Amount (sats)" />
      <div class="mesh-relay-mode">
        <label class="mesh-relay-mode-option" :class="{ active: relayMode === 'archy' }">
          <input type="radio" v-model="relayMode" value="archy" />
          <span>Archy Peers <small>(E2E encrypted, direct)</small></span>
        </label>
        <label class="mesh-relay-mode-option" :class="{ active: relayMode === 'broadcast' }">
          <input type="radio" v-model="relayMode" value="broadcast" />
          <span>Mesh Broadcast <small>(multi-hop, wider reach)</small></span>
        </label>
      </div>
      <button class="glass-button" :disabled="!bolt11Input.trim() || !bolt11AmountInput || relayingLn" @click="handleRelayLightning">
        {{ relayingLn ? 'Relaying...' : 'Pay via Mesh' }}
      </button>
    </div>
  </div>
</template>

<style>
.mesh-bitcoin-panel { padding: 18px; display: flex; flex-direction: column; gap: 14px; flex: 1; min-height: 0; overflow-y: auto; }
.mesh-panel-title { font-size: 1rem; font-weight: 700; color: rgba(255,255,255,0.95); margin: 0; }
.mesh-panel-sub { font-size: 0.78rem; color: rgba(255,255,255,0.45); margin: -6px 0 0; }
.mesh-bitcoin-section { display: flex; flex-direction: column; gap: 10px; }
.mesh-bitcoin-section-header { display: flex; justify-content: space-between; align-items: center; }
.mesh-bitcoin-label { font-size: 0.78rem; font-weight: 600; color: rgba(255,255,255,0.5); text-transform: uppercase; letter-spacing: 0.5px; }
.mesh-bitcoin-height { font-size: 0.85rem; font-weight: 700; color: #fb923c; font-family: monospace; }
.mesh-bitcoin-height.mesh-muted { color: rgba(255,255,255,0.3); font-weight: 400; }
.mesh-bitcoin-hint { font-size: 0.78rem; color: rgba(255,255,255,0.4); margin: 0; }
.mesh-bitcoin-input { width: 100%; background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); border-radius: 8px; color: rgba(255,255,255,0.9); padding: 10px 12px; font-size: 0.85rem; font-family: inherit; outline: none; box-sizing: border-box; }
.mesh-bitcoin-input:focus { border-color: rgba(251,146,60,0.4); }
.mesh-bitcoin-input::placeholder { color: rgba(255,255,255,0.25); }
.mesh-bitcoin-input-sm { padding: 8px 12px; font-size: 0.8rem; }
textarea.mesh-bitcoin-input { resize: vertical; min-height: 60px; }
select.mesh-bitcoin-input { cursor: pointer; }
.mesh-block-list { display: flex; flex-direction: column; gap: 4px; }
.mesh-block-row { display: flex; align-items: center; gap: 10px; padding: 6px 8px; background: rgba(255,255,255,0.04); border-radius: 6px; font-size: 0.78rem; }
.mesh-block-height { color: #fb923c; font-weight: 600; font-family: monospace; }
.mesh-block-hash { color: rgba(255,255,255,0.4); font-family: monospace; font-size: 0.72rem; }
.mesh-send-tabs { display: flex; gap: 2px; background: rgba(0,0,0,0.3); border-radius: 8px; padding: 3px; }
.mesh-send-tab { flex: 1; padding: 7px 10px; border: none; background: transparent; color: rgba(255,255,255,0.5); font-size: 0.8rem; font-weight: 500; border-radius: 6px; cursor: pointer; transition: all 0.2s; }
.mesh-send-tab:hover { color: rgba(255,255,255,0.8); background: rgba(255,255,255,0.05); }
.mesh-send-tab.active { color: #fff; background: rgba(255,255,255,0.1); }
.mesh-relay-mode { display: flex; gap: 6px; flex-wrap: wrap; }
.mesh-relay-mode-option { display: flex; align-items: center; gap: 6px; padding: 8px 12px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.1); cursor: pointer; font-size: 0.8rem; color: rgba(255,255,255,0.7); transition: all 0.2s; flex: 1; }
.mesh-relay-mode-option:hover { border-color: rgba(255,255,255,0.2); }
.mesh-relay-mode-option.active { border-color: rgba(251,146,60,0.4); background: rgba(251,146,60,0.08); color: rgba(255,255,255,0.9); }
.mesh-relay-mode-option small { color: rgba(255,255,255,0.4); }
.mesh-relay-mode-option input[type="radio"] { accent-color: #fb923c; }
.mesh-relay-result { padding: 10px 14px; border-radius: 8px; font-size: 0.8rem; }
.mesh-relay-result.success { background: rgba(74,222,128,0.1); border: 1px solid rgba(74,222,128,0.2); color: #4ade80; }
.mesh-relay-result.error { background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.2); color: #ef4444; }
.mesh-bitcoin-advanced { margin-top: 4px; }
.mesh-bitcoin-advanced summary { cursor: pointer; color: rgba(255,255,255,0.5); font-size: 0.8rem; }
</style>
