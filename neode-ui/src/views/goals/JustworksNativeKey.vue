<template>
  <div class="space-y-3 rounded-xl border border-white/15 bg-white/5 p-4">
    <p class="text-sm text-white/70">Save the owner key from website creation once, as a separate business identity in Archipelago.</p>
    <p v-if="identity" class="text-sm text-white/80">Saved identity: {{ identity.name }}</p>
    <template v-else-if="importAvailable">
      <label class="block text-sm">Website owner nsec
        <input v-model="nsec" type="password" autocomplete="off" maxlength="100" :disabled="busy || disabled" class="mt-1 w-full rounded-lg bg-white/10 px-3 py-2" />
      </label>
      <p class="text-xs text-white/60">Copy the website owner nsec from Just Works. The key must match this website; your existing identities remain unchanged.</p>
    </template>
    <label v-if="identity || importAvailable" class="block text-sm">Archipelago node password
      <input v-model="password" type="password" autocomplete="current-password" :disabled="busy || disabled" class="mt-1 w-full rounded-lg bg-white/10 px-3 py-2" />
    </label>
    <label v-if="identity" class="flex items-start gap-2 text-sm text-white/80">
      <input v-model="approved" type="checkbox" :disabled="busy || disabled" class="mt-1" />
      Allow this request to send this identity’s private Nostr key to justworks.cash for the selected website action. This is key sharing, not local signing.
    </label>
    <p role="status" class="text-sm text-white/90">{{ status }}</p>
    <button v-if="identity || importAvailable" type="button" :disabled="busy || disabled || !npub" @click="identity ? useKey() : importKey()" class="glass-button rounded-lg px-4 py-2 text-sm disabled:opacity-50">{{ busy ? 'Working…' : identity ? 'Use saved key' : 'Add key to native signer' }}</button>
    <button v-if="loadFailed" type="button" @click="load" class="text-sm underline">Retry identity lookup</button>
  </div>
</template>
<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
const props = defineProps<{ siteId: string; npub: string; disabled?: boolean }>()
const emit = defineEmits<{ credentials: [credentials: {ownerKey: string}] }>()
interface Identity { id: string; name: string; nostr_npub?: string; is_node?: boolean }
const identity = ref<Identity | null>(null)
const nsec = ref(''), password = ref(''), approved = ref(false), busy = ref(false), status = ref(''), loadFailed = ref(false)
const importAvailable = ref(false)
let mounted = true
function clear() { nsec.value = ''; password.value = ''; approved.value = false }
async function load() {
  loadFailed.value = false
  try {
    const result = await rpcClient.call<{identities: Identity[]}>({method:'identity.list'})
    if (!mounted) return
    identity.value = result.identities.find(i => !i.is_node && i.nostr_npub === props.npub) || null
    if (!identity.value) {
      try {
        const capabilities = await rpcClient.call<{import_nostr?:boolean}>({method:'identity.capabilities', maxRetries:1})
        if (mounted) importAvailable.value = capabilities.import_nostr === true
      } catch { importAvailable.value = false }
      if (mounted && !importAvailable.value) status.value = 'This node needs a native signer update before it can import a website key. You can still use the owner key directly in the setup guide.'
    }
  } catch { if (mounted) { loadFailed.value = true; status.value = 'Could not load native identities. Retry before importing a key.' } }
}
async function importKey() {
  if (busy.value) return
  if (loadFailed.value) { status.value = 'Retry identity lookup before importing.'; return }
  if (!nsec.value.trim().startsWith('nsec1') || !password.value) { status.value = 'Enter the website owner nsec and your Archipelago node password.'; return }
  busy.value = true; status.value = 'Adding the matching website key to your native signer…'
  try {
    const result = await rpcClient.call<Identity>({method:'identity.import-nostr', params:{name:`Just Works: ${props.siteId}`.slice(0,100), nsec:nsec.value.trim(), expected_npub:props.npub, password:password.value}, maxRetries:1})
    if (!mounted) return
    if (result.nostr_npub !== props.npub) throw new Error('Identity mismatch')
    identity.value = result; status.value = 'Key saved in Archipelago. Authorize its use below to connect payments.'
  } catch { if (mounted) status.value = 'Could not import the key. Check the key and node password. Your node must support identity.import-nostr.' }
  finally { clear(); busy.value = false }
}
async function useKey() {
  if (busy.value || !identity.value) return
  if (!approved.value || !password.value) { status.value = 'Enter your node password and approve sharing this key with Just Works.'; return }
  busy.value = true; status.value = 'Unlocking the saved website key…'
  try {
    const result = await rpcClient.call<{nostr_npub?:string; nostr_nsec?:string}>({method:'identity.export-keys', params:{id:identity.value.id, password:password.value}, maxRetries:1})
    if (!mounted) return
    if (result.nostr_npub !== props.npub || !result.nostr_nsec?.startsWith('nsec1')) throw new Error('Identity mismatch')
    const ownerKey = result.nostr_nsec
    result.nostr_nsec = undefined
    emit('credentials', {ownerKey})
    status.value = 'Request sent. Check the publication status below.'
  } catch { if (mounted) status.value = 'Could not unlock the matching website key. Check your node password and retry.' }
  finally { clear(); busy.value = false }
}
onMounted(load)
onUnmounted(() => { mounted = false; clear() })
</script>
