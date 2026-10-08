<template>
  <div class="space-y-4">
    <p class="text-sm text-white/70">1. Open Just Works and choose Create. Add your business name and existing website or social links, then publish using the usual Just Works setup.</p>
    <button @click="useAppLauncherStore().openSession('justworks')" class="glass-button inline-flex rounded-lg px-5 py-2 text-sm">Open Just Works</button>
    <p class="text-sm text-white/70">2. Return here with your published page link. Connecting your first website reveals Business tools in the launcher. Use your existing Just Works owner login there.</p>
    <p v-if="IS_DEMO" class="text-sm text-amber-200">Demo: explore the original Just Works site above. Connecting a published page requires the installed app on your node.</p>
    <form v-else class="space-y-3" @submit.prevent="connect">
      <label class="block text-sm text-white/80">Your Just Works page or npub
        <input v-model="url" required maxlength="240" type="text" inputmode="url" autocomplete="url" placeholder="https://justworks.cash/your-business" class="mt-2 w-full rounded-lg bg-white/10 border border-white/20 px-3 py-2 text-white" />
      </label>
      <p class="text-xs text-white/50">Use a public page link or npub. Connecting a page does not sign you in or establish ownership.</p>
      <button :disabled="busy || !bridgeReady" class="glass-button rounded-lg px-5 py-2 text-sm disabled:opacity-50">{{ busy ? 'Checking…' : 'Check and connect' }}</button>
    </form>
    <p role="status" class="text-sm text-white/75">{{ status }}</p>
    <iframe v-if="!IS_DEMO" ref="bridge" :src="bridgeUrl" @load="bridgeReady = true" title="Just Works connection" hidden />
  </div>
</template>
<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { IS_DEMO } from '@/composables/useDemoIntro'
import { resolveAppUrl } from '../appSession/appSessionConfig'
import { isJustworksCompletion } from './justworksCompletion'
const emit = defineEmits<{ connected: [] }>()
const bridge = ref<HTMLIFrameElement | null>(null)
const bridgeUrl = resolveAppUrl('justworks', '/bridge.html')
const origin = new URL(bridgeUrl, window.location.origin).origin
const requestedSite = useRoute().query.site
const url = ref(typeof requestedSite === 'string' && requestedSite.length <= 240 ? requestedSite : '')
const bridgeReady = ref(false)
const busy = ref(false)
const status = ref('')
let requestId = ''
let timer: ReturnType<typeof setTimeout> | undefined
function connect() {
  busy.value = true
  status.value = 'Checking your published website…'
  requestId = Array.from(crypto.getRandomValues(new Uint32Array(4))).join('-')
  bridge.value?.contentWindow?.postMessage({type:'justworks:check-site', requestId, url:url.value.trim()}, origin)
  clearTimeout(timer)
  timer = setTimeout(() => {
    busy.value = false
    requestId = ''
    status.value = 'Could not connect. Open the installed Just Works app to check access, then return here and retry.'
  }, 20000)
}
function receive(event: MessageEvent) {
  if (!busy.value || event.source !== bridge.value?.contentWindow || event.origin !== origin || event.data?.requestId !== requestId) return
  if (isJustworksCompletion(event, 'justworks', bridgeUrl, bridge.value?.contentWindow ?? null)) {
    status.value = event.data.remembered ? 'Website connected. Business tools are now available in your launcher.' : 'Allow browser storage for Just Works, then reconnect so the launcher can remember your website.'
    if (event.data.remembered) emit('connected')
  } else if (event.data?.type === 'justworks:site-error') {
    status.value = typeof event.data.error === 'string' ? event.data.error : 'Could not check this website. Try again.'
  } else return
  clearTimeout(timer)
  busy.value = false
  requestId = ''
}
onMounted(() => window.addEventListener('message', receive))
onUnmounted(() => { clearTimeout(timer); window.removeEventListener('message', receive) })
</script>
