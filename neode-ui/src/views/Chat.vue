<template>
  <div class="chat-fullscreen">
    <!-- Close button + connection indicator (desktop: top-right pill) -->
    <div class="chat-mode-pill hidden md:flex">
      <button class="chat-close-btn" :aria-label="t('chat.closeAssistant')" @click="closeChat">
        <svg class="w-4 h-4" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
        <span class="text-xs font-medium">{{ t('chat.close') }}</span>
      </button>
      <div
        v-if="aiuiConnected"
        class="w-2 h-2 rounded-full bg-green-400 ml-2 shadow-[0_0_6px_rgba(74,222,128,0.5)]"
        :title="t('chat.aiuiConnected')"
      />
    </div>

    <AIConnectionModal ref="connectionSetup" :show="showConnectionSetup" @close="showConnectionSetup = false" @configured="providerConfigured" />

    <!-- Loading indicator while iframe loads. pointer-events:none on the
         wrapper (see <style>) so this never blocks clicks reaching the
         iframe underneath even while shown; the bounded timeout below
         (see aiuiLoadTimedOut) additionally guarantees it disappears
         outright regardless of backend/handshake state, so it can never
         wedge the UI permanently. -->
    <Transition name="fade">
      <div v-if="aiuiUrl && !aiuiConnected && !aiuiLoadTimedOut" class="chat-loading" role="status" aria-live="polite">
        <div class="glass-card p-8 flex flex-col items-center gap-4">
          <div class="chat-loading-spinner" aria-hidden="true" />
          <p class="text-sm text-white/60">{{ t('chat.loadingAssistant') }}</p>
        </div>
      </div>
    </Transition>

    <!-- AIUI iframe — on mobile, leave room for close bar + tab bar at bottom.
         No `sandbox` attribute: it was considered and rejected for this
         phase (AIUI-04, 13-RESEARCH.md Open Question 2). `allow-scripts`
         together with `allow-same-origin` is the well-known escape pattern,
         and dropping `allow-same-origin` moves AIUI to an opaque origin,
         breaking its storage and its origin-checked postMessage bridge — a
         change bigger than this phase budgeted. The enforced boundary
         instead is the /aiui/-scoped Content-Security-Policy (nginx) plus
         the node-side rate limit (G-B3, 13-12); the residual risk (a
         browser that ignores or partially enforces CSP) is named in
         13-AI-SPEC.md §6, not silently assumed away. -->
    <iframe
      v-if="aiuiUrl"
      ref="aiuiFrame"
      :src="aiuiUrl"
      :title="t('chat.aiAssistant')"
      class="chat-iframe chat-iframe-mobile"
      allow="microphone"
      referrerpolicy="no-referrer"
      style="background: transparent"
    />

    <!-- Fallback when no AIUI URL configured -->
    <div v-else class="chat-placeholder">
      <div class="chat-placeholder-inner">
        <div class="chat-placeholder-icon">
          <svg class="w-8 h-8 text-white/40" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
          </svg>
        </div>
        <h2 class="text-2xl font-semibold text-white mb-2">{{ t('chat.aiAssistant') }}</h2>
        <p class="text-white/60 mb-4 leading-relaxed">
          {{ t('chat.notConfigured') }}
        </p>
        <p class="text-xs text-white/30">
          {{ t('chat.deployCta') }}
        </p>
      </div>
    </div>

    <!-- 13-08 (D-11): the destructive-tool confirmation dialog — trusted
         chrome, mounted as a SIBLING of the iframe, never inside it. The
         component Teleports to body with a full-screen backdrop, so it
         covers the whole viewport including the area over the iframe, and
         no ancestor transform can trap its position: fixed. Its text is
         node-authored, fetched by the ContextBroker over the page's own
         RPC session — nothing the iframe sends can open or resolve it. -->
    <ToolConfirmModal
      :show="!!toolConfirm"
      :description="toolConfirm?.description ?? ''"
      @approve="resolveToolConfirm(true)"
      @deny="resolveToolConfirm(false)"
      @dismiss="dismissToolConfirm"
    />

    <!-- A tool the operator asked for was blocked by an ungranted
         category. Trusted chrome, and Teleported to body for the same
         reason ToolConfirmModal is: a transformed ancestor would trap
         position:fixed. This only OFFERS the settings screen — it never
         changes a grant itself, so nothing the iframe or the model says
         can widen permissions. -->
    <Teleport to="body">
      <Transition name="fade">
        <div v-if="permissionNeeded.length" class="chat-permission-offer" role="status">
          <p class="text-sm text-white/85">
            {{ t('chat.permissionNeeded', { categories: permissionNeededLabels }) }}
          </p>
          <div class="flex items-center gap-2 shrink-0">
            <button class="chat-permission-btn" @click="openAISettings">
              {{ t('chat.openAISettings') }}
            </button>
            <button
              class="chat-permission-dismiss"
              :aria-label="t('common.dismiss')"
              @click="permissionNeeded = []"
            >
              <svg class="w-4 h-4" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ContextBroker } from '@/services/contextBroker'
import ToolConfirmModal from '@/components/ToolConfirmModal.vue'
import AIConnectionModal from '@/components/AIConnectionModal.vue'
import { AI_PERMISSION_CATEGORIES } from '@/stores/aiPermissions'
import { IS_DEMO } from '@/composables/useDemoIntro'

const { t } = useI18n()

const router = useRouter()
const route = useRoute()
const aiuiFrame = ref<HTMLIFrameElement | null>(null)
const aiuiConnected = ref(false)
const connectionSetup = ref<InstanceType<typeof AIConnectionModal> | null>(null)
const showConnectionSetup = ref(false)
function providerConfigured(provider: 'claude' | 'openai' | 'routstr' | 'auto' | 'local', model?: string) {
  if (!aiuiFrame.value?.contentWindow || !aiuiUrl.value) return
  aiuiFrame.value.contentWindow.postMessage({ type: 'ai:provider-configured', provider, model }, new URL(aiuiUrl.value, window.location.origin).origin)
}
onMounted(async () => { if (!IS_DEMO && await connectionSetup.value?.checkNeeded()) showConnectionSetup.value = true })
// Belt-and-suspenders backstop (2026-07-30 live-testing follow-up): the
// loading overlay must never be able to wedge the UI permanently regardless
// of AIUI/backend state — a broken handshake, a misconfigured origin, or the
// AI provider being unreachable must not leave the user staring at a
// spinner forever with no way to interact with the panel underneath. This
// timeout dismisses the overlay unconditionally after a bounded wait even if
// 'ready' never arrives; it does not affect aiuiConnected itself (the
// connection indicator dot still reflects the real state).
const AIUI_LOAD_TIMEOUT_MS = 8000
const aiuiLoadTimedOut = ref(false)
let loadTimeout: ReturnType<typeof setTimeout> | null = null
let broker: ContextBroker | null = null

// D-14 presentation flags (Phase 02 Plan 07, 02-AIUI-D14.md): AIUI opens
// already expanded (chatExpanded) and, on a mobile viewport, on its chat view
// rather than its context view (mobileChat). Both are static strings — never
// derived from a reactive viewport read, connection state, or timestamp — so
// the computed below has no reactive dependencies and its value never
// changes after first evaluation, which is exactly what keeps the iframe
// `src` byte-identical across re-renders, resizes, and deactivate/reactivate
// cycles (the load-bearing constraint that lets the AIUI panel survive a tab
// switch without reloading). AIUI decides how to apply mobileChat against its
// own viewport rather than neode-ui's, per 02-AIUI-D14.md's D-14b rationale.
const D14_FLAGS = 'chatExpanded=true&mobileChat=true'

const aiuiUrl = computed(() => {
  // Demo: ?mockArchy makes AIUI use its built-in mock node data (apps, system,
  // network, wallet, bitcoin, files) and &seed pre-loads the example chats.
  const demo = IS_DEMO ? '&mockArchy=1&seed=1' : ''
  const envUrl = import.meta.env.VITE_AIUI_URL
  if (envUrl) return `${envUrl}?embedded=true&hideClose=true&${D14_FLAGS}${demo}`
  if (import.meta.env.PROD || IS_DEMO) return `/aiui/?embedded=true&hideClose=true&${D14_FLAGS}${demo}`
  return ''
})

// ⌘K → "Talk to AIUI about it" hands the typed text over as `?ask=`.
//
// It is delivered by postMessage, NOT by adding a query param to `aiuiUrl`.
// That is deliberate: the comment on D14_FLAGS above explains that aiuiUrl must
// have no reactive dependencies so the iframe `src` stays byte-identical and
// AIUI survives a tab switch. Threading `ask` through the URL would rebuild the
// src on every question and reload AIUI, discarding the conversation — the
// exact opposite of what this feature is for.
//
// The ask is queued rather than sent directly, because the common case is
// arriving from ⌘K on a cold Chat tab where the iframe has not handshaked yet.
// `ready` flushes it.
const pendingAsk = ref('')

function flushAsk() {
  const text = pendingAsk.value
  if (!text || !aiuiConnected.value) return
  const frame = aiuiFrame.value
  if (!frame?.contentWindow || !aiuiUrl.value) return
  let targetOrigin: string
  try {
    targetOrigin = new URL(aiuiUrl.value, window.location.origin).origin
  } catch { return }
  frame.contentWindow.postMessage({ type: 'chat:prefill', text }, targetOrigin)
  pendingAsk.value = ''
  // Drop ask/askedAt from the URL so a refresh or a back-nav does not re-ask.
  const { ask: _a, askedAt: _t, ...rest } = route.query
  router.replace({ path: route.path, query: rest })
}

watch(
  () => route.query.askedAt,
  () => {
    const ask = route.query.ask
    if (!ask) return
    pendingAsk.value = String(ask)
    flushAsk()
  },
  { immediate: true },
)

function closeChat() {
  if (window.history.length > 1) {
    router.back()
  } else {
    router.push('/dashboard')
  }
}

// 13-08 (D-11): the pending destructive-tool confirmation the trusted
// chrome is currently showing. Set ONLY from the ContextBroker's
// aiui:tool-confirm-request CustomEvent, whose payload is node-fetched
// over the page's own RPC session — never from anything the iframe posts.
const toolConfirm = ref<{ reqId: string; description: string } | null>(null)

function onToolConfirmRequest(e: Event) {
  const detail = (e as CustomEvent).detail as { reqId?: string; description?: string }
  if (!detail?.reqId || typeof detail.description !== 'string') return
  toolConfirm.value = { reqId: detail.reqId, description: detail.description }
}

function resolveToolConfirm(approved: boolean) {
  const current = toolConfirm.value
  toolConfirm.value = null
  if (!current) return
  // The decision travels back to the broker (and from there to the node
  // over the authenticated RPC session) — never through the iframe.
  window.dispatchEvent(
    new CustomEvent('aiui:tool-confirm-response', {
      detail: { reqId: current.reqId, approved },
    }),
  )
}

function dismissToolConfirm() {
  // Closed without a decision: send nothing. The action stays pending on
  // the node until its own timeout declines it — never silently approved.
  toolConfirm.value = null
}

function onToolConfirmExpired(e: Event) {
  // 13-08 on-device UAT: the node no longer holds this pending action
  // (timed out, or resolved elsewhere) — close the dialog rather than
  // leave the human an Approve button whose click can only be refused.
  const detail = (e as CustomEvent).detail as { reqId?: string }
  if (toolConfirm.value && detail?.reqId === toolConfirm.value.reqId) {
    toolConfirm.value = null
  }
}

// A tool the operator's question needed was refused because its category
// is off. The node reports WHICH categories; we name them and offer the
// screen that owns the toggles. Never flips a toggle here — the operator
// decides, on the settings screen, in the trusted chrome.
const permissionNeeded = ref<string[]>([])

const permissionNeededLabels = computed(() =>
  permissionNeeded.value
    .map((id) => AI_PERMISSION_CATEGORIES.find((c) => c.id === id)?.label ?? id)
    .join(', '),
)

function onPermissionNeeded(e: Event) {
  const detail = (e as CustomEvent).detail as { categories?: unknown }
  const categories = Array.isArray(detail?.categories) ? detail.categories : []
  const known = categories.filter(
    (c): c is string => typeof c === 'string' && AI_PERMISSION_CATEGORIES.some((k) => k.id === c),
  )
  if (known.length) permissionNeeded.value = known
}

function openAISettings() {
  permissionNeeded.value = []
  router.push({ path: '/dashboard/settings', hash: '#ai-data-access' })
}

function onAiuiMessage(event: MessageEvent) {
  if (!aiuiUrl.value || event.source !== aiuiFrame.value?.contentWindow) return
  // Validate origin — only accept messages from AIUI
  try {
    const expected = new URL(aiuiUrl.value, window.location.origin).origin
    if (event.origin !== expected) return
  } catch { return }
  if (event.data?.type === 'ai:setup-request') {
    showConnectionSetup.value = true
    if (event.data.reason === 'funding') connectionSetup.value?.showRoutstr()
    return
  }
  // Listen for ready messages from AIUI iframe
  if (event.data?.type === 'ready') {
    aiuiConnected.value = true
    if (!IS_DEMO) void connectionSetup.value?.syncSelection()
    if (loadTimeout) { clearTimeout(loadTimeout); loadTimeout = null }
    // A ⌘K ask that arrived before the handshake is waiting — send it now.
    flushAsk()
  }
}

// The window message listener and the ContextBroker are only-while-visible:
// Chat is a main tab that survives a tab switch (KeepAlive), so both follow
// activation rather than mount. Idempotent — remove/stop any existing
// listener/broker before starting a new one, so two consecutive activations
// (Vue fires onActivated on first mount too) never double-arm either.
// `aiuiConnected` is set by a one-time 'ready' message from the iframe; once
// the iframe survives deactivation that message will not be re-sent on
// re-entry, so it must NOT be reset on deactivate.
function armChatLive() {
  window.removeEventListener('message', onAiuiMessage)
  window.addEventListener('message', onAiuiMessage)
  window.removeEventListener('aiui:tool-confirm-request', onToolConfirmRequest)
  window.addEventListener('aiui:tool-confirm-request', onToolConfirmRequest)
  window.removeEventListener('aiui:tool-confirm-expired', onToolConfirmExpired)
  window.addEventListener('aiui:tool-confirm-expired', onToolConfirmExpired)
  window.removeEventListener('aiui:permission-needed', onPermissionNeeded)
  window.addEventListener('aiui:permission-needed', onPermissionNeeded)
  broker?.stop()
  broker = null
  if (aiuiUrl.value) {
    broker = new ContextBroker(aiuiFrame, aiuiUrl.value)
    broker.start()
  }
  if (loadTimeout) clearTimeout(loadTimeout)
  loadTimeout = null
  if (aiuiUrl.value && !aiuiConnected.value) {
    loadTimeout = setTimeout(() => {
      aiuiLoadTimedOut.value = true
      loadTimeout = null
    }, AIUI_LOAD_TIMEOUT_MS)
  }
}
onActivated(() => armChatLive())

onDeactivated(() => {
  window.removeEventListener('message', onAiuiMessage)
  window.removeEventListener('aiui:tool-confirm-request', onToolConfirmRequest)
  window.removeEventListener('aiui:tool-confirm-expired', onToolConfirmExpired)
  window.removeEventListener('aiui:permission-needed', onPermissionNeeded)
  broker?.stop()
  broker = null
  if (loadTimeout) { clearTimeout(loadTimeout); loadTimeout = null }
})

// onActivated is a no-op outside a <KeepAlive> boundary — call it directly
// here too so a bare mount (a unit test, or any future non-KeepAlive usage)
// still gets the listener and the AIUI ContextBroker. Idempotent, so the
// redundant pass this causes on a KeepAlive-wrapped first mount is harmless.
onMounted(() => armChatLive())

onBeforeUnmount(() => {
  window.removeEventListener('message', onAiuiMessage)
  window.removeEventListener('aiui:tool-confirm-request', onToolConfirmRequest)
  window.removeEventListener('aiui:tool-confirm-expired', onToolConfirmExpired)
  window.removeEventListener('aiui:permission-needed', onPermissionNeeded)
  broker?.stop()
  broker = null
  if (loadTimeout) { clearTimeout(loadTimeout); loadTimeout = null }
})
</script>

<style scoped>

/* Teleported to body, so this is positioned against the viewport, not the
   chat panel. Sits above the iframe but below the confirm modal — a
   blocking decision must always win over a passive offer. */
.chat-permission-offer {
  position: fixed;
  left: 50%;
  transform: translateX(-50%);
  bottom: calc(1.25rem + var(--safe-bottom, 0px));
  z-index: 60;
  max-width: min(40rem, calc(100vw - 2rem));
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 0.875rem;
  border-radius: 0.875rem;
  background: rgba(24, 24, 27, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.12);
  backdrop-filter: blur(12px);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45);
}

.chat-permission-btn {
  padding: 0.375rem 0.75rem;
  border-radius: 0.5rem;
  font-size: 0.8125rem;
  font-weight: 500;
  white-space: nowrap;
  color: #fdba74;
  background: rgba(251, 146, 60, 0.14);
  border: 1px solid rgba(251, 146, 60, 0.3);
  transition: background 0.15s ease;
}

.chat-permission-btn:hover {
  background: rgba(251, 146, 60, 0.24);
}

.chat-permission-dismiss {
  padding: 0.375rem;
  border-radius: 0.5rem;
  color: rgba(255, 255, 255, 0.5);
  transition: color 0.15s ease, background 0.15s ease;
}

.chat-permission-dismiss:hover {
  color: rgba(255, 255, 255, 0.9);
  background: rgba(255, 255, 255, 0.08);
}

.chat-loading {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
  /* Never let the loading state itself block interaction with the iframe
     underneath — it has no interactive content of its own, so there is
     nothing here that needs to capture clicks. */
  pointer-events: none;
}

.chat-loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid rgba(255, 255, 255, 0.1);
  border-top-color: #fb923c;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

</style>
