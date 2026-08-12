<template>
  <div class="pb-6">
    <BackButton label="Back to Web5" @click="$router.push('/dashboard/web5')" />
    <div class="mb-8">
      <h1 class="text-3xl font-bold text-white">Credentials</h1>
      <p class="text-white/70">Issue, view, and verify W3C Verifiable Credentials</p>
    </div>

    <!-- Issue Credential Form -->
    <div class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-4">Issue New Credential</h2>
      <div class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="block text-sm text-white/70 mb-1">Issuer Identity <span class="text-red-400">*</span></label>
            <select v-model="issueForm.issuerId" class="credential-input w-full">
              <option value="" disabled>Select identity</option>
              <option v-for="id in identities" :key="id.id" :value="id.id">
                {{ id.name }} ({{ id.did?.slice(0, 24) }}...)
              </option>
            </select>
          </div>
          <div>
            <label class="block text-sm text-white/70 mb-1">Credential Type</label>
            <input v-model="issueForm.type" type="text" placeholder="e.g. NodeOperator" class="credential-input w-full" />
          </div>
        </div>
        <div>
          <label class="block text-sm text-white/70 mb-1">Subject DID <span class="text-red-400">*</span></label>
          <input v-model="issueForm.subjectDid" type="text" placeholder="did:key:z6Mk..." class="credential-input w-full font-mono text-sm" />
        </div>
        <div>
          <label class="block text-sm text-white/70 mb-1">Claims (JSON)</label>
          <textarea v-model="issueForm.claimsJson" rows="3" placeholder='{"role": "admin", "level": "full"}' class="credential-input w-full font-mono text-sm"></textarea>
        </div>
        <div>
          <label class="block text-sm text-white/70 mb-1">Expiration (optional)</label>
          <input v-model="issueForm.expiresAt" type="datetime-local" class="credential-input w-full" />
        </div>
        <button @click="issueCredential" :disabled="issuing || !issueForm.issuerId || !issueForm.subjectDid" class="glass-button px-6 py-2 text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed">
          {{ issuing ? 'Issuing...' : 'Issue Credential' }}
        </button>
      </div>
    </div>

    <!-- Credentials List -->
    <div class="glass-card p-6 mb-6">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-white">Your Credentials</h2>
        <button @click="loadCredentials" :disabled="loadingCreds" class="glass-button glass-button-sm px-3 py-1.5 text-xs">
          {{ loadingCreds ? 'Loading...' : 'Refresh' }}
        </button>
      </div>

      <div v-if="loadingCreds && credentials.length === 0" class="text-white/50 text-sm py-8 text-center">
        Loading credentials...
      </div>
      <div v-else-if="credentials.length === 0" class="text-white/50 text-sm py-8 text-center">
        No credentials yet. Issue one above or receive one from a peer.
      </div>
      <div v-else class="space-y-3">
        <div v-if="loadingCreds" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
          <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          Refreshing credentials...
        </div>
        <div
          v-for="cred in credentials"
          :key="cred.id"
          class="bg-black/20 rounded-xl border border-white/10 p-4 cursor-pointer hover:border-white/20 transition-colors"
          @click="selectedCredential = cred"
        >
          <div class="flex items-center justify-between mb-2">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-white">{{ credentialTypeLabel(cred) }}</span>
              <span
                class="px-2 py-0.5 rounded text-xs font-medium"
                :class="cred.status === 'revoked' ? 'bg-red-500/20 text-red-400' : 'bg-green-500/20 text-green-400'"
              >
                {{ cred.status }}
              </span>
            </div>
            <span class="text-xs text-white/40 font-mono">{{ cred.id?.slice(0, 20) }}...</span>
          </div>
          <div class="text-xs text-white/50 space-y-0.5">
            <p>Issuer: <span class="font-mono">{{ cred.issuer?.slice(0, 32) }}...</span></p>
            <p>Subject: <span class="font-mono">{{ cred.credentialSubject?.id?.slice(0, 32) }}...</span></p>
            <p>Issued: {{ formatDate(cred.issuanceDate) }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Verify Credential -->
    <div class="glass-card p-6 mb-6">
      <h2 class="text-lg font-semibold text-white mb-4">Verify Credential</h2>
      <div class="space-y-4">
        <div>
          <label class="block text-sm text-white/70 mb-1">Credential ID</label>
          <input v-model="verifyId" type="text" placeholder="urn:uuid:..." class="credential-input w-full font-mono text-sm" />
        </div>
        <div class="flex items-center gap-4">
          <button @click="verifyCredential" :disabled="verifying || !verifyId" class="glass-button px-6 py-2 text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed">
            {{ verifying ? 'Verifying...' : 'Verify' }}
          </button>
          <div v-if="verifyResult !== null" class="flex items-center gap-2">
            <div class="w-3 h-3 rounded-full" :class="verifyResult ? 'bg-green-400' : 'bg-red-400'"></div>
            <span class="text-sm" :class="verifyResult ? 'text-green-400' : 'text-red-400'">
              {{ verifyResult ? 'Valid' : 'Invalid' }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- Credential Detail Modal -->
    <Teleport to="body">
      <div v-if="selectedCredential" class="fixed inset-0 z-50 flex items-center justify-center p-4" @click.self="selectedCredential = null">
        <div class="fixed inset-0 bg-black/60 backdrop-blur-md"></div>
        <div class="relative glass-card p-6 max-w-2xl w-full max-h-[80vh] overflow-y-auto">
          <div class="flex items-center justify-between mb-4">
            <h3 class="text-lg font-semibold text-white">Credential Details</h3>
            <button @click="selectedCredential = null" class="text-white/60 hover:text-white text-xl">✕</button>
          </div>

          <div class="space-y-3 text-sm">
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Type</p>
              <p class="text-white font-medium">{{ credentialTypeLabel(selectedCredential) }}</p>
            </div>
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">ID</p>
              <p class="text-white/80 font-mono text-xs break-all">{{ selectedCredential.id }}</p>
            </div>
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Issuer</p>
              <p class="text-white/80 font-mono text-xs break-all">{{ selectedCredential.issuer }}</p>
            </div>
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Subject</p>
              <p class="text-white/80 font-mono text-xs break-all">{{ selectedCredential.credentialSubject?.id }}</p>
            </div>
            <div v-if="selectedCredential.credentialSubject" class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Claims</p>
              <pre class="text-white/80 font-mono text-xs whitespace-pre-wrap">{{ formatClaims(selectedCredential.credentialSubject) }}</pre>
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="bg-black/20 rounded-lg p-3">
                <p class="text-white/50 text-xs mb-1">Issued</p>
                <p class="text-white/80 text-xs">{{ formatDate(selectedCredential.issuanceDate) }}</p>
              </div>
              <div class="bg-black/20 rounded-lg p-3">
                <p class="text-white/50 text-xs mb-1">Expires</p>
                <p class="text-white/80 text-xs">{{ selectedCredential.expirationDate ? formatDate(selectedCredential.expirationDate) : 'Never' }}</p>
              </div>
            </div>
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Proof</p>
              <p class="text-white/80 text-xs">{{ selectedCredential.proof?.type }} — {{ selectedCredential.proof?.proofPurpose }}</p>
              <p class="text-white/60 font-mono text-xs mt-1 break-all">{{ selectedCredential.proof?.proofValue }}</p>
            </div>
            <div class="bg-black/20 rounded-lg p-3">
              <p class="text-white/50 text-xs mb-1">Status</p>
              <span
                class="px-2 py-0.5 rounded text-xs font-medium"
                :class="selectedCredential.status === 'revoked' ? 'bg-red-500/20 text-red-400' : 'bg-green-500/20 text-green-400'"
              >
                {{ selectedCredential.status }}
              </span>
            </div>
          </div>

          <div class="flex gap-3 mt-6">
            <button @click="copyCredentialJson" class="glass-button px-4 py-2 text-sm">
              {{ credCopied ? 'Copied!' : 'Copy JSON' }}
            </button>
            <button
              v-if="selectedCredential.status !== 'revoked'"
              @click="revokeSelected"
              :disabled="revoking"
              class="glass-button px-4 py-2 text-sm text-red-400 hover:text-red-300"
            >
              {{ revoking ? 'Revoking...' : 'Revoke' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Toast -->
    <div v-if="toast" class="fixed bottom-20 left-1/2 -translate-x-1/2 z-50 px-6 py-3 rounded-xl text-sm font-medium shadow-lg"
      :class="toast.type === 'error' ? 'bg-red-500/90 text-white' : 'bg-green-500/90 text-white'"
    >
      {{ toast.message }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import BackButton from '@/components/BackButton.vue'

interface Identity {
  id: string
  name: string
  did: string
  pubkey: string
}

interface Credential {
  '@context'?: string[]
  id: string
  type?: string[]
  issuer: string
  credentialSubject?: { id: string; [key: string]: unknown }
  issuanceDate?: string
  expirationDate?: string | null
  proof?: {
    type: string
    created: string
    verificationMethod: string
    proofPurpose: string
    proofValue: string
  }
  status: string
}

// Cached: revisits paint identities/credentials instantly and revalidate
// behind them (errors keep the last-known lists).
const identitiesRes = useCachedResource<Identity[]>({
  key: 'credentials.identities',
  fetcher: async (signal) => {
    const result = await rpcClient.call<{ identities: Identity[] }>({
      method: 'identity.list', params: {}, signal, dedup: true, maxRetries: 1,
    })
    return result?.identities || []
  },
  persist: false, // identity records — identity payload (T-02-01)
})
const credentialsRes = useCachedResource<Credential[]>({
  key: 'credentials.list',
  fetcher: async (signal) => {
    const result = await rpcClient.call<{ credentials: Credential[] }>({
      method: 'identity.list-credentials', params: {}, signal, dedup: true, maxRetries: 1,
    })
    return result?.credentials || []
  },
  persist: false, // credential material — identity payload (T-02-01)
})
const identities = computed(() => identitiesRes.data.value ?? [])
const credentials = computed(() => credentialsRes.data.value ?? [])
const loadingCreds = computed(() =>
  credentialsRes.loadState.value === 'loading' || credentialsRes.loadState.value === 'refreshing')
const selectedCredential = ref<Credential | null>(null)
const credCopied = ref(false)
const revoking = ref(false)

// Issue form
const issueForm = ref({
  issuerId: '',
  subjectDid: '',
  type: 'NodeOperator',
  claimsJson: '{}',
  expiresAt: '',
})
const issuing = ref(false)

// Verify
const verifyId = ref('')
const verifying = ref(false)
const verifyResult = ref<boolean | null>(null)

// Toast
const toast = ref<{ message: string; type: 'success' | 'error' } | null>(null)

function showToast(message: string, type: 'success' | 'error' = 'success') {
  toast.value = { message, type }
  setTimeout(() => { toast.value = null }, 3000)
}

function formatDate(dateStr?: string | null): string {
  if (!dateStr) return 'N/A'
  try {
    return new Date(dateStr).toLocaleString()
  } catch {
    return dateStr
  }
}

function credentialTypeLabel(cred: Credential): string {
  if (!cred.type || cred.type.length === 0) return 'Credential'
  // Return the most specific type (last non-VerifiableCredential type)
  const specific = cred.type.filter((t: string) => t !== 'VerifiableCredential')
  return specific.length > 0 ? (specific[specific.length - 1] ?? 'Credential') : 'VerifiableCredential'
}

function formatClaims(subject: Record<string, unknown>): string {
  const claims = { ...subject }
  delete claims.id
  return JSON.stringify(claims, null, 2)
}

async function loadCredentials() {
  await credentialsRes.refresh()
  if (credentialsRes.error.value) {
    showToast(`Failed to load credentials: ${credentialsRes.error.value}`, 'error')
  }
}

async function issueCredential() {
  if (!issueForm.value.issuerId || !issueForm.value.subjectDid) {
    showToast('Issuer identity and subject DID are required', 'error')
    return
  }

  let claims: Record<string, unknown>
  try {
    claims = JSON.parse(issueForm.value.claimsJson)
  } catch {
    showToast('Invalid JSON in claims field', 'error')
    return
  }

  issuing.value = true
  try {
    await rpcClient.call({
      method: 'identity.issue-credential',
      params: {
        issuer_id: issueForm.value.issuerId,
        subject_did: issueForm.value.subjectDid,
        type: issueForm.value.type || 'VerifiableCredential',
        claims,
        expires_at: issueForm.value.expiresAt || undefined,
      },
    })
    showToast('Credential issued successfully')
    issueForm.value.subjectDid = ''
    issueForm.value.claimsJson = '{}'
    issueForm.value.expiresAt = ''
    await loadCredentials()
  } catch (e) {
    showToast(`Failed to issue credential: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error')
  } finally {
    issuing.value = false
  }
}

async function verifyCredential() {
  if (!verifyId.value) {
    showToast('Enter a credential ID to verify', 'error')
    return
  }
  verifying.value = true
  verifyResult.value = null
  try {
    const result = await rpcClient.call<{ valid: boolean }>({
      method: 'identity.verify-credential',
      params: { id: verifyId.value },
    })
    verifyResult.value = result.valid
  } catch (e) {
    showToast(`Verification failed: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error')
    verifyResult.value = false
  } finally {
    verifying.value = false
  }
}

async function revokeSelected() {
  if (!selectedCredential.value) return
  revoking.value = true
  try {
    await rpcClient.call({
      method: 'identity.revoke-credential',
      params: { id: selectedCredential.value.id },
    })
    showToast('Credential revoked')
    selectedCredential.value = null
    await loadCredentials()
  } catch (e) {
    showToast(`Failed to revoke: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error')
  } finally {
    revoking.value = false
  }
}

async function copyCredentialJson() {
  if (!selectedCredential.value) return
  const json = JSON.stringify(selectedCredential.value, null, 2)
  try {
    await navigator.clipboard.writeText(json)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = json
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
  credCopied.value = true
  setTimeout(() => { credCopied.value = false }, 2000)
}

// Both resources fetch themselves on first use (skipping the fetch entirely
// when the cached value is fresh).

defineExpose({ credentials, loadCredentials })
</script>

<style scoped>
.credential-input {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 0.5rem;
  padding: 0.5rem 0.75rem;
  color: rgba(255, 255, 255, 0.9);
  font-size: 0.875rem;
  outline: none;
  transition: border-color 0.2s ease;
}
.credential-input:focus {
  border-color: rgba(255, 255, 255, 0.3);
}
.credential-input::placeholder {
  color: rgba(255, 255, 255, 0.3);
}
select.credential-input {
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='rgba(255,255,255,0.5)' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0.75rem center;
  padding-right: 2rem;
}
select.credential-input option {
  background: #1a1a2e;
  color: white;
}
textarea.credential-input {
  resize: vertical;
  min-height: 4rem;
}
</style>
