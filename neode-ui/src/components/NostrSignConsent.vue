<template>
  <Transition name="consent">
    <div v-if="show" class="absolute inset-0 z-40 flex items-center justify-center bg-black/70 p-3 backdrop-blur-md sm:p-6" @click.self="phase === 'review' && deny()">
      <div ref="modalRef" class="nostr-consent-card glass-card w-full max-w-md overflow-y-auto p-5 sm:p-6" role="dialog" aria-modal="true" :aria-labelledby="`${dialogId}-title`" :aria-busy="phase === 'signing'">
        <template v-if="phase === 'signing' || phase === 'success'">
          <div class="flex min-h-[300px] flex-col items-center justify-center text-center">
            <NostrIdentityOrb size="medium" :state="phase === 'success' ? 'success' : 'loading'" :aria-label="phase === 'success' ? 'Nostr request approved' : 'Signing with Nostr identity'" />
            <h3 :id="`${dialogId}-title`" class="mt-4 text-xl font-semibold text-white">{{ phase === 'success' ? successTitle : progressTitle }}</h3>
            <p class="mt-2 text-sm text-white/55">{{ phase === 'success' ? 'The app received the approved result.' : 'Your key stays on this node.' }}</p>
            <p class="mt-4 text-[10px] uppercase tracking-[0.22em] text-white/25">NIP-07 · signed locally</p>
          </div>
        </template>

        <template v-else>
          <div class="mb-5 flex items-center gap-3">
            <NostrIdentityOrb size="small" :state="phase === 'error' ? 'idle' : 'loading'" />
            <div class="min-w-0 flex-1">
              <p class="text-[10px] uppercase tracking-[0.2em] text-white/35">Nostr identity</p>
              <h3 :id="`${dialogId}-title`" class="mt-1 text-xl font-semibold text-white">{{ phase === 'error' ? 'Request failed' : requestTitle }}</h3>
              <p class="mt-1 truncate text-xs text-white/45">{{ appName }}</p>
            </div>
            <button type="button" class="rounded-lg p-2 text-white/55 transition-colors hover:bg-white/10 hover:text-white" aria-label="Close" @click="deny">
              <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18 18 6M6 6l12 12" /></svg>
            </button>
          </div>

          <div v-if="phase === 'error'" class="alert-error mb-5 text-sm">{{ error || 'The node could not complete this request.' }}</div>
          <div v-else class="mb-5 space-y-2">
            <div class="rounded-xl border border-white/10 bg-black/20 p-3"><p class="mb-1 text-xs uppercase tracking-wider text-white/45">Request</p><p class="text-sm font-medium text-white">{{ methodLabel }}</p></div>
            <div v-if="identityLabel" class="rounded-xl border border-white/10 bg-black/20 p-3"><p class="mb-1 text-xs uppercase tracking-wider text-white/45">Identity</p><p class="text-sm font-medium text-white">{{ identityLabel }}</p></div>
            <div v-if="mediaApproval" class="space-y-2 rounded-xl border border-white/10 bg-black/20 p-3 text-sm">
              <p class="text-white/75">{{ mediaApproval.resolving ? 'Recover the completed registration or retire its expired incomplete request.' : 'Register this video for the selected IndeeHub project.' }}</p>
              <dl class="space-y-2"><div><dt class="text-xs text-white/45">Project</dt><dd class="break-all text-white">{{ mediaApproval.project }}</dd></div><div v-if="mediaApproval.file"><dt class="text-xs text-white/45">Cloud video</dt><dd class="break-all text-white">{{ mediaApproval.file }}</dd></div><div><dt class="text-xs text-white/45">Rental terms</dt><dd class="text-white">{{ mediaApproval.price }} sats · {{ mediaApproval.seconds }} seconds after first play</dd></div></dl>
              <details><summary class="min-h-11 cursor-pointer py-3 text-white/65">Full signed terms</summary><pre class="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs text-white/60">{{ content }}</pre></details>
            </div>
            <div v-else-if="contentPreview" class="rounded-xl border border-white/10 bg-black/20 p-3"><p class="mb-1 text-xs uppercase tracking-wider text-white/45">Content</p><p class="break-all font-mono text-sm text-white/75">{{ contentPreview }}</p></div>
            <div v-if="eventKind !== undefined" class="rounded-xl border border-white/10 bg-black/20 p-3"><p class="mb-1 text-xs uppercase tracking-wider text-white/45">Event kind</p><p class="text-sm font-medium text-white">{{ eventKind }} <span class="text-white/45">({{ eventKindLabel }})</span></p></div>
          </div>

          <label v-if="phase === 'review'" class="mb-5 flex cursor-pointer items-start gap-2">
            <input v-model="rememberChoice" type="checkbox" class="mt-0.5 h-4 w-4 rounded border-white/30 bg-white/10 text-orange-400 focus:ring-orange-400/50" />
            <span class="text-sm leading-snug text-white/65">Remember for this app, identity, and request type</span>
          </label>
          <div class="flex gap-3">
            <button type="button" class="glass-button flex-1 rounded-lg py-2.5 text-sm font-medium" @click="deny">{{ phase === 'error' ? 'Close' : 'Deny' }}</button>
            <button v-if="phase === 'review'" type="button" class="glass-button flex-1 rounded-lg border-orange-400/30 py-2.5 text-sm font-medium text-orange-300" @click="approve">Approve</button>
          </div>
        </template>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import NostrIdentityOrb from '@/components/NostrIdentityOrb.vue'

const EVENT_KIND_LABELS: Record<number, string> = {
  0: 'Metadata', 1: 'Short text note', 2: 'Recommend relay', 3: 'Contacts', 4: 'Encrypted DM',
  5: 'Event deletion', 6: 'Repost', 7: 'Reaction', 1618: 'Git pull request', 1619: 'Git pull request update',
  1621: 'Git issue', 9734: 'Zap request', 9735: 'Zap receipt', 10002: 'Relay list',
  27236: 'Local Cloud video registration', 27237: 'Recover or retire video registration', 30023: 'Long-form content', 30617: 'Git repository announcement',
}
const METHOD_LABELS: Record<string, string> = {
  getPublicKey: 'Share public identity', signEvent: 'Sign Nostr event',
  'nip04.encrypt': 'Encrypt a private message', 'nip04.decrypt': 'Decrypt a private message',
  'nip44.encrypt': 'Encrypt protected content', 'nip44.decrypt': 'Decrypt protected content',
}
const props = withDefaults(defineProps<{
  show: boolean; appName: string; method: string; identityLabel?: string; eventKind?: number; content?: string
  phase?: 'review' | 'signing' | 'success' | 'error'; error?: string
}>(), { phase: 'review', error: '' })
const emit = defineEmits<{ approve: [remember: boolean]; deny: [] }>()
const modalRef = ref<HTMLElement | null>(null)
const rememberChoice = ref(false)
const dialogId = `nostr-consent-${Math.random().toString(36).slice(2)}`
watch(() => props.show, show => { if (show) rememberChoice.value = false })
useModalKeyboard(modalRef, computed(() => props.show), () => {
  if (props.phase === 'review' || props.phase === 'error') emit('deny')
})
const methodLabel = computed(() => METHOD_LABELS[props.method] ?? props.method)
const requestTitle = computed(() => props.method === 'getPublicKey' ? 'Share this identity?' : 'Approve this request?')
const progressTitle = computed(() => props.method === 'getPublicKey' ? 'Sharing identity…' : 'Signing locally…')
const successTitle = computed(() => props.method === 'getPublicKey' ? 'Identity shared' : 'Request signed')
const mediaApproval = computed(() => {
  if (![27236, 27237].includes(props.eventKind ?? 0) || !props.content) return null
  try {
    const value = JSON.parse(props.content)
    if (!['archipelago.media-registration.approval.v1', 'archipelago.media-registration.resolution.v1'].includes(value.scope)
      || typeof value.intent?.projectId !== 'string' || (props.eventKind === 27236 && typeof value.selection?.cloudFile !== 'string')
      || !Number.isSafeInteger(value.intent.priceSats) || !Number.isSafeInteger(value.intent.viewingSeconds)) return null
    return { project: value.intent.projectId, file: value.selection?.cloudFile ?? '', resolving: props.eventKind === 27237,
      price: value.intent.priceSats, seconds: value.intent.viewingSeconds }
  } catch { return null }
})
const contentPreview = computed(() => !props.content ? '' : props.content.length > 200 ? `${props.content.slice(0, 200)}…` : props.content)
const eventKindLabel = computed(() => props.eventKind === undefined ? '' : EVENT_KIND_LABELS[props.eventKind] ?? 'Unknown')
function approve() { emit('approve', rememberChoice.value) }
function deny() { emit('deny') }
</script>

<style scoped>
.nostr-consent-card { max-height: min(90%, 680px); }
.consent-enter-active, .consent-leave-active { transition: opacity .22s ease; }
.consent-enter-active .nostr-consent-card, .consent-leave-active .nostr-consent-card { transition: transform .24s ease, opacity .2s ease; }
.consent-enter-from, .consent-leave-to { opacity: 0; }
.consent-enter-from .nostr-consent-card, .consent-leave-to .nostr-consent-card { opacity: 0; transform: translateY(10px) scale(.98); }
</style>
