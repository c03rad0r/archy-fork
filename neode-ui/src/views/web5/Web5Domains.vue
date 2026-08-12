<template>
  <!-- Bitcoin Domain Name Portfolio -->
  <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="glass-card p-6 flex flex-col md:w-1/2" style="--stagger-index: 0">
    <div class="flex items-start gap-4 mb-4 shrink-0">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 20l-5.447-2.724A1 1 0 013 16.382V5.618a1 1 0 011.447-.894L9 7m0 13l6-3m-6 3V7m6 10l4.553 2.276A1 1 0 0021 18.382V7.618a1 1 0 00-.553-.894L15 4m0 13V4m0 0L9 7" />
        </svg>
      </div>
      <div class="flex-1">
        <h2 class="text-xl font-semibold text-white mb-2">{{ t('web5.bitcoinDomains') }}</h2>
        <p class="text-white/70 text-sm mb-4">{{ t('web5.domainsSubtitle') }}</p>
      </div>
    </div>

    <div class="space-y-3 flex-1 min-h-0">
      <div v-if="domainsLoading && registeredNames.length > 0" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing domains...
      </div>
      <div v-else-if="domainsLoadError && registeredNames.length > 0" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ domainsLoadError }}
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 20l-5.447-2.724A1 1 0 013 16.382V5.618a1 1 0 011.447-.894L9 7m0 13l6-3m-6 3V7m6 10l4.553 2.276A1 1 0 0021 18.382V7.618a1 1 0 00-.553-.894L15 4m0 13V4m0 0L9 7" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.namesRegistered') }}</span>
        </div>
        <span class="text-white/60 text-sm">{{ registeredNames.length }} {{ registeredNames.length === 1 ? 'name' : 'names' }}</span>
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('common.status') }}</span>
        </div>
        <span :class="activeNamesCount > 0 ? 'text-green-400' : 'text-white/60'" class="text-sm font-medium">
          {{ activeNamesCount > 0 ? `${activeNamesCount} Active` : 'None' }}
        </span>
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.expiringSoon') }}</span>
        </div>
        <span class="text-white/60 text-sm">{{ expiringNamesCount }} {{ expiringNamesCount === 1 ? 'name' : 'names' }}</span>
      </div>
    </div>

    <button @click="showDomainsModal = true" class="mt-6 w-full px-4 py-2 glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors shrink-0">
      {{ t('web5.manageDomains') }}
    </button>
  </div>

  <!-- Domains Management Modal -->
  <Teleport to="body">
  <div v-if="showDomainsModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="showDomainsModal = false" @keydown.escape="showDomainsModal = false">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="domains-title">
      <div class="flex items-center justify-between mb-4">
        <h2 id="domains-title" class="text-lg font-bold text-white">{{ t('web5.domainsTitle') }}</h2>
        <button @click="showDomainsModal = false" class="text-white/40 hover:text-white/80 transition-colors">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
        </button>
      </div>

      <!-- Registered Names List -->
      <div v-if="registeredNames.length" class="space-y-2 mb-4">
        <div v-for="n in registeredNames" :key="n.id" class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
          <div>
            <div class="text-sm text-white font-medium font-mono">{{ n.nip05 }}</div>
            <div class="text-xs text-white/50 truncate max-w-[200px]">DID: {{ n.did }}</div>
          </div>
          <div class="flex items-center gap-2">
            <span :class="{
              'text-green-400': n.status === 'active',
              'text-yellow-400': n.status === 'pending',
              'text-red-400': n.status === 'expired' || n.status === 'failed'
            }" class="text-xs font-medium capitalize">{{ n.status }}</span>
            <button @click="removeName(n.id)" class="text-white/30 hover:text-red-400 transition-colors p-1">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
            </button>
          </div>
        </div>
      </div>
      <div v-else class="text-center text-white/40 text-sm py-4 mb-4">{{ t('web5.noDomains') }}</div>

      <!-- Register New Name -->
      <div class="border-t border-white/10 pt-4">
        <h3 class="text-sm font-semibold text-white mb-3">{{ t('web5.registerNewName') }}</h3>
        <div class="grid grid-cols-2 gap-3 mb-3">
          <div>
            <label class="text-white/60 text-xs block mb-1">Username</label>
            <input v-model="newDomainName" type="text" placeholder="satoshi" class="w-full input-glass" />
          </div>
          <div>
            <label class="text-white/60 text-xs block mb-1">Domain</label>
            <input v-model="newDomainDomain" type="text" placeholder="example.com" class="w-full input-glass" />
          </div>
        </div>
        <div class="mb-3">
          <label class="text-white/60 text-xs block mb-1">Link to Identity</label>
          <select v-model="newDomainIdentityId" class="w-full input-glass">
            <option value="" disabled>Select identity...</option>
            <option v-for="id in managedIdentities" :key="id.id" :value="id.id">{{ id.name }} ({{ (id.did || '').slice(0, 24) }}...)</option>
          </select>
        </div>
        <div v-if="domainError" class="text-xs text-red-400 mb-2">{{ domainError }}</div>
        <button @click="registerNewName" :disabled="domainRegistering || !newDomainName.trim() || !newDomainDomain.trim() || !newDomainIdentityId" class="w-full glass-button px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
          {{ domainRegistering ? 'Registering...' : 'Register Name' }}
        </button>
      </div>

      <!-- Verify NIP-05 -->
      <div class="border-t border-white/10 pt-4 mt-4">
        <h3 class="text-sm font-semibold text-white mb-3">{{ t('web5.verifyNip05') }}</h3>
        <div class="flex gap-2">
          <input v-model="verifyNip05Input" type="text" placeholder="user@domain.com" class="flex-1 input-glass" />
          <button @click="verifyNip05" :disabled="nip05Verifying || !verifyNip05Input.trim()" class="glass-button px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
            {{ nip05Verifying ? '...' : 'Verify' }}
          </button>
        </div>
        <div v-if="nip05Result" class="mt-2 p-3 bg-white/5 rounded-lg">
          <div class="flex items-center gap-2 mb-1">
            <div class="w-2 h-2 rounded-full" :class="nip05Result.verified ? 'bg-green-400' : 'bg-red-400'"></div>
            <span class="text-sm text-white font-medium">{{ nip05Result.verified ? 'Verified' : 'Not Found' }}</span>
          </div>
          <div v-if="nip05Result.nostr_pubkey" class="text-xs text-white/50 font-mono truncate">Pubkey: {{ nip05Result.nostr_pubkey }}</div>
        </div>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import type { RegisteredNameData, Nip05Result, ManagedIdentity } from './types'

const { t } = useI18n()

const props = defineProps<{
  showStagger: boolean
  managedIdentities: ManagedIdentity[]
}>()

const registeredNames = ref<RegisteredNameData[]>([])
const showDomainsModal = ref(false)
const newDomainName = ref('')
const newDomainDomain = ref('')
const newDomainIdentityId = ref('')
const domainError = ref('')
const domainRegistering = ref(false)
const domainsLoading = ref(false)
const domainsLoadError = ref('')
const verifyNip05Input = ref('')
const nip05Verifying = ref(false)
const nip05Result = ref<Nip05Result | null>(null)

const activeNamesCount = computed(() => registeredNames.value.filter(n => n.status === 'active').length)
const expiringNamesCount = computed(() => registeredNames.value.filter(n => n.status === 'expired' || n.expires_at).length)

async function loadDomainNames() {
  const hadNames = registeredNames.value.length > 0
  domainsLoading.value = true
  domainsLoadError.value = ''
  try {
    const res = await rpcClient.call<{ names: RegisteredNameData[] }>({ method: 'identity.list-names' })
    registeredNames.value = res.names || []
  } catch (e: unknown) {
    domainsLoadError.value = e instanceof Error ? e.message : 'Failed to load domains'
    if (!hadNames) registeredNames.value = []
  } finally {
    domainsLoading.value = false
  }
}

async function registerNewName() {
  if (!newDomainName.value.trim() || !newDomainDomain.value.trim() || !newDomainIdentityId.value) return
  domainRegistering.value = true
  domainError.value = ''
  try {
    const identity = props.managedIdentities.find(i => i.id === newDomainIdentityId.value)
    await rpcClient.call({ method: 'identity.register-name', params: {
      name: newDomainName.value.trim(),
      domain: newDomainDomain.value.trim(),
      identity_id: newDomainIdentityId.value,
      did: identity?.did || '',
    }})
    newDomainName.value = ''
    newDomainDomain.value = ''
    newDomainIdentityId.value = ''
    await loadDomainNames()
  } catch (e: unknown) {
    domainError.value = e instanceof Error ? e.message : t('web5.registrationFailed')
  } finally {
    domainRegistering.value = false
  }
}

async function removeName(id: string) {
  try {
    await rpcClient.call({ method: 'identity.remove-name', params: { id } })
    await loadDomainNames()
  } catch (e: unknown) {
    domainError.value = e instanceof Error ? e.message : t('web5.removeFailed')
  }
}

async function verifyNip05() {
  if (!verifyNip05Input.value.trim()) return
  nip05Verifying.value = true
  nip05Result.value = null
  try {
    const res = await rpcClient.call<Nip05Result>({ method: 'identity.resolve-name', params: { identifier: verifyNip05Input.value.trim() } })
    nip05Result.value = res
  } catch {
    nip05Result.value = { name: '', domain: '', nostr_pubkey: null, relays: [], verified: false }
  } finally {
    nip05Verifying.value = false
  }
}

// Expose for parent to call on mount
defineExpose({ loadDomainNames, registeredNames })
</script>
