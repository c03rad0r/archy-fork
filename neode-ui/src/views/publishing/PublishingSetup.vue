<script setup lang="ts">
import { computed, onDeactivated, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { rpcClient } from '@/api/rpc-client'
import { pendingWebsiteHtml } from '@/services/websiteImport'
import { publishing, PUBLISH_ROUTES, websitePreview } from '@/services/publishing'
import type { DnsPlan, HttpsCheck, PublishRoute, PublishingStatus, WebsiteProject } from '@/services/publishing'
import { nsiteIdentities, prepareNsite, publishNsite, retryNsite, requestNsiteDeletion, namedNsiteUrl, relayAddresses, storeLocalWebsite } from '@/services/nsitePublishing'
import type { NsiteIdentity, PreparedNsite } from '@/services/nsitePublishing'

const route = useRoute()
const appStore = useAppStore()
const blossomInstalled = computed(() => !!appStore.data?.['package-data']?.blossom)
const websiteMode = computed(() => route.name === 'publish-website')
const status = ref<PublishingStatus | null>(null)
const error = ref('')
const message = ref('')
const busy = ref(false)
const projectId = ref('')
const name = ref('My website')
const selected = ref<PublishRoute[]>([])
const html = ref('')
const hostname = ref('')
const destination = ref('')
const prompt = ref('')
const model = ref('')
const dns = ref<DnsPlan | null>(null)
const httpsCheck = ref<HttpsCheck | null>(null)
watch([projectId, hostname, destination, () => status.value?.state.version], () => { httpsCheck.value = null })
const acknowledgeFips = ref(false)
const acknowledgeTor = ref(false)
const identities = ref<NsiteIdentity[]>([])
const identityId = ref('')
const blossom = ref('')
const relays = ref('')
const gateway = ref('')
const nsiteUrl = ref('')
const guestApp = ref('')
const guestLabel = ref('Guest')
const guestHours = ref(24)
const issuedAccess = ref<{ id: string; token: string; app_id: string; expires_at: number } | null>(null)
onDeactivated(() => { issuedAccess.value = null })
onBeforeUnmount(() => { issuedAccess.value = null })
const shareableApps = computed(() => status.value?.apps.filter(a => a.guest_access).filter((a, i, all) => all.findIndex(b => b.id === a.id) === i) ?? [])
const guestTarget = computed(() => shareableApps.value.find(a => a.id === guestApp.value))
const acknowledgeNostr = ref(false)
const acknowledgeUpload = ref(false)
const nsiteReview = ref<{ projectId: string; version: number; identity: NsiteIdentity; relays: string[]; prepared: PreparedNsite } | null>(null)
const onion = computed(() => status.value?.onions?.find(l => l.project_id === projectId.value))
const listener = computed(() => status.value?.listeners?.find(l => l.project_id === projectId.value))
const current = computed(() => status.value?.state.projects[projectId.value])
const projects = computed(() => Object.values(status.value?.state.projects ?? {}))
const publicName = computed(() => selected.value.includes('public-web') || selected.value.includes('nostr'))
const preview = computed(() => websitePreview(html.value))
const routes = computed(() => PUBLISH_ROUTES.filter(r => websiteMode.value || r.id !== 'nostr'))

async function perform(work: () => Promise<void>) {
  if (busy.value) return
  busy.value = true; error.value = ''; message.value = ''
  try { await work() } catch (e) { error.value = e instanceof Error ? e.message : 'The operation failed. Your saved project has been retained.' }
  finally { busy.value = false }
}
function selectProject(p: WebsiteProject) {
  projectId.value = p.id; name.value = p.name; selected.value = [...p.routes]
  html.value = p.draft; hostname.value = p.domain?.hostname ?? ''; destination.value = p.domain?.destination ?? ''; dns.value = null; acknowledgeFips.value = false; acknowledgeTor.value = false
  identityId.value = p.nsite_receipt?.identity_id ?? ''; blossom.value = p.nsite_receipt?.server ?? ''; acknowledgeNostr.value = false; nsiteUrl.value = ''
}
async function refresh() {
  await perform(async () => {
    status.value = await publishing.status()
    if (!relays.value) relays.value = (status.value.nostr_relays ?? []).join('\n')
    if (!websiteMode.value) selected.value = [...status.value.state.connections]
    else if (current.value) selectProject(current.value)
    else if (projects.value[0]) selectProject(projects.value[0])
  })
}
async function create() {
  await perform(async () => {
    if (!status.value) return
    const result = await publishing.update(status.value.state.version, { action: 'create', name: name.value })
    status.value.state = result.state
    if (result.project_id) selectProject(result.state.projects[result.project_id]!)
    message.value = 'Website project created on your node.'
  })
}
async function importFromAiui() {
  await perform(async () => {
    if (!status.value || pendingWebsiteHtml.value === null) return
    const incoming = pendingWebsiteHtml.value
    const result = await publishing.update(status.value.state.version, { action: 'create', name: 'Website from AIUI' })
    status.value.state = result.state
    if (result.project_id) {
      selectProject(result.state.projects[result.project_id]!)
      html.value = incoming
      pendingWebsiteHtml.value = null
      message.value = 'AIUI draft imported into a new project. Preview it, then save before publishing.'
    }
  })
}
async function save() {
  await perform(async () => {
    if (!status.value) return
    const change = websiteMode.value ? {
      action: 'save', id: projectId.value, name: name.value, routes: selected.value,
      domain: publicName.value && hostname.value.trim() ? { hostname: hostname.value, destination: destination.value.trim() || null } : null,
      html: html.value,
    } : { action: 'connections', routes: selected.value }
    const result = await publishing.update(status.value.state.version, change)
    status.value.state = result.state
    if (websiteMode.value) hostname.value = current.value?.domain?.hostname ?? ''
    message.value = websiteMode.value ? 'Draft and route choices saved on your node. Publish when you are ready to share this version.' : 'Connection preferences saved on your node. Existing app access has not changed.'
  })
}
async function restore(revision: string) {
  await perform(async () => {
    if (!status.value) return
    const result = await publishing.update(status.value.state.version, { action: 'restore', id: projectId.value, revision })
    status.value.state = result.state
    selectProject(result.state.projects[projectId.value]!)
    message.value = 'Previous draft restored. Published content has not changed.'
  })
}
async function setFipsPublication(enable: boolean) {
  await perform(async () => {
    if (!status.value || !current.value) return
    const result = await publishing.update(status.value.state.version, enable
      ? { action: 'publish-fips', id: projectId.value, acknowledge_public: acknowledgeFips.value }
      : { action: 'unpublish-fips', id: projectId.value })
    status.value.state = result.state
    status.value = await publishing.status()
    acknowledgeFips.value = false
    message.value = enable ? 'Saved version selected for FIPS publication. Check listener status, firewall and access from another FIPS device.' : 'FIPS website unpublished. Your draft and revisions are retained.'
  })
}
async function generate() {
  await perform(async () => {
    const result = await publishing.generate(prompt.value, model.value.trim())
    html.value = result.html
    message.value = 'Local model draft ready to preview. Save it to keep this revision.'
  })
}
async function setTorPublication(enable: boolean) {
  await perform(async () => {
    if (!status.value || !current.value) return
    const result = await publishing.update(status.value.state.version, enable
      ? { action: 'publish-tor', id: projectId.value, acknowledge_public: acknowledgeTor.value }
      : { action: 'unpublish-tor', id: projectId.value })
    status.value.state = result.state
    status.value = await publishing.status()
    acknowledgeTor.value = false
    message.value = enable ? 'Onion publication requested. Tor may take a few minutes to connect; reload to check its address.' : 'Onion website unpublished. Its address keys are retained so you can publish again at the same address.'
  })
}
async function prepareDns() {
  await perform(async () => { dns.value = await publishing.dns({ hostname: hostname.value, destination: destination.value || null }) })
}
async function verifyHttps() {
  await perform(async () => {
    httpsCheck.value = null
    if (!status.value) return
    httpsCheck.value = await publishing.verifyHttps(projectId.value, status.value.state.version)
  })
}
async function loadIdentities() { await perform(async () => { identities.value = await nsiteIdentities() }) }
async function createGuestAccess() {
  await perform(async () => {
    issuedAccess.value = null
    issuedAccess.value = await rpcClient.call({ method: 'publishing.access-create', params: { app_id: guestApp.value, label: guestLabel.value, hours: guestHours.value }, maxRetries: 0 })
    status.value = await publishing.status()
    message.value = 'App-only access created. Copy the token now; it cannot be shown again.'
  })
}
async function revokeGuestAccess(id: string) {
  await perform(async () => {
    await rpcClient.call({ method: 'publishing.access-revoke', params: { id }, maxRetries: 0 })
    if (issuedAccess.value?.id === id) issuedAccess.value = null
    status.value = await publishing.status()
    message.value = 'Access revoked for new requests. Content already downloaded cannot be recalled.'
  })
}
async function storeLocally() {
  await perform(async () => {
    const identity = identities.value.find(i => i.id === identityId.value)
    if (!identity || !status.value || !current.value) throw new Error('Choose a profile identity and save a draft first')
    await storeLocalWebsite(projectId.value, status.value.state.version, identity)
    status.value = await publishing.status()
    message.value = 'Saved draft stored in local Blossom and fetched back to verify its bytes. Nothing was announced or replicated externally.'
  })
}
watch([projectId, blossom, relays, identityId, html, selected], () => { nsiteReview.value = null; acknowledgeNostr.value = false; acknowledgeUpload.value = false }, { deep: true })
async function reviewNsite() {
  await perform(async () => {
    if (!status.value || !current.value) return
    const identity = identities.value.find(i => i.id === identityId.value)
    if (!identity) throw new Error('Choose a profile identity first')
    const targets = relayAddresses(relays.value)
    const prepared = await prepareNsite(projectId.value, status.value.state.version, blossom.value, current.value.draft)
    nsiteReview.value = { projectId: projectId.value, version: status.value.state.version, identity: { ...identity }, relays: [...targets], prepared }
    acknowledgeNostr.value = false; acknowledgeUpload.value = false
  })
}
async function handleNsite(action: 'publish' | 'retry' | 'delete') {
  await perform(async () => {
    if (!status.value || !current.value || !acknowledgeNostr.value) return
    const targets = relayAddresses(relays.value)
    const identity = identities.value.find(i => i.id === identityId.value)
    const receipt = current.value.nsite_receipt
    try {
      if (action === 'retry' && receipt) await retryNsite(projectId.value, receipt, targets)
      else {
        if (!identity) throw new Error('Load identities and choose the profile identity you want to use')
        if (action === 'delete' && receipt) await requestNsiteDeletion(projectId.value, receipt, identity, targets)
        else {
          const review = nsiteReview.value
          if (!review || !acknowledgeUpload.value) throw new Error('Review the exact upload and explicitly approve replication first')
          await publishNsite(review.projectId, review.version, review.identity, review.relays, review.prepared)
        }
      }
    } finally {
      acknowledgeNostr.value = false; acknowledgeUpload.value = false; nsiteReview.value = null
      // A failed relay delivery can still leave a durable upload/manifest. Show
      // that receipt so retry never silently uploads or signs a second copy.
      status.value = await publishing.status()
    }
    message.value = action === 'delete' ? 'A relay accepted the deletion request. Other relays, Blossom servers and cached copies may retain the website.' : 'The uploaded bytes were checked and a relay accepted the named-site manifest. Gateway availability still needs checking.'
  })
}
async function showNsiteAddress() {
  await perform(async () => {
    if (current.value?.nsite_receipt) nsiteUrl.value = namedNsiteUrl(current.value.nsite_receipt, gateway.value)
  })
}
function download() {
  const url = URL.createObjectURL(new Blob([html.value], { type: 'text/html;charset=utf-8' }))
  const a = document.createElement('a'); a.href = url; a.download = 'index.html'; a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
onMounted(refresh)
</script>

<template>
  <main class="max-w-5xl mx-auto px-4 sm:px-6 py-6 space-y-6">
    <RouterLink to="/dashboard?tab=setup" class="text-sm text-white/60 hover:text-white">← Setup</RouterLink>
    <div class="flex items-start justify-between gap-4">
      <div><h1 class="text-2xl font-semibold">{{ websiteMode ? 'Publish a website' : 'Allow external connections' }}</h1>
        <p class="text-white/60 mt-2">{{ websiteMode ? 'Create a website on your node and choose where people can find it.' : 'Choose how people will connect to selected services on your node.' }}</p></div>
      <button class="glass-button px-4 py-2" :disabled="busy" @click="refresh">Reload</button>
    </div>
    <p v-if="error" role="alert" class="rounded-xl p-4 bg-red-500/10 text-red-200">{{ error }}</p>
    <p v-if="message" role="status" class="rounded-xl p-4 bg-green-500/10 text-green-200">{{ message }}</p>
    <p v-if="busy" role="status" class="text-white/60">Working…</p>
      <template v-if="status">
        <section v-if="websiteMode" class="glass-card p-5 space-y-3" data-testid="blossom-setup">
          <h2 class="text-lg font-semibold">Local website files</h2>
          <template v-if="blossomInstalled">
            <p>Blossom is installed. You can skip installation.</p>
            <RouterLink to="/dashboard/apps/blossom" class="underline">Manage local Blossom</RouterLink>
            <template v-if="current">
              <button class="glass-button px-4 py-2" :disabled="busy" @click="loadIdentities">Choose a storage identity</button>
              <label class="block">Profile for local files<select v-model="identityId" :disabled="busy" class="field mt-2"><option value="">Choose an identity</option><option v-for="identity in identities" :key="identity.id" :value="identity.id">{{ identity.name }}</option></select></label>
              <button class="glass-button px-4 py-2" :disabled="busy || !identityId || !current.draft || html !== current.draft" @click="storeLocally">Store saved website in local Blossom</button>
              <p v-if="html !== current.draft" class="text-sm">Save your edits before storing this version in Blossom.</p>
              <p v-if="current.local_archive" class="text-sm break-all">Verified local snapshot: {{ current.local_archive.size }} bytes · {{ new Date(current.local_archive.created_at).toLocaleString() }} · SHA-256 {{ current.local_archive.sha256 }}</p>
              <p class="text-sm text-white/60">This stores the saved draft shown below. Later edits need another explicit store. Local files require node login; this does not create a public Blossom endpoint.</p>
            </template>
          </template>
          <template v-else>
            <p>Install Blossom from the app catalogue to store website files on this node. Create a profile identity first; Blossom uses the normal Archipelago signer.</p>
            <RouterLink to="/dashboard/marketplace/blossom" class="glass-button inline-block px-4 py-2">Install Blossom</RouterLink>
            <p class="text-sm text-white/60">Return here after installation. This step is optional for a simple HTML page served directly by the node.</p>
          </template>
          <p class="text-sm text-white/60">Installation and local uploads do not announce anything on Nostr. Publishing files externally requires a separate review of the content and destinations.</p>
        </section>
      <div class="rounded-xl p-4 border border-amber-300/20 bg-amber-400/10 text-amber-100 text-sm">{{ status.notice }}</div>
      <fieldset :disabled="busy" class="space-y-6">
        <section v-if="websiteMode && pendingWebsiteHtml !== null" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Continue from AIUI</h2>
          <p class="text-sm text-white/60">Create a new project from the HTML you selected in AIUI. Existing projects remain unchanged.</p>
          <button class="glass-button px-4 py-2" @click="importFromAiui">Import into a new website</button>
          <button class="text-sm underline ml-3" @click="pendingWebsiteHtml = null">Discard import</button>
        </section>
        <section v-if="websiteMode" class="glass-card p-5 space-y-4">
          <h2 class="text-lg font-semibold">Your websites</h2>
          <div v-if="projects.length" class="flex flex-wrap gap-2">
            <button v-for="p in projects" :key="p.id" class="glass-button px-3 py-2" :aria-pressed="p.id === projectId" @click="selectProject(p)">{{ p.name }}</button>
          </div>
          <label class="block">Website name<input v-model="name" maxlength="100" class="field mt-2" /></label>
          <button class="glass-button px-4 py-2" @click="create">Create another website</button>
        </section>
        <section v-if="websiteMode && current" class="glass-card p-5 space-y-4">
          <h2 class="text-lg font-semibold">Describe and preview</h2>
          <p class="text-sm text-white/60">Generate a simple page with a model installed on this node, or paste HTML from AIUI. Generation stays on your node.</p>
          <label class="block">What would you like to make?<textarea v-model="prompt" maxlength="16000" rows="3" class="field mt-2" placeholder="A simple website for my community garden…" /></label>
          <label class="block">Installed Ollama model<input v-model="model" class="field mt-2" placeholder="Enter the exact installed model name" /></label>
          <button class="glass-button px-4 py-2" :disabled="!prompt.trim() || !model.trim()" @click="generate">Generate locally</button>
          <label class="block">Website HTML<textarea v-model="html" rows="8" class="field mt-2 font-mono text-xs" spellcheck="false" /></label>
          <iframe :srcdoc="preview" sandbox="" referrerpolicy="no-referrer" title="Isolated website preview" class="w-full h-96 rounded-xl bg-white" />
          <p class="text-xs text-white/50">Preview blocks scripts, forms and external requests. It cannot access your dashboard.</p>
          <button class="glass-button px-4 py-2" :disabled="!html" @click="download">Download HTML</button>
        </section>
        <section class="glass-card p-5 space-y-4">
          <h2 class="text-lg font-semibold">Where should it be available?</h2>
          <p class="text-sm text-white/60">Choose any combination. Each connection will be checked separately.</p>
          <label v-for="option in routes" :key="option.id" class="flex items-start gap-3 rounded-xl border border-white/10 p-4 cursor-pointer">
            <input v-model="selected" type="checkbox" :value="option.id" class="mt-1" />
            <span><span class="font-medium">{{ option.title }}</span><span class="block text-sm text-white/60 mt-1">{{ option.description }}</span>
              <span v-if="websiteMode && status.state.connections.includes(option.id)" class="block text-xs text-amber-200 mt-2">Already selected in connection setup; reachability still needs verification.</span>
            </span>
          </label>
          <p v-if="selected.includes('fips')" class="text-sm text-white/60">{{ status.fips_address ? 'A local FIPS address exists. This does not yet verify a website or app route.' : 'No local FIPS address detected. FIPS must be connected before its routes can be verified.' }}</p>
          <RouterLink v-if="websiteMode" to="/dashboard/setup/external-access" class="inline-block text-sm underline">Manage shared connections</RouterLink>
        </section>
        <section v-if="websiteMode && publicName" class="glass-card p-5 space-y-4">
          <h2 class="text-lg font-semibold">Your domain</h2>
          <p class="text-sm text-white/60">Use a domain you own, or buy one with Bitcoin or Lightning. FIPS and Tor addresses do not need a domain purchase.</p>
          <a href="https://mynymbox.io/domainregistration" target="_blank" rel="noopener noreferrer" class="glass-button inline-block px-4 py-2">Buy a domain through Mynymbox ↗</a>
          <p class="text-xs text-white/50">Mynymbox is the registrant of record; you retain contractual control and transfer rights. Complete checkout yourself, then return here. No hosting purchase is needed.</p>
          <label class="block">Website hostname<input v-model="hostname" class="field mt-2" placeholder="www.yourdomain.com" /></label>
          <label class="block">Gateway hostname or public IP<input v-model="destination" class="field mt-2" placeholder="Use the destination supplied by your gateway" /></label>
          <p class="text-sm text-white/60">For a tunnel, point DNS at the public gateway. For a direct connection, use the node’s public IP. Do not use a home-network, FIPS or onion address for public web DNS.</p>
          <div v-if="selected.includes('public-web') && current?.fips_publication && status.fips_address" class="space-y-2 rounded-lg border border-white/10 p-4">
            <h3 class="font-medium">Use an existing reverse proxy</h3>
            <p class="text-sm text-white/60">If your proxy can reach this node over FIPS, you can reuse that connection. In Nginx Proxy Manager, add a separate Proxy Host with these settings:</p>
            <dl class="text-sm grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
              <dt>Domain</dt><dd class="font-mono break-all">{{ hostname || 'Your website hostname' }}</dd>
              <dt>Scheme</dt><dd>http</dd>
              <dt>Forward host</dt><dd class="font-mono break-all">[{{ status.fips_address }}]</dd>
              <dt>Forward port</dt><dd>{{ current.fips_publication.port }}</dd>
            </dl>
            <p class="text-sm text-white/60">Point the domain’s DNS at your proxy’s public address. Request a certificate in the proxy’s SSL tab and enable Force SSL. Then open the HTTPS address from a device outside your home network.</p>
            <p class="text-sm text-amber-200">The proxy terminates HTTPS and can read the public page. Removing the upstream publication also disconnects this proxy route.</p>
            <button class="glass-button px-4 py-2" :disabled="hostname !== current.domain?.hostname || !current.routes.includes('public-web')" @click="verifyHttps">Check public HTTPS</button>
            <p v-if="httpsCheck" class="text-sm text-green-200">Verified https://{{ httpsCheck.hostname }}/ at {{ new Date(httpsCheck.checked_at).toLocaleString() }}: valid TLS and exact published content. Checked from this node; also test from an outside device.</p>
            <p v-else class="text-sm text-amber-200">Public HTTPS has not been verified for these saved settings.</p>
          </div>
          <button class="glass-button px-4 py-2" :disabled="!hostname || !destination" @click="prepareDns">Show DNS instructions</button>
          <div v-if="dns" class="space-y-3">
            <p>In Mynymbox, open Domains → DNS Management → your domain → Manage records → Add Record.</p>
            <div class="overflow-x-auto"><table class="w-full text-sm text-left"><thead><tr><th>Type</th><th>Name</th><th>Value</th><th>TTL</th></tr></thead><tbody><tr v-for="record in dns.records" :key="record.name"><td>{{ record.record_type }}</td><td class="select-all">{{ record.name }}</td><td class="select-all">{{ record.value }}</td><td>{{ record.ttl }}</td></tr></tbody></table></div>
            <p v-for="note in dns.notes" :key="note" class="text-sm text-white/60">{{ note }}</p>
            <p class="text-amber-200 text-sm">Instructions prepared — DNS and HTTPS have not been verified.</p>
            <a href="https://mynymbox.io/docs?doc=domains/dns-records" target="_blank" rel="noopener noreferrer" class="underline text-sm">Mynymbox’s DNS guide ↗</a>
          </div>
        </section>
        <section v-if="!websiteMode" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Existing app access</h2>
          <p class="text-sm text-white/60">This inventory shows existing access policies. Local-only APIs are excluded. A local listener does not prove external reachability.</p>
          <ul class="space-y-2"><li v-for="app in status.apps" :key="app.id + app.port" class="flex flex-wrap justify-between gap-2 text-sm"><span>{{ app.name }} · {{ app.port }}</span><span class="text-white/60">{{ app.listener_claimed ? 'Local proxy listening' : 'Listener not confirmed' }} · {{ app.authentication === 'node-session' ? 'Node login required' : 'App access policy' }}</span></li></ul>
        </section>
        <section v-if="!websiteMode" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Grant access to an app</h2>
          <p class="text-sm text-white/60">Give someone access to one application without sharing your dashboard login. Only apps that explicitly support guest sharing are offered. Their own account permissions still apply.</p>
          <label class="block">Application<select v-model="guestApp" class="field mt-2"><option value="">Choose an application</option><option v-for="app in shareableApps" :key="app.id" :value="app.id">{{ app.name }}</option></select></label>
          <label class="block">Who is this for?<input v-model="guestLabel" maxlength="64" class="field mt-2" /></label>
          <label class="block">Access expires<select v-model.number="guestHours" class="field mt-2"><option :value="1">After one hour</option><option :value="24">After one day</option><option :value="168">After one week</option><option :value="720">After 30 days</option></select></label>
          <div v-if="guestTarget" class="space-y-2 text-sm">
            <p v-if="status.fips_address" class="break-all">FIPS address: <a :href="`https://[${status.fips_address}]:${guestTarget.port}/`" target="_blank" rel="noopener noreferrer" class="underline">https://[{{ status.fips_address }}]:{{ guestTarget.port }}/</a></p>
            <p v-if="selected.includes('public-web')">For your public reverse proxy, use the FIPS address above as its forward host and port {{ guestTarget.port }}, with upstream scheme HTTP. Keep the app gate enabled. Configure the application's public URL if it requires one.</p>
            <p>This creates permission to use the app. A reachable FIPS connection, onion service or configured public proxy is also needed.</p>
          </div>
          <button class="glass-button px-4 py-2" :disabled="!guestApp || !guestLabel.trim()" @click="createGuestAccess">Create app-only access</button>
          <div v-if="issuedAccess" class="rounded-xl border border-amber-300/30 p-4 space-y-2">
            <p>Copy this token and share it privately with the intended guest. It is shown once and is never posted to Nostr or another service.</p>
            <code class="block break-all select-all">{{ issuedAccess.token }}</code>
            <p class="text-sm">The guest opens the app address and chooses “Have an app-only access token?”. API clients can use it as an Authorization Bearer token. It cannot log in to the dashboard.</p>
            <button class="underline text-sm" @click="issuedAccess = null">Hide token</button>
          </div>
          <div v-for="grant in status.grants ?? []" :key="grant.id" class="flex flex-wrap items-center justify-between gap-3 border-t border-white/10 pt-3">
            <p>{{ grant.label }} · {{ grant.apps.join(', ') }} · {{ grant.expires_at ? new Date(grant.expires_at * 1000).toLocaleString() : 'No expiry' }}</p>
            <button class="underline text-sm" @click="revokeGuestAccess(grant.id)">Revoke access</button>
          </div>
        </section>
        <button class="glass-button px-5 py-3" :disabled="websiteMode && !current" @click="save">{{ websiteMode ? 'Save website draft and choices' : 'Save connection choices' }}</button>
        <section v-if="websiteMode && current && status.publication_enabled" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Publish the saved version on FIPS or your proxy</h2>
          <p class="text-sm text-white/60">Anyone who can reach this node through FIPS can view this website. Save your draft first. Scripts and external resources remain blocked in this first static-site version.</p>
          <label class="flex items-start gap-3"><input v-model="acknowledgeFips" type="checkbox" class="mt-1" /><span>I want the saved website to be visible to visitors on FIPS.</span></label>
          <button class="glass-button px-4 py-2" :disabled="!acknowledgeFips || (!current.routes.includes('fips') && !current.routes.includes('public-web')) || !current.draft" @click="setFipsPublication(true)">{{ current.fips_publication ? 'Publish saved update on FIPS' : 'Publish saved website on FIPS' }}</button>
          <button v-if="current.fips_publication" class="glass-button px-4 py-2 ml-2" @click="setFipsPublication(false)">Unpublish from FIPS</button>
          <div v-if="current.fips_publication" class="text-sm space-y-2">
            <p>{{ listener?.listening ? 'Local FIPS listener is ready.' : 'FIPS listener is not confirmed yet. Reload to check.' }}</p>
            <p v-if="listener?.error" role="alert">{{ listener.error }}</p>
            <p v-if="listener?.address" class="font-mono select-all break-all">{{ listener.address }}</p>
            <p class="text-amber-200">External access is not verified. The FIPS firewall must allow this website’s port, {{ current.fips_publication.port }}.</p>
          </div>
        </section>
        <section v-if="websiteMode && current && status.publication_enabled && selected.includes('tor')" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Publish the saved version on Tor</h2>
          <p class="text-sm text-white/60">Share an onion address without buying a domain. Anyone who knows the address can read the page in Tor Browser. Your node keeps the address keys when you unpublish.</p>
          <label class="flex items-start gap-3"><input v-model="acknowledgeTor" type="checkbox" class="mt-1" /><span>I want the saved website to be visible to visitors using Tor.</span></label>
          <button class="glass-button px-4 py-2" :disabled="!acknowledgeTor || !current.routes.includes('tor') || !current.draft" @click="setTorPublication(true)">{{ current.tor_publication ? 'Publish saved update on Tor' : 'Publish saved website on Tor' }}</button>
          <button v-if="current.tor_publication" class="glass-button px-4 py-2 ml-2" @click="setTorPublication(false)">Unpublish from Tor</button>
          <div v-if="current.tor_publication" class="text-sm space-y-2">
            <p v-if="onion?.error" role="alert">{{ onion.error }}</p>
            <p v-if="onion?.onion_address" class="font-mono select-all break-all">http://{{ onion.onion_address }}/</p>
            <p>{{ onion?.listening ? 'Local website listener is ready. Open the address in Tor Browser to check external access.' : 'Waiting for the website listener. Reload to check.' }}</p>
            <p class="text-amber-200">An address alone does not confirm that Tor has connected or that visitors can reach the page.</p>
          </div>
        </section>
        <section v-if="websiteMode && current && selected.includes('nostr')" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Publish a named nsite</h2>
          <p class="text-sm text-white/60">Upload a public static copy to a Blossom server, then announce it on your chosen Nostr relays. Your saved source stays on your node. A compatible nsite gateway can give it a browser address without buying a domain.</p>
          <button class="glass-button px-4 py-2" @click="loadIdentities">Load signing identities</button>
          <label class="block">Profile identity<select v-model="identityId" class="field mt-2"><option value="">Choose an identity</option><option v-for="identity in identities" :key="identity.id" :value="identity.id">{{ identity.name }}</option></select></label>
          <p class="text-xs text-white/50">The node's operational identity is excluded. Your private key stays in the existing signer.</p>
          <label class="block">Blossom server<input v-model="blossom" class="field mt-2" placeholder="https://your-blossom-server.example" /></label>
          <label class="block">Relays<textarea v-model="relays" rows="3" class="field mt-2" placeholder="wss://your-relay.example" /></label>
          <p class="text-sm text-white/60">Choose servers you trust or host your own. The Blossom server must allow browser uploads and reads. Paid storage requires a separate arrangement; this flow never pays automatically.</p>
          <button class="glass-button px-4 py-2" :disabled="!identityId || !blossom || !current.draft" @click="reviewNsite">Prepare publication review — stays on this node</button>
          <div v-if="nsiteReview" class="rounded-xl border border-amber-300/30 p-4 space-y-3">
            <h3 class="font-semibold">Review exactly what will leave your node</h3>
            <p class="text-sm">Signing identity: {{ nsiteReview.identity.name }} <span class="font-mono break-all">{{ nsiteReview.identity.nostr_pubkey }}</span></p>
            <p class="text-sm break-all">Upload destination: {{ nsiteReview.prepared.server }}</p>
            <p class="text-sm break-all">Announcement relays: {{ nsiteReview.relays.join(', ') }}</p>
            <p class="text-xs font-mono break-all">Content SHA-256: {{ nsiteReview.prepared.sha256 }}</p>
            <iframe :srcdoc="websitePreview(nsiteReview.prepared.html)" sandbox="" referrerpolicy="no-referrer" title="Exact nsite publication preview" class="w-full h-64 rounded-xl bg-white" />
            <details><summary>Inspect the exact uploaded HTML</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ nsiteReview.prepared.html }}</pre></details>
            <details><summary>Inspect the public manifest</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ JSON.stringify(nsiteReview.prepared.manifest, null, 2) }}</pre></details>
            <p class="text-sm text-amber-200">Check for personal information, credentials, private addresses and anything you do not want copied. Sanitising HTML does not remove sensitive text. Public copies cannot be guaranteed erased.</p>
            <label class="flex items-start gap-3"><input v-model="acknowledgeUpload" type="checkbox" class="mt-1" /><span>I approve sending these exact website bytes to this Blossom server.</span></label>
          </div>
          <label class="flex items-start gap-3"><input v-model="acknowledgeNostr" type="checkbox" class="mt-1" /><span>I approve sending the displayed manifest or removal request to the listed relays when I press its action button. It identifies the author, and copies may remain after a deletion request.</span></label>
          <button class="glass-button px-4 py-2" :disabled="!acknowledgeNostr || !acknowledgeUpload || !nsiteReview || !current.routes.includes('nostr') || !current.draft || !identityId || !blossom" @click="handleNsite('publish')">Upload and publish saved website</button>
          <template v-if="current.nsite_receipt">
            <p class="text-sm">{{ current.nsite_receipt.deletion_requested ? 'Deletion requested; copies may remain.' : current.nsite_receipt.accepted_relays.length ? 'Relay delivery recorded; gateway access not verified.' : 'Signed manifest retained; relay delivery is pending.' }}</p>
            <details><summary>Review retained manifest for retry or removal</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ JSON.stringify(current.nsite_receipt.event, null, 2) }}</pre></details>
            <p class="text-sm break-all">Retry destinations: {{ relays }}. Removal also contacts relays that previously accepted this manifest: {{ current.nsite_receipt.accepted_relays.join(', ') || 'none recorded' }}.</p>
            <button class="glass-button px-4 py-2" :disabled="!acknowledgeNostr || current.nsite_receipt.deletion_requested" @click="handleNsite('retry')">Retry manifest delivery</button>
            <button class="glass-button px-4 py-2 ml-2" :disabled="!acknowledgeNostr || !identityId" @click="handleNsite('delete')">Request removal from relays</button>
            <label class="block">Compatible nsite gateway<input v-model="gateway" class="field mt-2" placeholder="https://your-nsite-gateway.example" /></label>
            <button class="glass-button px-4 py-2" :disabled="!gateway" @click="showNsiteAddress">Show browser address</button>
            <a v-if="nsiteUrl" :href="nsiteUrl" target="_blank" rel="noopener noreferrer" class="block break-all underline">{{ nsiteUrl }}</a>
          </template>
        </section>
        <section v-if="websiteMode && current?.revisions.length" class="glass-card p-5 space-y-3">
          <h2 class="text-lg font-semibold">Saved revisions</h2>
          <div v-for="revision in [...current.revisions].reverse()" :key="revision.id" class="flex justify-between gap-3"><span class="text-sm text-white/60">{{ new Date(revision.created_at).toLocaleString() }}</span><button class="text-sm underline" @click="restore(revision.id)">Restore draft</button></div>
        </section>
      </fieldset>
    </template>
  </main>
</template>

<style scoped>
.field { display: block; width: 100%; border: 1px solid rgb(255 255 255 / .15); border-radius: .5rem; padding: .75rem; background: rgb(0 0 0 / .2); color: white; }
button:disabled { opacity: .5; cursor: not-allowed; }
th, td { padding: .5rem; }
</style>
