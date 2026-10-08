<template>
  <div class="space-y-4">
    <p v-if="IS_DEMO" class="text-sm text-white/60">Connect payments from your own node. This preview does not change a live payment address.</p>
    <template v-else>
      <p v-if="siteId" class="text-sm text-white/70 break-all">Website: https://justworks.cash/{{ siteId }}</p>
      <button v-if="!address" :disabled="busy || !siteId" @click="prepare" class="glass-button rounded-lg px-5 py-2 text-sm disabled:opacity-50">{{ busy ? 'Getting wallet address…' : 'Use my Archipelago wallet' }}</button>
      <form v-else novalidate @submit.prevent="publish" class="space-y-3">
        <p class="text-sm text-white/80 break-all">Payments will go to <strong>{{ address }}</strong>.</p>
        <p class="text-sm text-white/60">Authorize Just Works to update and republish your website. Your owner sign-in is used for this request only and is not saved.</p>
        <label class="block text-sm text-white/80">Sign in with
          <select v-model="mode" :disabled="busy" class="block mt-1 w-full rounded-lg bg-black/30 border border-white/20 px-3 py-2"><option value="login">Just Works owner login</option><option value="key">Just Works owner key</option><option value="native">Saved Archipelago identity</option></select>
        </label>
        <template v-if="mode === 'login'">
          <label class="block text-sm text-white/80">Owner login<input v-model="ownerIdentifier" :disabled="busy" required autocomplete="username" maxlength="200" class="block mt-1 w-full rounded-lg bg-white/10 border border-white/20 px-3 py-2" /></label>
          <label class="block text-sm text-white/80">Owner password<input v-model="ownerPassword" :disabled="busy" required type="password" autocomplete="current-password" maxlength="2048" class="block mt-1 w-full rounded-lg bg-white/10 border border-white/20 px-3 py-2" /></label>
        </template>
        <template v-else-if="mode === 'key'">
          <label class="block text-sm text-white/80">Owner key<input v-model="ownerKey" :disabled="busy" required type="password" autocomplete="off" maxlength="2048" placeholder="nsec1… or ncryptsec1…" class="block mt-1 w-full rounded-lg bg-white/10 border border-white/20 px-3 py-2" /></label>
          <label class="block text-sm text-white/80">Key password (if encrypted)<input v-model="keyPassword" :disabled="busy" type="password" autocomplete="off" maxlength="2048" class="block mt-1 w-full rounded-lg bg-white/10 border border-white/20 px-3 py-2" /></label>
        </template>
        <label v-if="mode === 'native'" class="block text-sm">Use the saved key to
          <select v-model="nativeAction" :disabled="busy" class="block mt-1 rounded-lg bg-black/30 px-3 py-2"><option value="payment">Connect wallet &amp; publish</option><option value="business">Sign into Business tools</option></select>
        </label>
        <JustworksNativeKey v-if="mode === 'native'" :site-id="siteId" :npub="siteNpub" :disabled="busy" @credentials="publishNative" />
        <p role="status" aria-live="polite" class="text-sm text-white/90">{{ status }}</p>
        <button v-if="mode !== 'native'" type="submit" :disabled="busy" class="glass-button rounded-lg px-5 py-2 text-sm disabled:opacity-50">{{ busy ? 'Publishing and verifying…' : 'Connect wallet & publish' }}</button>
      </form>
      <button v-if="!siteId" @click="useAppLauncherStore().openSession('justworks')" class="glass-button rounded-lg px-4 py-2 text-sm">Open Just Works</button>
      <button @click="showReceive = true" class="text-sm text-white/70 underline">Open wallet Receive</button>
      <p class="text-xs text-white/50">Uses the existing hosted Lightning address and mint. Open wallet Receive → Ecash to claim incoming payments.</p>
    </template>
    <p v-if="!address" role="status" class="text-sm text-white/75">{{ status }}</p>
    <iframe v-if="!IS_DEMO" ref="bridge" :src="bridgeUrl" @load="loadConnection" title="Just Works payment connection" hidden />
    <ReceiveBitcoinModal :show="showReceive" @close="showReceive = false" />
  </div>
</template>
<script setup lang="ts">
import JustworksNativeKey from './JustworksNativeKey.vue'
import { ref, onMounted, onUnmounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { IS_DEMO } from '@/composables/useDemoIntro'
import { useAppLauncherStore } from '@/stores/appLauncher'
import ReceiveBitcoinModal from '@/components/ReceiveBitcoinModal.vue'
import { resolveAppUrl } from '../appSession/appSessionConfig'
const emit = defineEmits<{ connected: [] }>()
const bridge = ref<HTMLIFrameElement | null>(null)
const bridgeUrl = resolveAppUrl('justworks', '/bridge.html')
const origin = new URL(bridgeUrl, location.origin).origin
const nativeAction = ref('payment')
const siteNpub = ref('')
const siteId = ref(''), address = ref(''), status = ref('Checking your connected website…')
const busy = ref(false), showReceive = ref(false), mode = ref('login')
const ownerIdentifier = ref(''), ownerPassword = ref(''), ownerKey = ref(''), keyPassword = ref('')
let pending = '', operation = '', timer: ReturnType<typeof setTimeout> | undefined
function clearSecrets() { ownerPassword.value = ''; ownerKey.value = ''; keyPassword.value = '' }
function send(type: string, payload?: unknown) {
  pending = Array.from(crypto.getRandomValues(new Uint32Array(4))).join('-')
  operation = type
  bridge.value?.contentWindow?.postMessage({type, requestId:pending, payload}, origin)
  clearTimeout(timer)
  timer = setTimeout(() => {
    pending = ''; busy.value = false; clearSecrets()
    status.value = type === 'justworks:connect-payment' ? 'Publication could not be verified. The address may have been saved. Retry to check and publish again.' : 'Open Just Works to check app access, then reopen this guide.'
  }, type === 'justworks:connect-payment' ? 180000 : type === 'justworks:business-login' ? 65000 : 15000)
}
function loadConnection() { send('justworks:get-connection') }
async function prepare() {
  busy.value = true; status.value = 'Getting the address from your native wallet…'
  try {
    const result = await rpcClient.call<{address?: string}>({method:'wallet.ecash-lnaddress', timeout:60000})
    if (!result.address || !/^[a-z0-9._+-]{1,100}@minibits\.cash$/.test(result.address)) throw new Error('Native wallet address is unavailable. Open wallet Receive → Ecash to finish wallet setup, then retry.')
    address.value = result.address
    status.value = 'Ready for your Just Works owner authorization.'
  } catch { status.value = 'Could not get the native wallet address. Open wallet Receive → Ecash to finish setup, then retry.' }
  finally { busy.value = false }
}
function publishNative(credentials: {ownerKey: string}) {
  if (busy.value) return
  if (nativeAction.value === 'business') {
    busy.value = true
    status.value = 'Signing into your local Business tools…'
    try { send('justworks:business-login', {npub:siteNpub.value, ownerKey:credentials.ownerKey}) }
    catch { busy.value = false; pending = ''; status.value = 'Could not contact Business tools. Reload and retry.' }
    return
  }
  ownerKey.value = credentials.ownerKey
  mode.value = 'key'
  publish()
  mode.value = 'native'
}
function publish() {
  if (busy.value) return
  // Native validation can prevent submit without visible feedback on mobile.
  // Accept pasted owner keys in either mode; never send them as login identifiers.
  const identifier = ownerIdentifier.value.trim()
  const pastedKey = /^(nsec1|ncryptsec1)/.test(identifier)
  const key = (mode.value === 'key' ? ownerKey.value : pastedKey ? identifier : '').trim()
  if (!siteId.value || !address.value) {
    status.value = 'Reconnect your website and select your Archipelago wallet first.'
    return
  }
  if (mode.value === 'key' || pastedKey) {
    if (!/^(nsec1|ncryptsec1)/.test(key)) {
      status.value = 'Enter the website owner’s nsec1… or ncryptsec1… key. A public npub cannot authorize changes.'
      return
    }
  } else if (!identifier || !ownerPassword.value) {
    status.value = 'Enter your Just Works owner login and password, or choose Just Works owner key to use an nsec.'
    return
  }
  const credentials = key
    ? {ownerKey:key, password:mode.value === 'key' ? keyPassword.value : ownerPassword.value}
    : {ownerIdentifier:identifier, ownerPassword:ownerPassword.value}
  busy.value = true
  status.value = 'Saving the payment address, publishing, and checking the public website…'
  try {
    if (!bridge.value?.contentWindow) throw new Error('Connector unavailable')
    send('justworks:connect-payment', {siteId:siteId.value, address:address.value, credentials})
  } catch {
    clearTimeout(timer); pending = ''; busy.value = false
    status.value = 'Could not contact the Just Works app. Reload this guide and try again.'
  } finally {
    if (pastedKey) ownerIdentifier.value = ''
    clearSecrets()
  }
}
function receive(event: MessageEvent) {
  if (!pending || event.source !== bridge.value?.contentWindow || event.origin !== origin || event.data?.requestId !== pending) return
  if (operation === 'justworks:get-connection' && event.data.type === 'justworks:connection') {
    siteNpub.value = typeof event.data.npub === 'string' ? event.data.npub : ''
    const id = event.data.siteId
    siteId.value = typeof id === 'string' && id.length <= 160 && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id) ? id : ''
    status.value = siteId.value ? '' : 'Connect your website in the previous step first, in this browser.'
  } else if (operation === 'justworks:business-login' && event.data.type === 'justworks:business-result') {
    status.value = event.data.ok === true && event.data.npub === siteNpub.value
      ? 'Business tools signed in. Open Just Works and choose Business tools.'
      : 'Business tools could not verify the owner key. Your website has not been changed.'
  } else if (operation === 'justworks:connect-payment' && event.data.type === 'justworks:payment-result') {
    if (event.data.verified === true && event.data.siteId === siteId.value && event.data.address === address.value) {
      status.value = 'The published website now uses your Archipelago wallet address.'
      emit('connected')
    } else status.value = typeof event.data.error === 'string' ? event.data.error : 'The published payment destination could not be verified. Retry.'
  } else if (event.data.type === 'justworks:site-error') status.value = 'Could not reach Just Works. Retry after checking app access.'
  else return
  clearTimeout(timer); pending = ''; busy.value = false; clearSecrets()
}
onMounted(() => window.addEventListener('message', receive))
onUnmounted(() => { clearTimeout(timer); clearSecrets(); window.removeEventListener('message', receive) })
</script>
