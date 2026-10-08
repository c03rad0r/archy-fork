<script setup lang="ts">
import { computed, onDeactivated, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { rpcClient } from '@/api/rpc-client'
import { installPublishingApp } from '@/services/installPublishingApp'
import { pendingWebsiteHtml } from '@/services/websiteImport'
import { publishing, PUBLISH_ROUTES, websitePreview } from '@/services/publishing'
import type { DnsPlan, HttpsCheck, PublishRoute, PublishingStatus, WebsiteProject } from '@/services/publishing'
import PublicWebGateway from './PublicWebGateway.vue'
import { nsiteIdentities, prepareNsite, publishNsite, retryNsite, requestNsiteDeletion, namedNsiteUrl, relayAddresses, storeLocalWebsite } from '@/services/nsitePublishing'
import type { NsiteIdentity, PreparedNsite } from '@/services/nsitePublishing'

import BackButton from '@/components/BackButton.vue'
import SetupWalkthrough from '@/components/SetupWalkthrough.vue'
import WebsiteArchiveSharing from '@/components/WebsiteArchiveSharing.vue'
import SearchableAppSelect from '@/components/SearchableAppSelect.vue'
import AIConnectionModal from '@/components/AIConnectionModal.vue'

const router = useRouter()
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
const showAiConnection = ref(false)
const aiConnection = ref<InstanceType<typeof AIConnectionModal>>()
function openAiCredit() { showAiConnection.value = true; aiConnection.value?.showRoutstr() }
const dns = ref<DnsPlan | null>(null)
const httpsCheck = ref<HttpsCheck | null>(null)
watch([projectId, hostname, destination, () => status.value?.state.version], () => { httpsCheck.value = null })
const acknowledgeFips = ref(false)
const acknowledgeTor = ref(false)
const identities = ref<NsiteIdentity[]>([])
const identityId = ref('')
const blossom = ref('')
const localNsite = ref(true)
const nsiteServer = computed(() => localNsite.value ? (current.value?.domain?.hostname ? `https://${current.value.domain.hostname}` : '') : blossom.value)
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
const walkthroughStep = ref('connections')
const walkthroughStarted = ref(false)
watch(websiteMode, async () => {
  issuedAccess.value = null
  walkthroughStarted.value = false
  walkthroughStep.value = 'connections'
  await refresh()
})
watch(pendingWebsiteHtml, value => {
  if (value !== null && websiteMode.value) walkthroughStep.value = 'design'
})
const walkthroughSteps = computed(() => websiteMode.value ? [
  { id: 'connections', title: 'Choose your connections', description: status.value?.state.connections.length ? 'Your saved connection choices are ready to reuse. You can change them for this website.' : 'Choose where visitors will find your website. You can use more than one connection.', complete: !!status.value?.state.connections.length },
  ...(!blossomInstalled.value ? [{ id: 'storage', title: 'Install local website storage', description: 'Optional: install Blossom to keep signed website files on this node.', complete: false }] : []),
  { id: 'design', title: 'Create your website', description: 'Describe a page, import it from AIUI, or edit your HTML. Preview and save it privately before publishing.', complete: !!current.value?.draft },
  ...(publicName.value ? [{ id: 'domain', title: 'Choose your address', description: 'Use your own domain, buy one privately, or use a compatible nsite gateway.', complete: !!current.value?.domain }] : []),
  { id: 'publish', title: 'Review and publish', description: 'Choose exactly what to share, publish each connection separately, and check that visitors can reach it.', complete: false },
] : [
  { id: 'connections', title: 'Choose your connections', description: 'Choose FIPS, public HTTPS or Tor. Save your preferences to reuse them when publishing a website.', complete: !!status.value?.state.connections.length },
  { id: 'sharing', title: 'Choose an app and grant access', description: 'Give a guest access to one supported app with an expiry date. Your dashboard stays private.', complete: false },
])

const routeHints: Record<PublishRoute, string> = {
  fips: 'For people connected to FIPS · No domain needed',
  'public-web': 'For ordinary browsers · Domain and gateway needed',
  tor: 'For Tor Browser · No domain needed',
  nostr: 'For nsite gateways · Public copies may remain',
}
const continueLabel = computed(() => walkthroughStep.value === 'storage' ? 'Continue without installing' : 'Save and continue')
const continueDisabled = computed(() => walkthroughStep.value === 'connections' ? !selected.value.length : ['design', 'domain'].includes(walkthroughStep.value) && (!current.value || !html.value.trim()))
async function continueSetup(next: string) {
  if (walkthroughStep.value === 'connections' && websiteMode.value && !current.value) await saveSharedConnections()
  else if (['connections', 'design', 'domain'].includes(walkthroughStep.value)) await save()
  if (!error.value) walkthroughStep.value = next
}
const publishedRoutes = computed(() => [current.value?.fips_publication ? 'FIPS' : '', current.value?.tor_publication ? 'Tor' : '', current.value?.nsite_receipt?.accepted_relays.length ? 'Nostr' : ''].filter(Boolean))

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
    if (!walkthroughStarted.value) {
      if (websiteMode.value && status.value.state.connections.length) {
        if (!current.value) selected.value = [...status.value.state.connections]
        walkthroughStep.value = blossomInstalled.value ? 'design' : 'storage'
      }
      walkthroughStarted.value = true
      if (websiteMode.value && pendingWebsiteHtml.value !== null) walkthroughStep.value = 'design'
    }
  })
}
async function create() {
  await perform(async () => {
    if (!status.value) return
    const choices = [...selected.value]
    const result = await publishing.update(status.value.state.version, { action: 'create', name: name.value })
    status.value.state = result.state
    if (result.project_id) { selectProject(result.state.projects[result.project_id]!); selected.value = choices }
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
async function saveSharedConnections() {
  await perform(async () => {
    if (!status.value) return
    const result = await publishing.update(status.value.state.version, { action: 'connections', routes: selected.value })
    status.value.state = result.state
    message.value = 'Connection preferences saved. Continue to create your website; nothing has been published.'
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
async function setArchiveSharing(route: 'fips' | 'tor', enable: boolean) {
  await perform(async () => {
    if (!status.value || !current.value) return
    const result = await publishing.update(status.value.state.version, enable
      ? { action: 'share-archive', id: projectId.value, route, acknowledge_public: true }
      : { action: 'unshare-archive', id: projectId.value, route })
    status.value.state = result.state
    message.value = enable ? 'This archived snapshot is available on the selected website connection. Other Blossom files remain private.' : 'File sharing stopped on this connection. Copies already downloaded may remain.'
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
async function installBlossom() {
  await perform(async () => {
    await installPublishingApp('blossom')
    message.value = 'Blossom installation requested through the app catalogue. This step will be skipped when installation is recorded.'
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
watch([projectId, blossom, localNsite, relays, identityId, html, selected], () => { nsiteReview.value = null; acknowledgeNostr.value = false; acknowledgeUpload.value = false }, { deep: true })
async function reviewNsite() {
  await perform(async () => {
    if (!status.value || !current.value) return
    const identity = identities.value.find(i => i.id === identityId.value)
    if (!identity) throw new Error('Choose a profile identity first')
    const targets = relayAddresses(relays.value)
    const prepared = await prepareNsite(projectId.value, status.value.state.version, nsiteServer.value, current.value.draft, localNsite.value)
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
async function unshareNsiteAsset() {
  await perform(async () => {
    if (!status.value || !current.value) return
    await publishing.update(status.value.state.version, { action: 'unshare-nsite-asset', id: projectId.value })
    status.value = await publishing.status()
    message.value = 'Local nsite file sharing stopped. Published manifests and downloaded copies may remain.'
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
  <main class="publishing-guide w-full min-w-0 pb-6">
    <BackButton label="Back to Setup" desktop-margin="mb-6" @click="router.push({ path: '/dashboard', query: { tab: 'setup' } })" />
    <div class="flex flex-wrap items-start justify-between gap-4 mb-8">
      <div><h1 class="text-3xl font-bold text-white mb-2 drop-shadow-[0_2px_8px_rgba(0,0,0,0.6)]">{{ websiteMode ? 'Publish a website' : 'Allow external connections' }}</h1>
        <p class="text-white/70">{{ websiteMode ? 'Create a website on your node and choose where people can find it.' : 'Choose how people will connect to selected services on your node.' }}</p></div>
      <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="refresh">Reload</button>
    </div>
    <p v-if="error" role="alert" class="rounded-xl p-4 mb-4 bg-red-500/10 text-red-200">{{ error }}</p>
    <p v-if="message" role="status" class="rounded-xl p-4 mb-4 bg-green-500/10 text-green-200">{{ message }}</p>
    <p v-if="busy" role="status" class="text-white/60 mb-4">Working…</p>
    <div v-if="websiteMode" class="flex flex-wrap items-center gap-2 text-xs text-white/65 mb-6">
      <span class="rounded-full border border-white/15 bg-white/5 px-3 py-1.5">{{ publishedRoutes.length ? `Published on ${publishedRoutes.join(', ')}` : 'Private until you publish' }}</span>
      <span v-if="blossomInstalled" class="rounded-full border border-green-400/20 bg-green-400/5 px-3 py-1.5 text-green-200">Blossom installed</span>
      <span v-if="status?.state.connections.length" class="rounded-full border border-white/15 bg-white/5 px-3 py-1.5">Saved connections ready to reuse</span>
    </div>
    <SetupWalkthrough v-if="status" v-model="walkthroughStep" :steps="walkthroughSteps" :busy="busy" :continue-label="continueLabel" :continue-disabled="continueDisabled" @next="continueSetup">
      <template #connections>
        <section class="space-y-4">
          <p class="text-sm text-white/65">Select all that apply. You can change these choices later.</p>
          <div class="grid gap-3 sm:grid-cols-2">
          <label v-for="option in routes" :key="option.id" class="connection-option flex items-start gap-3 rounded-xl border p-4 cursor-pointer transition-colors" :class="selected.includes(option.id) ? 'border-orange-300/50 bg-orange-400/10' : 'border-white/10 bg-white/[0.03] hover:bg-white/[0.07]'">
            <input v-model="selected" type="checkbox" :value="option.id" class="mt-1" />
            <span class="min-w-0"><span class="font-semibold text-white/95">{{ option.title }}</span><span class="block text-xs text-orange-100/75 mt-1">{{ routeHints[option.id] }}</span><span class="block text-sm text-white/60 mt-1">{{ option.description }}</span>
              <span v-if="websiteMode && status.state.connections.includes(option.id)" class="block text-xs text-amber-200 mt-2">Already selected in connection setup; reachability still needs verification.</span>
            </span>
          </label>
          </div>
          <p v-if="selected.includes('fips')" class="text-sm text-white/60">{{ status.fips_address ? 'Your node has a FIPS address. Check visitor access after publishing or sharing an app.' : 'Your node does not have a FIPS address yet. Connect FIPS in Network settings before publishing here.' }}</p>
          <PublicWebGateway v-if="!websiteMode && selected.includes('public-web') && status.gateway" :gateway="status.gateway" @refresh="refresh" />
          <RouterLink v-if="websiteMode" to="/dashboard/setup/external-access" class="inline-block text-sm underline">Manage shared connections</RouterLink>
        </section>
      </template>
      <template #storage>
        <section v-if="websiteMode" class="space-y-3" data-testid="blossom-setup">
          <h2 class="text-lg font-semibold">Local website files</h2>
          <template v-if="blossomInstalled">
            <p>Blossom is installed. You can skip installation.</p>
            <RouterLink to="/dashboard/apps/blossom" class="underline">Manage local Blossom</RouterLink>
            <template v-if="current">
              <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="loadIdentities">Choose a storage identity</button>
              <label class="block">Profile for local files<select v-model="identityId" :disabled="busy" class="field mt-2"><option value="">Choose an identity</option><option v-for="identity in identities" :key="identity.id" :value="identity.id">{{ identity.name }}</option></select></label>
              <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || !identityId || !current.draft || html !== current.draft" @click="storeLocally">Store saved website in local Blossom</button>
              <p v-if="html !== current.draft" class="text-sm">Save your edits before storing this version in Blossom.</p>
              <p v-if="current.local_archive" class="text-sm break-all">Verified local snapshot: {{ current.local_archive.size }} bytes · {{ new Date(current.local_archive.created_at).toLocaleString() }} · SHA-256 {{ current.local_archive.sha256 }}</p>
              <p class="text-sm text-white/60">This stores the saved draft shown below. Later edits need another explicit store. Local files require node login; this does not create a public Blossom endpoint.</p>
            </template>
          </template>
          <template v-else>
            <p>Install Blossom from the app catalogue to store website files on this node. Create a profile identity first; Blossom uses the normal Archipelago signer.</p>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="installBlossom">{{ busy ? 'Installing…' : 'Install Blossom' }}</button>
            <RouterLink to="/dashboard/marketplace/blossom" class="text-sm underline ml-3">View app details</RouterLink>
            <p class="text-sm text-white/60">Return here after installation. This step is optional for a simple HTML page served directly by the node.</p>
          </template>
          <p class="text-sm text-white/60">Installation and local uploads do not announce anything on Nostr. Publishing files externally requires a separate review of the content and destinations.</p>
        </section>
      </template>
      <template #design>
        <section class="space-y-4">
          <h2 class="text-lg font-semibold">Your AI connection</h2>
          <p class="text-sm text-white/60">Use Routstr by default, or choose Claude or OpenAI with your API key. Your selected provider creates the draft; you review it here before publishing.</p>
          <div class="flex flex-wrap items-center gap-3">
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="showAiConnection = true">Choose AI provider</button>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="openAiCredit">Routstr credit and top up</button>
          </div>
          <p class="text-xs text-white/50">Routstr uses your node’s ecash balance within the spending allowance you set. Topping up and changing the allowance are separate choices.</p>
        </section>
        <section v-if="websiteMode && pendingWebsiteHtml !== null" class="space-y-3">
          <h2 class="text-lg font-semibold">Continue from AIUI</h2>
          <p class="text-sm text-white/60">Create a new project from the HTML you selected in AIUI. Existing projects remain unchanged.</p>
          <div class="flex flex-wrap items-center gap-3">
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="importFromAiui">Import into a new website</button>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="pendingWebsiteHtml = null">Discard import</button>
          </div>
        </section>
        <section v-if="websiteMode" class="space-y-4">
          <h2 class="text-lg font-semibold">{{ projects.length ? 'Your websites' : 'Start with an idea' }}</h2>
          <div v-if="!current" class="space-y-3">
            <p class="text-sm text-white/65">Ask AIUI to make a simple HTML website. When its preview is ready, choose “Continue to website setup” to bring it here.</p>
            <RouterLink to="/dashboard/chat" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium">Create with AIUI <span aria-hidden="true">↗</span></RouterLink>
            <p class="text-sm text-white/50">Or give your website a name and start with a blank page below.</p>
          </div>
          <div v-if="projects.length" class="flex flex-wrap gap-2">
            <button v-for="p in projects" :key="p.id" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :aria-pressed="p.id === projectId" @click="selectProject(p)">{{ p.name }}</button>
          </div>
          <label class="block">Website name<input v-model="name" maxlength="100" class="field mt-2" /></label>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="create">Create another website</button>
        </section>
        <section v-if="websiteMode && current" class="space-y-4">
          <h2 class="text-lg font-semibold">Describe and preview</h2>
          <RouterLink to="/dashboard/chat" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium">Create with AIUI <span aria-hidden="true">↗</span></RouterLink>
          <p class="text-sm text-white/60">In AIUI, use “Continue to website setup” on your HTML preview to bring it back here for review.</p>
          <details class="guide-details"><summary>Edit or paste HTML</summary><div class="pt-4">
          <label class="block">Website HTML<textarea v-model="html" rows="8" class="field mt-2 font-mono text-xs" spellcheck="false" /></label>
          </div></details>
          <div class="flex items-center justify-between gap-2"><h3 class="font-medium">Your preview</h3><span class="text-xs text-white/50">Private preview</span></div>
          <iframe :srcdoc="preview" sandbox="" referrerpolicy="no-referrer" title="Isolated website preview" class="w-full h-96 rounded-xl bg-white" />
          <p class="text-xs text-white/50">Your preview stays private. This first version supports static pages; scripts and external resources are blocked.</p>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!html" @click="download">Download HTML</button>
        </section>
        <details v-if="websiteMode && blossomInstalled" class="guide-details" data-testid="blossom-setup"><summary>Keep a signed copy in local Blossom</summary><div class="space-y-3 pt-4">
          <h2 class="text-lg font-semibold">Local website files</h2>
          <template v-if="blossomInstalled">
            <p>Blossom is installed. You can skip installation.</p>
            <RouterLink to="/dashboard/apps/blossom" class="underline">Manage local Blossom</RouterLink>
            <template v-if="current">
              <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="loadIdentities">Choose a storage identity</button>
              <label class="block">Profile for local files<select v-model="identityId" :disabled="busy" class="field mt-2"><option value="">Choose an identity</option><option v-for="identity in identities" :key="identity.id" :value="identity.id">{{ identity.name }}</option></select></label>
              <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || !identityId || !current.draft || html !== current.draft" @click="storeLocally">Store saved website in local Blossom</button>
              <p v-if="html !== current.draft" class="text-sm">Save your edits before storing this version in Blossom.</p>
              <p v-if="current.local_archive" class="text-sm break-all">Verified local snapshot: {{ current.local_archive.size }} bytes · {{ new Date(current.local_archive.created_at).toLocaleString() }} · SHA-256 {{ current.local_archive.sha256 }}</p>
              <p class="text-sm text-white/60">This stores the saved draft shown below. Later edits need another explicit store. Local files require node login; this does not create a public Blossom endpoint.</p>
            </template>
          </template>
          <template v-else>
            <p>Install Blossom from the app catalogue to store website files on this node. Create a profile identity first; Blossom uses the normal Archipelago signer.</p>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="installBlossom">{{ busy ? 'Installing…' : 'Install Blossom' }}</button>
            <RouterLink to="/dashboard/marketplace/blossom" class="text-sm underline ml-3">View app details</RouterLink>
            <p class="text-sm text-white/60">Return here after installation. This step is optional for a simple HTML page served directly by the node.</p>
          </template>
          <p class="text-sm text-white/60">Installation and local uploads do not announce anything on Nostr. Publishing files externally requires a separate review of the content and destinations.</p>
        </div></details>
        <section v-if="websiteMode && current?.revisions.length" class="space-y-3">
          <h2 class="text-lg font-semibold">Saved revisions</h2>
          <div v-for="revision in [...current.revisions].reverse()" :key="revision.id" class="flex flex-wrap items-center gap-3"><span class="text-sm text-white/60">{{ new Date(revision.created_at).toLocaleString() }}</span><button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="restore(revision.id)">Restore draft</button></div>
        </section>
        <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || (websiteMode && !current && walkthroughStep !== 'connections')" @click="walkthroughStep === 'connections' && websiteMode && !current ? saveSharedConnections() : save()">{{ walkthroughStep === 'connections' ? 'Save connection choices' : 'Save website draft and choices' }}</button>
      </template>
      <template #domain>
        <section v-if="websiteMode && publicName" class="space-y-4">
          <h2 class="text-lg font-semibold">Your domain</h2>
          <p class="text-sm text-white/60">Use a domain you own, or buy one with Bitcoin or Lightning. FIPS and Tor addresses do not need a domain purchase.</p>
          <a href="https://mynymbox.io/domainregistration" target="_blank" rel="noopener noreferrer" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium">Buy a domain through Mynymbox ↗</a>
          <p class="text-xs text-white/50">Mynymbox is the registrant of record; you retain contractual control and transfer rights. Complete checkout yourself, then return here. No hosting purchase is needed.</p>
          <label class="block">Website hostname<input v-model="hostname" class="field mt-2" placeholder="www.yourdomain.com" /></label>
          <label class="block">Gateway hostname or public IP<input v-model="destination" class="field mt-2" placeholder="Use the destination supplied by your gateway" /></label>
          <p class="text-sm text-white/60">For a tunnel, point DNS at the public gateway. For a direct connection, use the node’s public IP. Do not use a home-network, FIPS or onion address for public web DNS.</p>
          <PublicWebGateway v-if="selected.includes('public-web') && status.gateway" :gateway="status.gateway" :project="current" @refresh="refresh" />
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
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="hostname !== current.domain?.hostname || !current.routes.includes('public-web')" @click="verifyHttps">Check public HTTPS</button>
            <p v-if="httpsCheck" class="text-sm text-green-200">Verified https://{{ httpsCheck.hostname }}/ at {{ new Date(httpsCheck.checked_at).toLocaleString() }}: valid TLS and exact published content. Checked from this node; also test from an outside device.</p>
            <p v-else class="text-sm text-amber-200">Public HTTPS has not been verified for these saved settings.</p>
          </div>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!hostname || !destination" @click="prepareDns">Show DNS instructions</button>
          <div v-if="dns" class="space-y-3">
            <p>In Mynymbox, open Domains → DNS Management → your domain → Manage records → Add Record.</p>
            <div class="overflow-x-auto"><table class="w-full text-sm text-left"><thead><tr><th>Type</th><th>Name</th><th>Value</th><th>TTL</th></tr></thead><tbody><tr v-for="record in dns.records" :key="record.name"><td>{{ record.record_type }}</td><td class="select-all">{{ record.name }}</td><td class="select-all">{{ record.value }}</td><td>{{ record.ttl }}</td></tr></tbody></table></div>
            <p v-for="note in dns.notes" :key="note" class="text-sm text-white/60">{{ note }}</p>
            <p class="text-amber-200 text-sm">Instructions prepared — DNS and HTTPS have not been verified.</p>
            <a href="https://mynymbox.io/docs?doc=domains/dns-records" target="_blank" rel="noopener noreferrer" class="underline text-sm">Mynymbox’s DNS guide ↗</a>
          </div>
        </section>
      </template>
      <template #sharing>
        <section v-if="!websiteMode" class="space-y-3">
          <h2 class="text-lg font-semibold">Grant access to an app</h2>
          <p v-if="!shareableApps.length" class="rounded-xl border border-white/10 bg-white/5 p-4 text-sm text-white/70">No installed apps currently support guest sharing. <RouterLink to="/dashboard/marketplace" class="underline">Browse apps</RouterLink></p>
          <p v-if="shareableApps.length" class="text-sm text-white/60">Give someone access to one application without sharing your dashboard login. Only apps that explicitly support guest sharing are offered. Their own account permissions still apply.</p>
          <template v-if="shareableApps.length">
          <SearchableAppSelect v-model="guestApp" :options="shareableApps" :disabled="busy" />
          <template v-if="guestTarget">
          <PublicWebGateway v-if="selected.includes('public-web') && status.gateway?.configured" :gateway="status.gateway" :app="guestTarget" @refresh="refresh" />
          <label class="block">Who is this for?<input v-model="guestLabel" maxlength="64" class="field mt-2" /></label>
          <label class="block">Access expires<select v-model.number="guestHours" class="field mt-2"><option :value="1">After one hour</option><option :value="24">After one day</option><option :value="168">After one week</option><option :value="720">After 30 days</option></select></label>
          <div v-if="guestTarget" class="space-y-2 text-sm">
            <p v-if="status.fips_address" class="break-all">FIPS address: <a :href="`https://[${status.fips_address}]:${guestTarget.port}/`" target="_blank" rel="noopener noreferrer" class="underline">https://[{{ status.fips_address }}]:{{ guestTarget.port }}/</a></p>
            <p v-if="selected.includes('public-web')">For your public reverse proxy, use the FIPS address above as its forward host and port {{ guestTarget.port }}, with upstream scheme HTTP. Keep the app gate enabled. Configure the application's public URL if it requires one.</p>
            <p>This creates permission to use the app. A reachable FIPS connection, onion service or configured public proxy is also needed.</p>
          </div>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!guestApp || !guestLabel.trim()" @click="createGuestAccess">Create app-only access</button>
          </template>
          </template>
          <div v-if="issuedAccess" class="rounded-xl border border-amber-300/30 p-4 space-y-2">
            <p>Copy this token and share it privately with the intended guest. It is shown once and is never posted to Nostr or another service.</p>
            <code class="block break-all select-all">{{ issuedAccess.token }}</code>
            <p class="text-sm">The guest opens the app address and chooses “Have an app-only access token?”. API clients can use it as an Authorization Bearer token. It cannot log in to the dashboard.</p>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="issuedAccess = null">Hide token</button>
          </div>
          <div v-for="grant in status.grants ?? []" :key="grant.id" class="flex flex-wrap items-center gap-3 border-t border-white/10 pt-3">
            <p>{{ grant.label }} · {{ grant.apps.join(', ') }} · {{ grant.expires_at ? new Date(grant.expires_at * 1000).toLocaleString() : 'No expiry' }}</p>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="revokeGuestAccess(grant.id)">Revoke access</button>
          </div>
        </section>
      </template>
      <template #publish>
        <section v-if="current?.draft" class="space-y-3">
          <div class="flex flex-wrap items-center justify-between gap-2"><h2 class="text-lg font-semibold">Ready to share?</h2><span class="text-xs text-white/55">{{ current.name }} · Saved version</span></div>
          <p class="text-sm text-white/65">Check the page below, then publish using your chosen connections. Nothing is sent just by opening this step.</p>
          <p v-if="html !== current.draft" class="text-sm text-amber-200">You have unsaved edits. This preview shows the saved version that will be published.</p>
          <iframe :srcdoc="websitePreview(current.draft)" sandbox="" referrerpolicy="no-referrer" title="Saved website publication preview" class="w-full h-64 rounded-xl bg-white" />
        </section>
        <p v-else class="text-sm text-white/65">Create and save your website first, then return here to publish it.</p>
        <section v-if="websiteMode && current && status.publication_enabled && (selected.includes('fips') || selected.includes('public-web') || current.fips_publication)" class="space-y-3">
          <h2 class="text-lg font-semibold">Publish from your node</h2>
          <p class="text-sm text-white/60">Visitors on FIPS can read this page. If you chose public web, your domain’s proxy uses this same publication.</p>
          <label class="flex items-start gap-3"><input v-model="acknowledgeFips" type="checkbox" class="mt-1" /><span>I want the saved website to be visible to visitors on FIPS.</span></label>
          <div class="flex flex-wrap items-center gap-3">
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!acknowledgeFips || (!current.routes.includes('fips') && !current.routes.includes('public-web')) || !current.draft" @click="setFipsPublication(true)">{{ current.fips_publication ? 'Publish saved update on FIPS' : 'Publish saved website on FIPS' }}</button>
          <button v-if="current.fips_publication" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="setFipsPublication(false)">Unpublish from FIPS</button>
          </div>
          <div v-if="current.fips_publication" class="text-sm space-y-2">
            <p>{{ listener?.listening ? 'Local FIPS listener is ready.' : 'FIPS listener is not confirmed yet. Reload to check.' }}</p>
            <p v-if="listener?.error" role="alert">{{ listener.error }}</p>
            <p v-if="listener?.address" class="font-mono select-all break-all">{{ listener.address }}</p>
            <p class="text-white/60">Open this address from another FIPS device to check visitor access.</p>
            <button v-if="selected.includes('public-web')" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="walkthroughStep = 'domain'">Connect or check my HTTPS domain</button>
            <WebsiteArchiveSharing v-if="status.public_archive_enabled" :publication="current.fips_publication" :archive="current.local_archive" :address="selected.includes('public-web') && current.domain?.hostname ? `https://${current.domain.hostname}/` : listener?.address" :busy="busy" @change="setArchiveSharing('fips', $event)" />
            <details class="text-xs text-white/50"><summary>Connection details</summary><p class="mt-2">Website port {{ current.fips_publication.port }}. A local listener does not confirm access from another device.</p></details>
          </div>
        </section>
        <section v-if="websiteMode && current && status.publication_enabled && (selected.includes('tor') || current.tor_publication)" class="space-y-3">
          <h2 class="text-lg font-semibold">Publish the saved version on Tor</h2>
          <p class="text-sm text-white/60">Share an onion address without buying a domain. Anyone who knows the address can read the page in Tor Browser. Your node keeps the address keys when you unpublish.</p>
          <label class="flex items-start gap-3"><input v-model="acknowledgeTor" type="checkbox" class="mt-1" /><span>I want the saved website to be visible to visitors using Tor.</span></label>
          <div class="flex flex-wrap items-center gap-3">
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!acknowledgeTor || !current.routes.includes('tor') || !current.draft" @click="setTorPublication(true)">{{ current.tor_publication ? 'Publish saved update on Tor' : 'Publish saved website on Tor' }}</button>
          <button v-if="current.tor_publication" class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="setTorPublication(false)">Unpublish from Tor</button>
          </div>
          <div v-if="current.tor_publication" class="text-sm space-y-2">
            <p v-if="onion?.error" role="alert">{{ onion.error }}</p>
            <p v-if="onion?.onion_address" class="font-mono select-all break-all">http://{{ onion.onion_address }}/</p>
            <p>{{ onion?.listening ? 'Local website listener is ready. Open the address in Tor Browser to check external access.' : 'Waiting for the website listener. Reload to check.' }}</p>
            <WebsiteArchiveSharing v-if="status.public_archive_enabled" :publication="current.tor_publication" :archive="current.local_archive" :address="onion?.onion_address ? `http://${onion.onion_address}/` : null" :busy="busy" @change="setArchiveSharing('tor', $event)" />
            <p class="text-amber-200">An address alone does not confirm that Tor has connected or that visitors can reach the page.</p>
          </div>
        </section>
        <section v-if="websiteMode && current && (selected.includes('nostr') || current.nsite_receipt)" class="space-y-3">
          <h2 class="text-lg font-semibold">Publish a named nsite</h2>
          <p class="text-sm text-white/60">Serve a reviewed static copy from your node or another Blossom server, then announce it on your chosen Nostr relays. Your saved source stays on your node. A compatible nsite gateway can give it a browser address without buying a domain.</p>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="loadIdentities">Load signing identities</button>
          <label class="block">Profile identity<select v-model="identityId" class="field mt-2"><option value="">Choose an identity</option><option v-for="identity in identities" :key="identity.id" :value="identity.id">{{ identity.name }}</option></select></label>
          <p class="text-xs text-white/50">The node's operational identity is excluded. Your private key stays in the existing signer.</p>
          <label class="flex items-start gap-3"><input v-model="localNsite" type="checkbox" class="mt-1" /><span>Serve the reviewed file from this node’s Blossom storage</span></label>
          <p v-if="localNsite" class="text-sm text-white/60">Publish and verify this website’s public HTTPS connection first. Only the reviewed file is shared; private Blossom files stay protected. Your node must stay online for nsite gateways to fetch it.</p>
          <p v-if="localNsite" class="text-sm break-all">File address: {{ nsiteServer || 'Set this website’s domain first' }}</p>
          <label v-else class="block">External Blossom server<input v-model="blossom" class="field mt-2" placeholder="https://your-blossom-server.example" /></label>
          <label class="block">Relays<textarea v-model="relays" rows="3" class="field mt-2" placeholder="wss://your-relay.example" /></label>
          <p class="text-sm text-white/60">Choose servers you trust or host your own. The Blossom server must allow browser uploads and reads. Paid storage requires a separate arrangement; this flow never pays automatically.</p>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!identityId || !nsiteServer || !current.draft" @click="reviewNsite">Prepare publication review — stays on this node</button>
          <div v-if="nsiteReview" class="rounded-xl border border-amber-300/30 p-4 space-y-3">
            <h3 class="font-semibold">Review exactly what will leave your node</h3>
            <p class="text-sm">Signing identity: {{ nsiteReview.identity.name }} <span class="font-mono break-all">{{ nsiteReview.identity.nostr_pubkey }}</span></p>
            <p class="text-sm break-all">{{ nsiteReview.prepared.local ? 'Public file origin' : 'Upload destination' }}: {{ nsiteReview.prepared.server }}</p>
            <p class="text-sm break-all">Announcement relays: {{ nsiteReview.relays.join(', ') }}</p>
            <p class="text-xs font-mono break-all">Content SHA-256: {{ nsiteReview.prepared.sha256 }}</p>
            <iframe :srcdoc="websitePreview(nsiteReview.prepared.html)" sandbox="" referrerpolicy="no-referrer" title="Exact nsite publication preview" class="w-full h-64 rounded-xl bg-white" />
            <details><summary>Inspect the exact uploaded HTML</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ nsiteReview.prepared.html }}</pre></details>
            <details><summary>Inspect the public manifest</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ JSON.stringify(nsiteReview.prepared.manifest, null, 2) }}</pre></details>
            <p class="text-sm text-amber-200">Check for personal information, credentials, private addresses and anything you do not want copied. Sanitising HTML does not remove sensitive text. Public copies cannot be guaranteed erased.</p>
            <label class="flex items-start gap-3"><input v-model="acknowledgeUpload" type="checkbox" class="mt-1" /><span>{{ localNsite ? 'I approve making these exact website bytes publicly readable from my node.' : 'I approve sending these exact website bytes to this Blossom server.' }}</span></label>
          </div>
          <label class="flex items-start gap-3"><input v-model="acknowledgeNostr" type="checkbox" class="mt-1" /><span>I approve sending the displayed manifest or removal request to the listed relays when I press its action button. It identifies the author, and copies may remain after a deletion request.</span></label>
          <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!acknowledgeNostr || !acknowledgeUpload || !nsiteReview || !current.routes.includes('nostr') || !current.draft || !identityId || !nsiteServer" @click="handleNsite('publish')">{{ localNsite ? 'Share file and publish manifest' : 'Upload and publish saved website' }}</button>
          <div v-if="current.fips_publication?.nsite_asset" class="space-y-2">
            <p class="text-sm break-all">Local nsite file is publicly readable: {{ current.fips_publication.nsite_asset.receipt.sha256 }}</p>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" @click="unshareNsiteAsset">Stop sharing the local nsite file</button>
          </div>
          <template v-if="current.nsite_receipt">
            <p class="text-sm">{{ current.nsite_receipt.deletion_requested ? 'Deletion requested; copies may remain.' : current.nsite_receipt.accepted_relays.length ? 'Relay delivery recorded; gateway access not verified.' : 'Signed manifest retained; relay delivery is pending.' }}</p>
            <details><summary>Review retained manifest for retry or removal</summary><pre class="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{{ JSON.stringify(current.nsite_receipt.event, null, 2) }}</pre></details>
            <p class="text-sm break-all">Retry destinations: {{ relays }}. Removal also contacts relays that previously accepted this manifest: {{ current.nsite_receipt.accepted_relays.join(', ') || 'none recorded' }}.</p>
          <div class="flex flex-wrap items-center gap-3">
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!acknowledgeNostr || current.nsite_receipt.deletion_requested" @click="handleNsite('retry')">Retry manifest delivery</button>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!acknowledgeNostr || !identityId" @click="handleNsite('delete')">Request removal from relays</button>
          </div>
            <label class="block">Compatible nsite gateway<input v-model="gateway" class="field mt-2" placeholder="https://your-nsite-gateway.example" /></label>
            <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="!gateway" @click="showNsiteAddress">Show browser address</button>
            <a v-if="nsiteUrl" :href="nsiteUrl" target="_blank" rel="noopener noreferrer" class="block break-all underline">{{ nsiteUrl }}</a>
          </template>
        </section>
      </template>
    </SetupWalkthrough>
    <AIConnectionModal ref="aiConnection" :show="showAiConnection" @close="showAiConnection = false" />
  </main>
</template>

<style scoped>
.field { display: block; width: 100%; border: 1px solid rgb(255 255 255 / .15); border-radius: .5rem; padding: .75rem; background: rgb(0 0 0 / .2); color: white; }
button:disabled { opacity: .5; cursor: not-allowed; }
th, td { padding: .5rem; }
.guide-details { border: 1px solid rgb(255 255 255 / .1); border-radius: .75rem; padding: 1rem; background: rgb(255 255 255 / .025); }
.guide-details > summary { cursor: pointer; color: rgb(255 255 255 / .8); font-size: .875rem; font-weight: 500; }
.connection-option input { accent-color: #fb923c; width: 1.125rem; height: 1.125rem; flex-shrink: 0; }
.field:focus-visible, summary:focus-visible { outline: 2px solid #fb923c; outline-offset: 3px; }

@media (prefers-reduced-motion: reduce) { .transition-colors { transition: none; } }
</style>
