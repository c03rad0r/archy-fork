<template>
  <Teleport to="body">
    <Transition name="app-launcher">
      <!-- z-[4000]: apps must open ABOVE any modal that launched them
           (BaseModal defaults to z-3000). At the old z-2400 an app opened
           from e.g. the Transactions modal loaded invisibly underneath it;
           now the existing enter animation plays over the modal, and closing
           the app returns you to the modal you came from. -->
      <div
        v-if="store.isOpen"
        class="fixed inset-0 z-[4000] flex items-stretch justify-stretch p-0"
        @click.self="store.close()"
      >
        <!-- Backdrop - blur like spotlight -->
        <div class="app-launcher-backdrop absolute inset-0 bg-black/75 backdrop-blur-md"></div>

        <!-- Panel - full-screen overlay -->
        <div
          class="app-launcher-panel relative z-10 flex flex-col overflow-hidden rounded-none shadow-2xl"
          :class="panelClasses"
          style="border-radius: 0;"
        >
          <!-- Header bar - sticky on mobile -->
          <div class="sticky top-0 z-10 flex items-center gap-3 border-b border-white/10 px-4 py-3 bg-black/60 backdrop-blur-md md:bg-transparent md:backdrop-blur-none">
            <div class="hidden md:flex items-center justify-center w-8 h-8 shrink-0 rounded cursor-grab hover:bg-white/10 transition-colors">
              <svg class="w-4 h-4 text-white/50" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 6h2v2H8V6zm0 5h2v2H8v-2zm0 5h2v2H8v-2zm5-10h2v2h-2V6zm0 5h2v2h-2v-2zm0 5h2v2h-2v-2z" />
              </svg>
            </div>
            <span class="flex-1 truncate text-sm font-medium text-white/90">{{ store.title || 'App' }}</span>
            <button
              type="button"
              class="flex items-center justify-center w-9 h-9 rounded-lg hover:bg-white/15 text-white/70 hover:text-white transition-colors disabled:opacity-70"
              aria-label="Refresh"
              :disabled="isRefreshing"
              @click="refreshIframe"
            >
              <svg
                class="w-5 h-5 transition-transform duration-300"
                :class="{ 'animate-spin': isRefreshing }"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
            <button
              type="button"
              class="flex items-center justify-center w-9 h-9 rounded-lg hover:bg-white/15 text-white/70 hover:text-white transition-colors"
              aria-label="Open in new tab"
              title="Open in new tab"
              @click="openInNewTab"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </button>
            <button
              ref="closeBtnRef"
              type="button"
              class="flex items-center justify-center w-9 h-9 rounded-lg hover:bg-white/15 text-white/70 hover:text-white transition-colors"
              aria-label="Close"
              @click="store.close()"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
            <kbd class="hidden sm:inline-flex px-2 py-1 text-xs text-white/50 bg-white/10 rounded">Esc</kbd>
          </div>

          <!-- Iframe container - overflow hidden to clip inner scrollbars -->
          <div class="relative flex-1 min-h-0 bg-black/40 overflow-hidden">
            <!-- Loading indicator -->
            <Transition name="content-fade">
              <AppLoadingScreen
                v-if="iframeLoading"
                :icon="overlayIcon"
                :title="store.title || 'App'"
                :progress="loadProgress"
              />
            </Transition>
            <iframe
              ref="iframeRef"
              v-if="store.url && !iframeBlocked"
              :key="iframeRefreshKey"
              :src="store.url"
              class="absolute inset-0 w-full h-full border-0 iframe-scrollbar-hide"
              title="App content"
              @load="onIframeLoad"
              @error="onIframeError"
            />

            <!-- Iframe blocked fallback -->
            <Transition name="content-fade">
              <div v-if="iframeBlocked && !iframeLoading" class="absolute inset-0 z-10 flex flex-col items-center justify-center">
                <div class="text-center px-8">
                  <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-white/5 border border-white/10 flex items-center justify-center">
                    <svg class="w-8 h-8 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 8V6a2 2 0 012-2h14a2 2 0 012 2v12a2 2 0 01-2 2H5a2 2 0 01-2-2v-2m0-8h18M3 8v8m18-8v8" />
                    </svg>
                  </div>
                  <h3 class="text-lg font-semibold text-white mb-2">Can't display in frame</h3>
                  <p class="text-white/50 text-sm mb-6">This app doesn't support embedded viewing.<br>Please open it in a new tab instead.</p>
                  <button
                    @click="openInNewTabAndClose"
                    class="glass-button px-6 py-3 rounded-lg text-sm font-semibold inline-flex items-center gap-2"
                  >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                    </svg>
                    Open in new tab
                  </button>
                </div>
              </div>
            </Transition>

            <!-- Payment Confirmation Dialog -->
            <Transition name="content-fade">
              <div v-if="completedPayment" class="absolute inset-0 z-20 flex items-center justify-center bg-black/70 backdrop-blur-sm">
                <div class="bg-black/80 border border-white/15 rounded-2xl p-6 w-full max-w-md mx-4 shadow-2xl max-h-[90%] overflow-y-auto">
                  <PaymentSuccessPane
                    :amount="completedPayment.amount"
                    :verb="completedPayment.verb"
                    :method-label="completedPayment.methodLabel"
                    :rows="completedPayment.rows"
                    :note="completedPayment.note"
                    @done="completedPayment = null"
                  />
                </div>
              </div>
            </Transition>

            <Transition name="content-fade">
              <div v-if="pendingPayment && !completedPayment" class="absolute inset-0 z-20 flex items-center justify-center bg-black/70 backdrop-blur-sm">
                <div class="bg-black/80 border border-white/15 rounded-2xl p-6 w-full max-w-sm mx-4 shadow-2xl">
                  <div class="flex items-center gap-3 mb-4">
                    <div class="w-10 h-10 rounded-lg bg-orange-500/20 flex items-center justify-center">
                      <svg class="w-5 h-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                      </svg>
                    </div>
                    <div>
                      <h3 class="text-white font-semibold text-sm">Payment Request</h3>
                      <p class="text-white/50 text-xs">{{ store.title || 'App' }} wants to make a payment</p>
                    </div>
                  </div>

                  <div class="space-y-2 mb-4">
                    <div class="flex justify-between items-center p-3 bg-white/5 rounded-lg">
                      <span class="text-white/60 text-sm">Amount</span>
                      <span class="text-orange-400 font-bold text-lg">{{ pendingPayment.amount_sats.toLocaleString() }} sats</span>
                    </div>
                    <div v-if="pendingPayment.memo" class="flex justify-between items-center p-3 bg-white/5 rounded-lg">
                      <span class="text-white/60 text-sm">Memo</span>
                      <span class="text-white/80 text-sm truncate ml-2">{{ pendingPayment.memo }}</span>
                    </div>
                    <div class="flex justify-between items-center p-3 bg-white/5 rounded-lg">
                      <span class="text-white/60 text-sm">Method</span>
                      <span class="text-white/80 text-sm capitalize">{{ pendingPayment.method || 'auto' }}</span>
                    </div>
                  </div>

                  <div v-if="paymentError" class="mb-3 alert-error">
                    <p class="text-xs">{{ paymentError }}</p>
                  </div>

                  <div class="flex gap-3">
                    <button @click="rejectPayment" class="flex-1 px-4 py-2.5 bg-white/5 border border-white/10 rounded-lg text-sm text-white/70 hover:bg-white/10 transition-colors">
                      Deny
                    </button>
                    <button @click="approvePayment" :disabled="paymentProcessing" class="flex-1 px-4 py-2.5 glass-button glass-button-warning rounded-lg text-sm font-medium disabled:opacity-50">
                      {{ paymentProcessing ? 'Paying...' : 'Approve' }}
                    </button>
                  </div>
                </div>
              </div>
            </Transition>

            <NostrSignConsent
              :show="store.showConsent"
              :app-name="store.consentRequest?.appName ?? ''"
              :method="store.consentRequest?.method ?? ''"
              :identity-label="store.consentRequest?.identityLabel"
              :event-kind="store.consentRequest?.eventKind"
              :content="store.consentRequest?.content"
              :phase="store.consentPhase"
              :error="store.consentError"
              @approve="store.approveConsent"
              @deny="store.denyConsent"
            />
            <NostrIdentityPicker
              :show="showIdentityPicker"
              :app-name="store.title || 'App'"
              @select="onIdentitySelected"
              @cancel="cancelIdentitySelection"
            />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useAppLauncherStore } from '@/stores/appLauncher'
import NostrSignConsent from '@/components/NostrSignConsent.vue'
import NostrIdentityPicker from '@/components/NostrIdentityPicker.vue'
import AppLoadingScreen from '@/components/AppLoadingScreen.vue'
import PaymentSuccessPane, { type SuccessRow } from '@/components/PaymentSuccessPane.vue'
import { DEFAULT_APP_ICON } from '@/views/apps/appsConfig'
import { rpcClient } from '@/api/rpc-client'
import { useLightningRequired } from '@/composables/useLightningRequired'

const lightning = useLightningRequired()

interface PaymentRequest {
  request_id: string
  amount_sats: number
  memo?: string
  method?: 'lightning' | 'ecash' | 'onchain' | 'auto'
  invoice?: string
  address?: string
}

const store = useAppLauncherStore()
const closeBtnRef = ref<HTMLButtonElement | null>(null)
const iframeRef = ref<HTMLIFrameElement | null>(null)
watch(iframeRef, frame => store.setNostrFrame(frame?.contentWindow || null), { flush: 'post' })
const iframeRefreshKey = ref(0)
const isRefreshing = ref(false)
const iframeLoading = ref(true)
const iframeBlocked = ref(false)

// Best-guess icon for the loading screen — resolved from the /app/{id}/ path
// when present; AppLoadingScreen's <img> falls back to the default icon if the
// guessed asset 404s.
const overlayIcon = computed(() => {
  const url = store.url
  if (!url) return DEFAULT_APP_ICON
  try {
    const m = new URL(url, window.location.origin).pathname.match(/^\/app\/([a-z0-9._-]+)/i)
    if (m?.[1]) return `/assets/img/app-icons/${m[1].toLowerCase()}.png`
  } catch { /* not a parseable URL */ }
  return DEFAULT_APP_ICON
})

// Faux load progress (cross-origin iframes give no real progress events): ease
// toward ~92% while loading, snap to 100% on load.
const loadProgress = ref(0)
let progressTimer: ReturnType<typeof setInterval> | null = null
function stopProgress() {
  if (progressTimer) { clearInterval(progressTimer); progressTimer = null }
}
function startProgress() {
  stopProgress()
  loadProgress.value = 8
  progressTimer = setInterval(() => {
    loadProgress.value += Math.max(0.4, (92 - loadProgress.value) * 0.08)
    if (loadProgress.value >= 92) { loadProgress.value = 92; stopProgress() }
  }, 180)
}
watch(iframeLoading, (loading) => {
  if (loading) startProgress()
  else { stopProgress(); loadProgress.value = 100 }
}, { immediate: true })

// Nostr identity picker state
const showIdentityPicker = ref(false)
const IDENTITY_STORAGE_KEY = 'archipelago_app_identity_'

interface SelectedIdentity {
  id: string
  name: string
  did: string
  pubkey: string
  nostr_pubkey?: string
  nostr_npub?: string
}

/** Get the stored identity for an app, or null if first launch */
function getStoredIdentity(appUrl: string): SelectedIdentity | null {
  try {
    const key = IDENTITY_STORAGE_KEY + appUrl.replace(/[^a-z0-9]/gi, '_')
    const stored = localStorage.getItem(key)
    return stored ? JSON.parse(stored) as SelectedIdentity : null
  } catch {
    return null
  }
}

/** Store the selected identity for an app */
function storeIdentity(appUrl: string, identity: SelectedIdentity) {
  try {
    const key = IDENTITY_STORAGE_KEY + appUrl.replace(/[^a-z0-9]/gi, '_')
    localStorage.setItem(key, JSON.stringify(identity))
  } catch { /* ignore */ }
}

/** Handle identity selection from the picker */
function onIdentitySelected(identity: SelectedIdentity) {
  showIdentityPicker.value = false
  if (store.url) {
    storeIdentity(store.url, identity)
  }
  // Send identity to the iframe
  sendSelectedIdentity(identity)
}

function cancelIdentitySelection() {
  showIdentityPicker.value = false
  iframeRef.value?.contentWindow?.postMessage({ type: 'archipelago:identity-cancelled' }, '*')
}

/** Send a specific identity to the iframe */
async function sendSelectedIdentity(identity: SelectedIdentity) {
  try {
    const challenge = `archipelago-identity:${Date.now()}`
    const sigRes = await rpcClient.call<{ signature: string }>({
      method: 'identity.sign',
      params: { id: identity.id, message: challenge }
    })
    const iframe = iframeRef.value
    if (!iframe?.contentWindow) return
    iframe.contentWindow.postMessage({
      type: 'archipelago:identity',
      did: identity.did,
      name: identity.name,
      pubkey: identity.pubkey,
      nostr_pubkey: identity.nostr_pubkey || null,
      nostr_npub: identity.nostr_npub || null,
      challenge,
      signature: sigRes.signature
    }, '*')
  } catch {
    /* identity signing not available */
  }
}

// Timers for iframe load detection
let loadTimeoutId: ReturnType<typeof setTimeout> | null = null
let contentCheckId: ReturnType<typeof setTimeout> | null = null

function clearTimers() {
  if (loadTimeoutId) { clearTimeout(loadTimeoutId); loadTimeoutId = null }
  if (contentCheckId) { clearTimeout(contentCheckId); contentCheckId = null }
}

// Wallet connect — payment request state
const pendingPayment = ref<PaymentRequest | null>(null)
const paymentProcessing = ref(false)
const paymentError = ref('')
const completedPayment = ref<{
  amount: number
  verb: string
  methodLabel: string
  rows: SuccessRow[]
  note?: string
} | null>(null)
const paymentOrigin = ref('')

function refreshIframe() {
  isRefreshing.value = true
  iframeLoading.value = true
  iframeBlocked.value = false
  clearTimers()
  iframeRefreshKey.value++
  loadTimeoutId = setTimeout(() => {
    if (iframeLoading.value) {
      iframeLoading.value = false
      iframeBlocked.value = true
    }
  }, 15000)
}

function openInNewTab() {
  if (!store.url) return
  // Inside the Archipelago companion app, open the app in the in-app WebView
  // instead of window.open — which the WebView suppresses for noopener popups
  // (so the tap silently no-ops). The native bridge is reliable; fall back to
  // window.open in a plain mobile browser.
  const native = (window as any).ArchipelagoNative
  if (native && typeof native.openInAppEx === 'function' && store.title) {
    native.openInAppEx(store.url, '', store.title)
    return
  }
  if (native && typeof native.openInApp === 'function') {
    native.openInApp(store.url)
    return
  }
  window.open(store.url, '_blank', 'noopener,noreferrer')
}

function openInNewTabAndClose() {
  openInNewTab()
  store.close()
}

function onIframeLoad() {
  injectScrollbarHideIfSameOrigin()
  isRefreshing.value = false
  iframeLoading.value = false
  sendIdentityIfSupported()

  // Clear the load timeout
  if (loadTimeoutId) { clearTimeout(loadTimeoutId); loadTimeoutId = null }

  // Check iframe content after a brief delay to let the app render
  contentCheckId = setTimeout(checkIframeContent, 2000)
}

function onIframeError() {
  clearTimers()
  iframeLoading.value = false
  iframeBlocked.value = true
}

/** Check if the iframe loaded meaningful content (same-origin only) */
function checkIframeContent() {
  try {
    const iframe = iframeRef.value
    if (!iframe) return
    const doc = iframe.contentDocument
    if (!doc) return // Cross-origin — can't check, assume OK
    const body = doc.body
    if (!body || (body.children.length === 0 && body.innerText.trim() === '')) {
      iframeBlocked.value = true
    }
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Cross-origin: can\'t access iframe, assume working', e)
  }
}

/** Apps that support the Archipelago identity protocol (postMessage) */
function isIdentityAwareApp(url: string): boolean {
  return url.includes('indeehub') || url.includes('indeedhub')
}

/** Send the user's identity to the iframe via postMessage.
 *  On first launch, shows the identity picker modal.
 *  On subsequent launches, uses the previously selected identity. */
async function sendIdentityIfSupported() {
  if (!store.url || !isIdentityAwareApp(store.url)) return

  // Check if we have a stored identity for this app
  const stored = getStoredIdentity(store.url)
  if (stored) {
    // Use the previously selected identity
    await sendSelectedIdentity(stored)
    return
  }

  // First launch — show the identity picker
  showIdentityPicker.value = true
  return // Identity will be sent after selection via onIdentitySelected

}

function injectScrollbarHideIfSameOrigin() {
  try {
    const doc = iframeRef.value?.contentDocument
    if (!doc) return
    const style = doc.createElement('style')
    style.textContent = `
      * { -ms-overflow-style: none; scrollbar-width: none; }
      *::-webkit-scrollbar { display: none; }
    `
    doc.head.appendChild(style)
    // Escape from inside iframe → close overlay and return focus to launcher
    doc.addEventListener('keydown', (e) => {
      if ((e as KeyboardEvent).key === 'Escape') {
        e.preventDefault()
        window.parent.postMessage({ type: 'app-launcher-escape' }, '*')
      }
    })
  } catch {
    /* Cross-origin: cannot access iframe document */
  }
}

const panelClasses = [
  'glass-card',
  'w-full h-full',
  'max-w-none max-h-none',
]

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape' && store.isOpen) {
    store.close()
    e.preventDefault()
    e.stopPropagation()
  }
}

function onMessage(e: MessageEvent) {
  if (e.data?.type === 'app-launcher-escape' && store.isOpen) {
    store.close()
  }
  // Iframe app requests identity on demand
  if (e.data?.type === 'archipelago:identity:request' && store.isOpen) {
    if (e.data?.force === true) showIdentityPicker.value = true
    else sendIdentityIfSupported()
  }
  // Wallet connect — app requests a payment
  if (e.data?.type === 'archipelago:payment-request' && store.isOpen) {
    handlePaymentRequest(e)
  }
}

/** Handle incoming payment request from iframe app */
function handlePaymentRequest(e: MessageEvent) {
  const data = e.data
  if (!data.amount_sats || typeof data.amount_sats !== 'number' || data.amount_sats <= 0) {
    sendPaymentResponse(e.origin, data.request_id, false, 'Invalid amount')
    return
  }
  pendingPayment.value = {
    request_id: data.request_id || `pay-${Date.now()}`,
    amount_sats: data.amount_sats,
    memo: data.memo,
    method: data.method || 'auto',
    invoice: data.invoice,
    address: data.address,
  }
  paymentOrigin.value = e.origin
  completedPayment.value = null
  paymentError.value = ''
  paymentProcessing.value = false
}

/** Send payment response back to the iframe */
function sendPaymentResponse(origin: string, requestId: string, success: boolean, error?: string, receipt?: Record<string, unknown>) {
  const iframe = iframeRef.value
  if (!iframe?.contentWindow) return
  iframe.contentWindow.postMessage({
    type: 'archipelago:payment-response',
    request_id: requestId,
    success,
    error: error || null,
    receipt: receipt || null,
  }, origin || '*')
}

/** User approves the payment */
async function approvePayment() {
  if (!pendingPayment.value || paymentProcessing.value) return
  paymentProcessing.value = true
  paymentError.value = ''

  const pay = pendingPayment.value
  const method = resolvePaymentMethod(pay)

  try {
    let receipt: Record<string, unknown> = {}

    if (method === 'ecash') {
      const res = await rpcClient.call<{ token: string; amount_sats: number }>({
        method: 'wallet.ecash-send',
        params: { amount_sats: pay.amount_sats },
      })
      receipt = { method: 'ecash', token: res.token, amount_sats: res.amount_sats }
    } else if (method === 'lightning') {
      // Both arms below need a Lightning node — paying an invoice and minting
      // one. With none installed, raise the install modal instead of failing.
      if (!(await lightning.requireLightningReady('send'))) return
      if (pay.invoice) {
        // Tracked to a real terminal state — slow routing is not a failure.
        const res = await rpcClient.payLightningInvoice({ payment_request: pay.invoice })
        if (res.status === 'failed') throw new Error(res.failure_reason || 'Payment failed')
        if (res.status === 'pending') throw new Error('Payment is still settling — check your wallet transactions before retrying.')
        receipt = { method: 'lightning', payment_hash: res.payment_hash, amount_sats: res.amount_sats }
      } else {
        // Create and immediately return an invoice for the requester to display
        const res = await rpcClient.call<{ payment_request: string }>({
          method: 'lnd.createinvoice',
          params: { amount_sats: pay.amount_sats, memo: pay.memo || '' },
        })
        receipt = { method: 'lightning', payment_request: res.payment_request, amount_sats: pay.amount_sats }
      }
    } else {
      if (!pay.address) {
        paymentError.value = 'No Bitcoin address provided for on-chain payment'
        return
      }
      const res = await rpcClient.call<{ txid: string }>({
        method: 'lnd.sendcoins',
        params: { addr: pay.address, amount: pay.amount_sats },
      })
      receipt = { method: 'onchain', txid: res.txid, amount_sats: pay.amount_sats }
    }

    sendPaymentResponse(paymentOrigin.value, pay.request_id, true, undefined, receipt)
    const rows: SuccessRow[] = []
    if (typeof receipt.payment_hash === 'string' && receipt.payment_hash) {
      rows.push({ label: 'Payment hash', value: receipt.payment_hash })
    }
    if (typeof receipt.txid === 'string' && receipt.txid) {
      rows.push({ label: 'Transaction ID', value: receipt.txid })
    }
    if (typeof receipt.token === 'string' && receipt.token) {
      rows.push({
        label: 'Token sent to app',
        value: receipt.token,
        hint: 'The app received this token. Keep a copy until it confirms the payment.',
        truncate: true,
      })
    }
    if (typeof receipt.payment_request === 'string' && receipt.payment_request) {
      rows.push({ label: 'Invoice sent to app', value: receipt.payment_request, truncate: true })
    }
    const createdInvoice = method === 'lightning' && !pay.invoice
    completedPayment.value = {
      amount: Number(receipt.amount_sats) || pay.amount_sats,
      verb: createdInvoice ? 'READY' : 'SENT',
      methodLabel: createdInvoice
        ? 'Lightning invoice created'
        : method === 'lightning' ? 'Paid over Lightning' : method === 'ecash' ? 'Sent as Cashu' : 'Sent on-chain',
      rows,
      note: createdInvoice ? 'The invoice was returned to the requesting app.' : `Payment approved for ${store.title || 'the app'}.`,
    }
    pendingPayment.value = null
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : 'Payment failed'
    paymentError.value = msg
  } finally {
    paymentProcessing.value = false
  }
}

/** User rejects the payment */
function rejectPayment() {
  if (pendingPayment.value) {
    sendPaymentResponse(paymentOrigin.value, pendingPayment.value.request_id, false, 'Payment denied by user')
    pendingPayment.value = null
  }
}

/** Resolve auto method based on amount */
function resolvePaymentMethod(pay: PaymentRequest): 'ecash' | 'lightning' | 'onchain' {
  if (pay.method && pay.method !== 'auto') return pay.method
  if (pay.invoice) return 'lightning'
  if (pay.address) return 'onchain'
  if (pay.amount_sats < 1000) return 'ecash'
  if (pay.amount_sats > 500000) return 'onchain'
  return 'lightning'
}

watch(
  () => store.isOpen,
  (open) => {
    if (open) {
      iframeLoading.value = true
      iframeBlocked.value = false
      clearTimers()
      // Set max load timeout — if iframe never fires load, show fallback
      loadTimeoutId = setTimeout(() => {
        if (iframeLoading.value) {
          iframeLoading.value = false
          iframeBlocked.value = true
        }
      }, 15000)
      closeBtnRef.value?.focus()
    } else {
      isRefreshing.value = false
      iframeLoading.value = true
      iframeBlocked.value = false
      clearTimers()
      // Clear any pending payment when closing
      if (pendingPayment.value) {
        rejectPayment()
      }
      completedPayment.value = null
    }
  }
)

onMounted(() => {
  window.addEventListener('keydown', onKeyDown, true)
  window.addEventListener('message', onMessage)
})

onBeforeUnmount(() => {
  store.setNostrFrame(null)
  clearTimers()
  stopProgress()
  window.removeEventListener('keydown', onKeyDown, true)
  window.removeEventListener('message', onMessage)
})
</script>

<style scoped>
.app-launcher-panel {
  will-change: transform, opacity;
}

.app-launcher-enter-active,
.app-launcher-leave-active {
  transition: opacity 0.3s ease;
}

.app-launcher-enter-active .app-launcher-backdrop {
  transition: opacity 0.3s ease, backdrop-filter 0.3s ease;
}
.app-launcher-leave-active .app-launcher-backdrop {
  transition: opacity 0.2s ease, backdrop-filter 0.2s ease;
}

.app-launcher-enter-active .app-launcher-panel {
  transition: transform 0.35s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.3s ease;
}
.app-launcher-leave-active .app-launcher-panel {
  transition: transform 0.25s cubic-bezier(0.55, 0, 1, 0.45), opacity 0.2s ease;
}

.app-launcher-enter-from {
  opacity: 0;
}
.app-launcher-enter-from .app-launcher-backdrop {
  opacity: 0;
  backdrop-filter: blur(0);
}
.app-launcher-enter-from .app-launcher-panel {
  transform: translateY(40px);
  opacity: 0;
}

.app-launcher-leave-to {
  opacity: 0;
}
.app-launcher-leave-to .app-launcher-backdrop {
  opacity: 0;
  backdrop-filter: blur(0);
}
.app-launcher-leave-to .app-launcher-panel {
  transform: translateY(30px);
  opacity: 0;
}
</style>
