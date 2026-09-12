<template>
  <Transition name="identity-picker">
    <div
      v-if="show"
      class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 p-3 backdrop-blur-md sm:p-6"
      @click.self="$emit('cancel')"
    >

        <!-- Main panel -->
        <div
          ref="modalRef"
          @click.stop
          role="dialog"
          aria-modal="true"
          :aria-label="`Select identity for ${appName}`"
          class="identity-picker-card glass-card relative z-10 w-full max-w-md overflow-y-auto p-5 sm:p-6"
        >
          <div class="mb-5 flex items-center gap-3">
            <NostrIdentityOrb size="small" />
            <div class="min-w-0 flex-1">
              <p class="text-[10px] uppercase tracking-[0.2em] text-white/35">Nostr identity</p>
              <h2 class="mt-1 text-xl font-semibold text-white">Choose an identity</h2>
              <p class="mt-1 truncate text-xs text-white/45">{{ appName }}</p>
            </div>
            <button type="button" class="rounded-lg p-2 text-white/55 transition-colors hover:bg-white/10 hover:text-white" aria-label="Close" @click="$emit('cancel')">
              <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18 18 6M6 6l12 12" /></svg>
            </button>
          </div>

          <!-- Identity list -->
          <div class="space-y-2" role="radiogroup" aria-label="Available identities">
            <div v-if="loading" class="flex items-center justify-center py-8">
              <svg class="animate-spin h-6 w-6 text-white/40" viewBox="0 0 24 24" fill="none">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              <span class="ml-3 text-white/60 text-sm">Loading identities...</span>
            </div>

            <div v-else-if="loadError" class="text-center py-8">
              <p class="text-red-300/80 text-sm">Couldn&rsquo;t load identities.</p>
              <p class="text-white/35 text-xs mt-1">{{ loadError }}</p>
              <button type="button" class="glass-button mt-4 rounded-lg px-4 py-2 text-xs text-white/70" @click="loadIdentities">
                Try again
              </button>
            </div>

            <div v-else-if="userIdentities.length === 0" class="text-center py-8">
              <p class="text-white/50 text-sm">No identities found.</p>
              <p class="text-white/30 text-xs mt-1">Create one in Settings &rarr; Credentials</p>
            </div>

            <button
              v-for="identity in userIdentities"
              :key="identity.id"
              type="button"
              role="radio"
              :aria-checked="selectedId === identity.id"
              :aria-label="`Identity: ${identity.name}`"
              class="w-full text-left p-3 rounded-lg transition-all duration-200"
              :class="selectedId === identity.id
                ? 'bg-white/10 ring-1 ring-white/20'
                : 'bg-white/[0.03] hover:bg-white/[0.06]'"
              @click="selectedId = identity.id"
            >
              <div class="flex items-center gap-3">
                <div
                  class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0"
                  :class="avatarClasses(identity.purpose)"
                >
                  <span class="text-sm font-bold">{{ identity.name.charAt(0).toUpperCase() }}</span>
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-white font-semibold text-sm truncate">{{ identity.name }}</span>
                    <span v-if="identity.is_default" class="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-white/60">default</span>
                  </div>
                  <div class="mt-0.5">
                    <span v-if="identity.nostr_npub" class="text-white/35 text-xs font-mono truncate">{{ truncateNpub(identity.nostr_npub) }}</span>
                    <span v-else class="text-red-400/60 text-xs">No Nostr key</span>
                  </div>
                </div>
                <div class="shrink-0">
                  <div v-if="selectedId === identity.id" class="w-5 h-5 rounded-full bg-white/15 flex items-center justify-center">
                    <div class="w-2.5 h-2.5 rounded-full bg-white/70"></div>
                  </div>
                  <div v-else class="w-5 h-5 rounded-full bg-white/5"></div>
                </div>
              </div>
            </button>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 mt-4">
            <button @click="$emit('cancel')" class="glass-button flex-1 py-3 rounded-lg text-sm font-medium text-white/70">
              Cancel
            </button>
            <button
              @click="confirm"
              :disabled="!selectedId || !hasNostrKey"
              class="flex-1 py-3 rounded-lg text-sm font-semibold transition-all duration-200 disabled:opacity-30 disabled:cursor-not-allowed"
              :class="selectedId && hasNostrKey
                ? 'bg-white/10 text-white hover:bg-white/15'
                : 'bg-white/[0.03] text-white/40'"
            >
              Authenticate
            </button>
          </div>

          <p class="mt-3 text-center text-[10px] tracking-widest text-white/20">
            NIP-07 &middot; SECP256K1 &middot; Signed locally
          </p>
        </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import { rpcClient } from '@/api/rpc-client'
import NostrIdentityOrb from '@/components/NostrIdentityOrb.vue'

interface Identity {
  id: string
  name: string
  purpose: string
  pubkey: string
  did: string
  is_default: boolean
  nostr_pubkey?: string
  nostr_npub?: string
  is_node?: boolean
}

const props = defineProps<{
  show: boolean
  appName: string
}>()

const emit = defineEmits<{
  select: [identity: Identity]
  cancel: []
}>()

const modalRef = ref<HTMLElement | null>(null)
const identities = ref<Identity[]>([])
const selectedId = ref<string | null>(null)
const loading = ref(false)
const loadError = ref<string | null>(null)

// The node key authenticates the appliance itself (mesh/discovery and other
// platform operations), not the person's public profile. The API marks it
// explicitly; keep a defensive name/purpose fallback for older nodes that do
// not send is_node yet.
const userIdentities = computed(() => identities.value.filter(identity =>
  !identity.is_node && identity.name.trim().toLowerCase() !== 'node'
))

useModalKeyboard(modalRef, computed(() => props.show), () => emit('cancel'))

const hasNostrKey = computed(() => {
  const selected = userIdentities.value.find(i => i.id === selectedId.value)
  return selected?.nostr_pubkey != null
})

watch(() => props.show, async (open) => {
  if (open) await loadIdentities()
})

onMounted(() => {
  if (props.show) loadIdentities()
})

async function loadIdentities() {
  loading.value = true
  loadError.value = null
  try {
    const res = await rpcClient.call<{ identities: Identity[] }>({ method: 'identity.list' })
    identities.value = res.identities || []
    const defaultId = userIdentities.value.find(i => i.is_default && i.nostr_pubkey)
      || userIdentities.value.find(i => i.nostr_pubkey)
    if (defaultId) selectedId.value = defaultId.id
  } catch (error) {
    identities.value = []
    loadError.value = error instanceof Error
      ? error.message
      : 'Check the node connection and try again.'
  } finally {
    loading.value = false
  }
}

function confirm() {
  const selected = userIdentities.value.find(i => i.id === selectedId.value)
  if (selected) emit('select', selected)
}

function truncateNpub(npub: string): string {
  if (npub.length <= 20) return npub
  return npub.slice(0, 12) + '...' + npub.slice(-6)
}

function avatarClasses(purpose: string): string {
  switch (purpose) {
    case 'business': return 'bg-blue-500/15 text-blue-400'
    case 'anonymous': return 'bg-purple-500/15 text-purple-400'
    default: return 'bg-white/10 text-white/80'
  }
}
</script>

<style scoped>
.identity-picker-card { max-height: min(90%, 680px); }
/* ── Modal transitions ── */
.identity-picker-enter-active,
.identity-picker-leave-active {
  transition: opacity 0.4s ease;
}
.identity-picker-enter-active .identity-picker-card {
  transition: transform 0.5s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.4s ease;
}
.identity-picker-leave-active .identity-picker-card {
  transition: transform 0.25s ease, opacity 0.2s ease;
}
.identity-picker-enter-from { opacity: 0; }
.identity-picker-enter-from .identity-picker-card { transform: translateY(10px) scale(0.98); opacity: 0; }
.identity-picker-leave-to { opacity: 0; }
.identity-picker-leave-to .identity-picker-card { transform: translateY(10px) scale(0.98); opacity: 0; }
</style>
