<template>
  <main class="relative h-screen w-screen overflow-hidden bg-transparent" aria-label="Archipelago Nostr signer">
    <NostrIdentityPicker
      :show="showIdentityPicker"
      :app-name="appName"
      @select="onIdentitySelected"
      @cancel="cancelIdentitySelection"
    />
    <RentalPurchaseConsent v-if="!bridge.showConsent.value && !showIdentityPicker && !registrationBridge.request.value" :request="rentalBridge.request.value" :quote="rentalBridge.quote.value" :phase="rentalBridge.phase.value" :progress="rentalBridge.progress.value" :error="rentalBridge.error.value" @cancel-unpaid="rentalBridge.cancelUnpaid" @review="rentalBridge.review" @approve="rentalBridge.approve" @cancel="rentalBridge.cancel" />
      <MediaRegistrationConsent v-if="!bridge.showConsent.value && !showIdentityPicker"
      :request="registrationBridge.request.value" :phase="registrationBridge.phase.value"
      :error="registrationBridge.error.value" @approve="registrationBridge.approve" @resolve="registrationBridge.approveResolution" @cancel="registrationBridge.cancel" />
    <NostrSignConsent
      :show="bridge.showConsent.value"
      :app-name="bridge.consentRequest.value?.appName ?? appName"
      :method="bridge.consentRequest.value?.method ?? ''"
      :identity-label="bridge.consentRequest.value?.identityLabel"
      :event-kind="bridge.consentRequest.value?.eventKind"
      :content="bridge.consentRequest.value?.content"
      :phase="bridge.consentPhase.value"
      :error="bridge.consentError.value"
      @approve="bridge.approveConsent"
      @deny="bridge.denyConsent"
    />
  </main>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import NostrIdentityPicker from '@/components/NostrIdentityPicker.vue'
import NostrSignConsent from '@/components/NostrSignConsent.vue'
import MediaRegistrationConsent from '@/components/MediaRegistrationConsent.vue'
import RentalPurchaseConsent from '@/components/RentalPurchaseConsent.vue'
import { useMediaRegistrationBridge } from '@/composables/useMediaRegistrationBridge'
import { useRentalPurchaseBridge } from '@/composables/useRentalPurchaseBridge'
import { useNostrBridge } from '@/views/appSession/useNostrBridge'
import type { SelectedIdentity } from '@/views/appSession/useAppIdentity'

const appId = ref('')
const appName = ref('App')
const appOrigin = ref('')
const showIdentityPicker = ref(false)
const queuedRequests: MessageEvent[] = []
let hideTimer: ReturnType<typeof setTimeout> | null = null

function getStoredIdentity(): SelectedIdentity | null {
  if (!appId.value) return null
  try {
    const raw = localStorage.getItem(`archipelago_app_identity_${appId.value}`)
    return raw ? JSON.parse(raw) as SelectedIdentity : null
  } catch {
    return null
  }
}

function storeIdentity(identity: SelectedIdentity) {
  try { localStorage.setItem(`archipelago_app_identity_${appId.value}`, JSON.stringify(identity)) } catch {}
}

function parentPost(message: Record<string, unknown>) {
  window.parent.postMessage(message, appOrigin.value || '*')
}

function showSigner() {
  if (hideTimer !== null) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  parentPost({ type: 'archipelago:signer-show' })
}

function hideSigner(delay = 0) {
  if (hideTimer !== null) clearTimeout(hideTimer)
  const hide = () => {
    hideTimer = null
    if (!showIdentityPicker.value && !bridge.showConsent.value && !registrationBridge.request.value && !rentalBridge.request.value) {
      parentPost({ type: 'archipelago:signer-hide' })
    }
  }
  if (delay > 0) hideTimer = setTimeout(hide, delay)
  else hide()
}

function sendIdentity(identity: SelectedIdentity) {
  // The picker emits a Vue reactive object. Browser postMessage cannot clone
  // that proxy: sending it closes the picker, then throws before the broker
  // can hide, leaving a blank full-screen surface. Send public scalar fields
  // explicitly, also keeping any unrelated identity metadata out of the app.
  const publicIdentity: SelectedIdentity = {
    id: identity.id,
    name: identity.name,
    did: identity.did,
    pubkey: identity.pubkey,
    nostr_pubkey: identity.nostr_pubkey,
    nostr_npub: identity.nostr_npub,
  }
  parentPost({ type: 'archipelago:signer-identity', identity: publicIdentity })
}

const registrationBridge = useMediaRegistrationBridge({
  consentBusy: (): boolean => rentalBridge.isBusy(),
  appId: () => appId.value, appUrl: () => appOrigin.value, frameWindow: () => window.parent,
})
const rentalBridge = useRentalPurchaseBridge({
  consentBusy: (): boolean => bridge.showConsent.value || showIdentityPicker.value || registrationBridge.isBusy(),
  appId: () => appId.value, appUrl: () => appOrigin.value, frameWindow: () => window.parent,
})
watch(rentalBridge.request, request => { if (request) showSigner(); else hideSigner() })
watch(registrationBridge.request, request => { if (request) showSigner(); else hideSigner() })
watch([appId, appOrigin], () => { registrationBridge.cancel(); rentalBridge.cancel() }, { flush: 'sync' })
const bridge = useNostrBridge(getStoredIdentity, {
  appId: () => appId.value,
  appName: () => appName.value,
  appUrl: () => appOrigin.value,
  frameWindow: () => window.parent,
})

// Keep the app-side broker frame hidden for silent/remembered requests. It is
// a full-viewport iframe in tabs and Companion WebViews, so showing it for
// every background getPublicKey/getRelays call produces a visible flash even
// when no consent card opens. Reveal it only when there is actually something
// for the user to review, and keep it visible through signing/result.
watch(bridge.showConsent, (show) => {
  if (show) showSigner()
  else if (!showIdentityPicker.value) hideSigner()
})

function isSameNodeAppOrigin(origin: string): boolean {
  try {
    const candidate = new URL(origin)
    return (candidate.protocol === 'http:' || candidate.protocol === 'https:')
      && candidate.hostname === window.location.hostname
  } catch {
    return false
  }
}

async function onIdentitySelected(identity: SelectedIdentity) {
  storeIdentity(identity)
  showIdentityPicker.value = false
  sendIdentity(identity)
  const requests = queuedRequests.splice(0)
  if (requests.length) {
    await nextTick()
    for (const request of requests) await handleRequest(request)
  } else {
    // The host app normally follows the selected identity with getPublicKey
    // and signEvent. Keep one continuous signer surface through that chain so
    // the picker does not disappear and immediately flash back as consent.
    hideSigner(400)
  }
}

async function handleRequest(event: MessageEvent) {
  if (hideTimer !== null) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
  await bridge.handleNostrRequest(event)
  // getRelays and remembered approvals complete without opening the consent
  // card. Do not leave the otherwise-transparent broker intercepting the app.
  if (!bridge.showConsent.value && !showIdentityPicker.value) {
    hideSigner()
  }
}

function cancelIdentitySelection() {
  showIdentityPicker.value = false
  const requests = queuedRequests.splice(0)
  for (const request of requests) {
    const id = (request.data as { id?: unknown } | null)?.id
    window.parent.postMessage({ type: 'nostr-response', id, error: 'Identity selection cancelled' }, request.origin)
  }
  parentPost({ type: 'archipelago:signer-identity-cancelled' })
  hideSigner()
}

function onMessage(event: MessageEvent) {
  if (event.source !== window.parent) return
  const data = event.data as Record<string, unknown> | null
  if (!data) return

  if (data.type === 'archipelago:signer-init') {
    const id = typeof data.appId === 'string' ? data.appId : ''
    const name = typeof data.appName === 'string' ? data.appName : 'App'
    if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(id) || !isSameNodeAppOrigin(event.origin)) return
    appId.value = id
    appName.value = name.slice(0, 120)
    appOrigin.value = event.origin
    const stored = getStoredIdentity()
    if (stored) {
      sendIdentity(stored)
      hideSigner(400)
    } else {
      showIdentityPicker.value = true
      showSigner()
    }
    return
  }

  if (data.type === 'archipelago:signer-select-identity' && appId.value && event.origin === appOrigin.value) {
    showIdentityPicker.value = true
    showSigner()
    return
  }

  if (data.type === 'archipelago-rental-request' && appId.value && event.origin === appOrigin.value) {
    void rentalBridge.handle(event); return
  }
  if (data.type === 'archipelago-media-registration-request' && appId.value && event.origin === appOrigin.value) {
    void registrationBridge.handle(event)
    return
  }
  if (data.type !== 'nostr-request' || !appId.value || event.origin !== appOrigin.value) return
  if (!getStoredIdentity()) {
    queuedRequests.push(event)
    showIdentityPicker.value = true
    showSigner()
    return
  }
  void handleRequest(event)
}

onMounted(() => {
  // This route is rendered inside a full-viewport broker iframe. The global
  // dashboard stylesheet gives body a solid black canvas and animated
  // compositor layers; Android WebView can retain that last iframe surface
  // for a frame (or indefinitely) after the picker closes. Keep the broker's
  // document genuinely transparent so even a stale surface cannot cover the
  // app beneath it.
  document.documentElement.classList.add('nostr-signer-route')
  document.body.classList.add('nostr-signer-route')
  window.addEventListener('message', onMessage)
  window.parent.postMessage({ type: 'archipelago:signer-ready' }, '*')
})
onBeforeUnmount(() => {
  registrationBridge.dispose(); rentalBridge.dispose()
  if (hideTimer !== null) clearTimeout(hideTimer)
  window.removeEventListener('message', onMessage)
  document.documentElement.classList.remove('nostr-signer-route')
  document.body.classList.remove('nostr-signer-route')
})
</script>

<style>
html.nostr-signer-route,
html.nostr-signer-route body,
html.nostr-signer-route #app {
  background: transparent !important;
}

html.nostr-signer-route body::before,
html.nostr-signer-route body::after,
html.nostr-signer-route::before {
  content: none !important;
  animation: none !important;
  backdrop-filter: none !important;
  -webkit-backdrop-filter: none !important;
}
</style>
