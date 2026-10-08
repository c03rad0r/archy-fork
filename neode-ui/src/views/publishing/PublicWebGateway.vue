<script setup lang="ts">
import { computed, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { installPublishingApp } from '@/services/installPublishingApp'
import { useAppStore } from '@/stores/app'
import type { WebsiteProject } from '@/services/publishing'
interface GatewayStatus { configured: boolean; host?: string; port?: number; domains?: string[]; certificate_mode?: string; routes: { id: string; domain: string }[]; error?: string }
const props = defineProps<{ gateway: GatewayStatus; project?: WebsiteProject | null; app?: { id: string; name: string } | null }>()
const emit = defineEmits<{ refresh: [] }>()
const appStore = useAppStore()
const appDomain = ref('')
const busy = ref(false)
const error = ref('')
const message = ref('')
const enrollment = ref<Record<string, unknown> | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)
const acknowledge = ref(false)
const testCertificates = ref(false)
const installed = computed(() => !!appStore.data?.['package-data']?.['public-web-router'])
const routeId = computed(() => props.project?.id ?? (props.app ? `app-${props.app.id}` : ''))
const connected = computed(() => props.gateway.routes.some(r => r.id === routeId.value))
async function perform(fn: () => Promise<void>) {
  busy.value = true; error.value = ''; message.value = ''
  try { await fn() } catch (e) { error.value = e instanceof Error ? e.message : 'Gateway action failed' }
  finally { busy.value = false }
}
async function readEnrollment(event: Event) {
  enrollment.value = null; acknowledge.value = false; error.value = ''
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  try {
    if (file.size > 65536) throw new Error('Enrollment file is too large')
    const data = JSON.parse(await file.text())
    if (!data || typeof data !== 'object' || typeof data.host !== 'string' || !Array.isArray(data.domains) || !data.domains.every((d: unknown) => typeof d === 'string')) throw new Error('Choose the enrollment file supplied by your gateway operator')
    enrollment.value = data
  } catch { error.value = 'Choose a valid gateway enrollment JSON file. Its contents stay in this setup session until you connect.' }
}
async function configure() {
  if (!enrollment.value || !acknowledge.value) return
  await perform(async () => {
    await rpcClient.call({ method: 'publishing.gateway-configure', params: { enrollment: enrollment.value, certificate_mode: testCertificates.value ? 'test' : 'public', acknowledge: true }, maxRetries: 0 })
    enrollment.value = null; acknowledge.value = false
    if (fileInput.value) fileInput.value.value = ''
    message.value = 'Gateway saved privately. Connect each published website when you are ready.'
    emit('refresh')
  })
}
async function route(enabled: boolean) {
  if (!props.project && !props.app) return
  await perform(async () => {
    if (props.app) await rpcClient.call({ method: 'publishing.gateway-app-route', params: { app_id: props.app.id, domain: appDomain.value.trim(), enabled }, maxRetries: 0 })
    else await rpcClient.call({ method: 'publishing.gateway-route', params: { id: props.project!.id, enabled }, maxRetries: 0 })
    message.value = enabled ? 'Route requested. Check HTTPS and guest access before sharing the address.' : 'Gateway route removed. Other connections remain available.'
    emit('refresh')
  })
}
async function disconnect() {
  await perform(async () => {
    await rpcClient.call({ method: 'publishing.gateway-disconnect', maxRetries: 0 })
    message.value = 'Gateway disconnected. Its local enrollment was removed; website drafts and certificates are retained.'
    emit('refresh')
  })
}
async function install() {
  await perform(async () => { await installPublishingApp('public-web-router'); message.value = 'Router installation requested through the app catalogue.' })
}
</script>

<template>
  <section class="space-y-4" aria-label="Your public gateway">
    <h3 class="font-medium">Keep HTTPS on this node</h3>
    <p class="text-sm text-white/60">Connect to a gateway you control using the open-source Public Web Router. The gateway forwards encrypted traffic; website certificates and keys stay here. You can reuse this connection for websites and supported apps.</p>
    <p v-if="gateway.error" role="alert" class="text-sm text-amber-200">{{ gateway.error }}</p>
    <button v-if="!installed" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="install">Install Public Web Router</button>
    <template v-if="gateway.configured">
      <p class="text-sm break-all">Gateway: {{ gateway.host }} · control port {{ gateway.port }}</p>
      <p class="text-sm break-all">Assigned domains: {{ gateway.domains?.join(', ') }}</p>
      <p v-if="gateway.certificate_mode === 'test'" class="text-sm text-amber-200">Test certificates only. Ordinary browsers will not trust this route.</p>
      <p v-else class="text-sm text-white/60">Point your domain at the gateway’s public IP. It must forward public port 443 to this node so a certificate can be issued.</p>
      <div v-if="project" class="flex flex-wrap gap-3">
        <button v-if="!connected" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || !installed || !project.fips_publication || !project.domain || !gateway.domains?.includes(project.domain.hostname)" @click="route(true)">Connect this published website</button>
        <button v-else class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="route(false)">Disconnect this website from gateway</button>
      </div>
      <div v-if="app" class="space-y-3">
        <p class="text-sm">Connect {{ app.name }} through its existing app gate. Guests still need app-only access; this does not grant dashboard access.</p>
        <label v-if="!connected" class="block text-sm">Assigned domain for this app<input v-model="appDomain" maxlength="253" autocomplete="off" class="w-full mt-2 rounded-lg border border-white/15 bg-white/5 px-3 py-2 text-sm" placeholder="app.yourdomain.com" :disabled="busy" /></label>
        <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || (!connected && (!installed || !gateway.domains?.includes(appDomain.trim())))" @click="route(!connected)">{{ connected ? 'Disconnect this app from gateway' : 'Connect this app through its gate' }}</button>
      </div>
      <p v-if="project && !project.fips_publication" class="text-sm text-white/60">Publish the website upstream in Review and publish, then return here to connect it.</p>
      <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="disconnect">Disconnect all gateway routes</button>
    </template>
    <details class="space-y-4">
      <summary class="cursor-pointer">{{ gateway.configured ? 'Replace gateway enrollment' : 'Connect your gateway' }}</summary>
      <p class="text-sm text-white/60">Choose the private enrollment file supplied by your gateway operator. It contains connection credentials: keep it off Nostr and public file storage.</p>
      <label class="block text-sm">Gateway enrollment file<input ref="fileInput" type="file" accept="application/json,.json" class="block w-full mt-2 text-sm" :disabled="busy" @change="readEnrollment" /></label>
      <template v-if="enrollment">
        <p class="text-sm break-all">Connect to {{ enrollment.host }} · assigned domains: {{ (enrollment.domains as string[]).join(', ') }}</p>
        <label class="flex items-start gap-3 text-sm"><input v-model="acknowledge" type="checkbox" class="mt-1" :disabled="busy" /><span>I trust this gateway operator. Replacing enrollment disconnects existing gateway routes until I connect them again.</span></label>
        <details><summary class="cursor-pointer text-sm">Testing options</summary><label class="flex items-start gap-3 mt-3 text-sm"><input v-model="testCertificates" type="checkbox" class="mt-1" :disabled="busy" /><span>Use private test certificates for isolated-port testing.</span></label></details>
        <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || !acknowledge" @click="configure">Save private gateway connection</button>
      </template>
    </details>
    <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
    <p v-if="message" role="status" class="text-sm text-white/70">{{ message }}</p>
  </section>
</template>
