<template>
  <Teleport to="body">
    <Transition name="identity-picker">
      <div
        v-if="show"
        class="fixed inset-0 z-[3100] flex items-center justify-center p-4"
        @click="$emit('cancel')"
      >
        <!-- Backdrop — frosted blur -->
        <div class="absolute inset-0 bg-black/40 backdrop-blur-2xl"></div>

        <!-- Main panel -->
        <div
          ref="modalRef"
          @click.stop
          role="dialog"
          aria-modal="true"
          :aria-label="`Select identity for ${appName}`"
          class="relative z-10 w-full max-w-lg"
        >
          <!-- Header: screensaver-style glass disc + radial viz ring -->
          <div class="relative mb-6 flex flex-col items-center">
            <div class="nostr-hero">
              <!-- Radial viz segments — exact screensaver pattern, 48 bars, #FAFAFA -->
              <div class="nostr-viz-ring">
                <div
                  v-for="(_, i) in 48"
                  :key="i"
                  class="nostr-viz-segment"
                  :style="{ '--seg-i': i, '--seg-deg': `${(i / 48) * 360}deg` }"
                />
              </div>

              <!-- Glass disc — exact logo-gradient-border from screensaver -->
              <div class="nostr-glass-border">
                <div class="nostr-glass-inner">
                  <svg viewBox="0 0 122.88 88.39" width="42" height="30" xmlns="http://www.w3.org/2000/svg" class="nostr-cinema-svg">
                    <path fill="#FAFAFA" fill-rule="evenodd" clip-rule="evenodd" d="M87.51,21.16c5.26,1.45,10.79,1.84,16.58,1.18c1.42-0.16,2.81-0.35,4.16-0.53c6.46-0.84,11.86-1.32,13.78,3.52 c3.39,8.55-4.28,27.07-8.32,34.56c-8.32,15.43-24.9,32.69-44.08,27.57c-2.99-0.8-5.68-2.1-8.08-3.86 c6.3-3.51,11.28-8.9,15.13-15.24l-0.01,0.02c4.77,0.26,9.73,2.78,14.27,5.44c0.33-5.99-5.46-9.97-10.62-12.45 c4.14-9.29,6.33-19.72,7.01-29.03C87.53,29.46,87.64,25.53,87.51,21.16L87.51,21.16z M2.61,6.51c1.56-1.48,3.92-1.87,6.6-1.7 c5.03,0.31,10.23,1.86,15.11,3.18c10.61,2.86,20.99,1.93,31.1-2.74c1.36-0.63,2.69-1.28,3.98-1.9C65.56,0.37,70.8-1.9,74.31,2.3 c6.21,7.42,4.68,28.44,3.13,37.25c-3.2,18.15-14.03,40.87-34.88,42.1c-11.06,0.65-20.49-5.57-28.61-17.32 c-5.17-8-8.9-16.22-11.18-24.67C1.13,33.5-2.46,11.34,2.61,6.51L2.61,6.51z M12.94,34.3c-1.91-0.5-3.01-1.12-3.38-1.85 c-1.47-2.92,10.66-10.29,19.22-3.52C40.95,38.4,17.26,35.58,12.94,34.3L12.94,34.3z M32.63,62.79c-3.23-2.31-4.96-5.16-5.9-9.02 c10.67,5.4,20.66,5.01,29.96-2.42c-0.37,3.29-1.44,6.24-3.28,8.83C47.98,67.83,40.04,68.08,32.63,62.79L32.63,62.79z M67.07,30.06 c1.79-0.84,2.76-1.65,2.99-2.44c0.92-3.14-12.35-8.19-19.54,0.03C40.27,39.18,63.06,32.1,67.07,30.06L67.07,30.06z M90.82,42.07 c5.04-4.04,11.94-3.22,16.74,0.73c1.22,1.01,4.57,3.95,2.64,5.56c-0.53,0.44-1.41,0.69-2.63,0.75c-2.98,0.34-7.32-0.28-10.78-1.71 C94.07,46.3,92.01,44.83,90.82,42.07L90.82,42.07z"/>
                  </svg>
                </div>
              </div>
            </div>

            <h2 class="mt-5 text-lg font-semibold text-white">Select Identity</h2>
            <p class="mt-1 text-white/25 tracking-widest uppercase" style="font-size: 10px;">Nostr authentication protocol</p>
          </div>

          <!-- Identity list -->
          <div class="glass-card p-4 space-y-2 max-h-[50vh] overflow-y-auto" role="radiogroup" aria-label="Available identities">
            <div v-if="loading" class="flex items-center justify-center py-8">
              <svg class="animate-spin h-6 w-6 text-white/40" viewBox="0 0 24 24" fill="none">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              <span class="ml-3 text-white/60 text-sm">Loading identities...</span>
            </div>

            <div v-else-if="identities.length === 0" class="text-center py-8">
              <p class="text-white/50 text-sm">No identities found.</p>
              <p class="text-white/30 text-xs mt-1">Create one in Settings &rarr; Credentials</p>
            </div>

            <button
              v-for="identity in identities"
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

          <p class="mt-3 text-center text-[10px] text-white/20 tracking-widest">
            NIP-07 &middot; SECP256K1 &middot; Signed locally
          </p>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import { rpcClient } from '@/api/rpc-client'

interface Identity {
  id: string
  name: string
  purpose: string
  pubkey: string
  did: string
  is_default: boolean
  nostr_pubkey?: string
  nostr_npub?: string
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

useModalKeyboard(modalRef, computed(() => props.show), () => emit('cancel'))

const hasNostrKey = computed(() => {
  const selected = identities.value.find(i => i.id === selectedId.value)
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
  try {
    const res = await rpcClient.call<{ identities: Identity[] }>({ method: 'identity.list' })
    identities.value = res.identities || []
    const defaultId = identities.value.find(i => i.is_default && i.nostr_pubkey)
      || identities.value.find(i => i.nostr_pubkey)
    if (defaultId) selectedId.value = defaultId.id
  } catch {
    identities.value = []
  } finally {
    loading.value = false
  }
}

function confirm() {
  const selected = identities.value.find(i => i.id === selectedId.value)
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
/* ── Hero container ── */
.nostr-hero {
  position: relative;
  width: 148px;
  height: 148px;
}

/* ── Radial viz ring — exact screensaver pattern, #FAFAFA ── */
.nostr-viz-ring {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

.nostr-viz-segment {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 2.5px;
  height: 14px;
  margin-left: -1.25px;
  margin-top: -7px;
  background: linear-gradient(to bottom, rgba(250, 250, 250, 0.4), rgba(250, 250, 250, 0.06));
  border-radius: 1.5px;
  transform-origin: center center;
  transform: rotate(var(--seg-deg)) translateY(-60px);
  animation: seg-pulse 14s ease-in-out infinite;
  animation-delay: calc(var(--seg-i) * 0.02s);
}

/* Exact screensaver keyframes — 5 normal pulses then 1 strong expression, 14s total */
@keyframes seg-pulse {
  0%     { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  7.1%   { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1); }
  14.3%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  21.4%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1); }
  28.6%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  35.7%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1); }
  42.9%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  50%    { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1); }
  57.1%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  64.3%  { opacity: 0.7;  transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1); }
  71.4%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  78.6%  { opacity: 1;    transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1.5); }
  85.7%  { opacity: 1;    transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(1.5); }
  92.9%  { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
  100%   { opacity: 0.15; transform: rotate(var(--seg-deg)) translateY(-60px) scaleY(0.4); }
}

/* ── Glass disc — exact screensaver logo-gradient-border ── */
.nostr-glass-border {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 104px;
  height: 104px;
  border-radius: 9999px;
  padding: 3px;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.6) 0%, rgba(0, 0, 0, 0.8) 100%);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  filter: drop-shadow(0 0 24px rgba(255, 255, 255, 0.08));
}

.nostr-glass-inner {
  width: 100%;
  height: 100%;
  border-radius: 9999px;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* ── Cinema icon — breathing glow ── */
.nostr-cinema-svg {
  position: relative;
  z-index: 1;
  filter: drop-shadow(0 0 12px rgba(250, 250, 250, 0.12));
  animation: cinema-breathe 4s ease-in-out infinite;
}

@keyframes cinema-breathe {
  0%, 100% {
    opacity: 0.7;
    transform: scale(1);
    filter: drop-shadow(0 0 8px rgba(250, 250, 250, 0.08));
  }
  50% {
    opacity: 1;
    transform: scale(1.08);
    filter: drop-shadow(0 0 20px rgba(250, 250, 250, 0.22));
  }
}

/* ── Modal transitions ── */
.identity-picker-enter-active,
.identity-picker-leave-active {
  transition: opacity 0.4s ease;
}
.identity-picker-enter-active > .relative {
  transition: transform 0.5s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.4s ease;
}
.identity-picker-leave-active > .relative {
  transition: transform 0.25s ease, opacity 0.2s ease;
}
.identity-picker-enter-from { opacity: 0; }
.identity-picker-enter-from > .relative { transform: translateY(24px) scale(0.94); opacity: 0; }
.identity-picker-leave-to { opacity: 0; }
.identity-picker-leave-to > .relative { transform: translateY(10px) scale(0.98); opacity: 0; }
</style>
