<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { rpcClient } from '@/api/rpc-client'

const props = withDefaults(defineProps<{ expanded?: boolean }>(), { expanded: false })
const emit = defineEmits<{ open: []; close: [] }>()

interface RouterListing {
  id: string
  pubkey: string
  title: string
  summary: string
  content: string
  location: string
  status: string
  image: string
  price?: { amount: string; currency: string; frequency?: string }
  relay: string
}

const relayUrls = ref<string[]>([])
const DEV_LISTINGS: RouterListing[] = [
  { id: 'dev-beryl-ax', pubkey: 'archipelago-demo-seller', title: 'GL.iNet Beryl AX (GL-MT3000)', summary: 'Wi‑Fi 6 travel router with OpenWrt, WireGuard and mesh-ready firmware.', content: '', location: 'London, UK', status: 'active', image: '', price: { amount: '€89', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-flint-2', pubkey: 'openwrt-demo-seller', title: 'GL.iNet Flint 2', summary: 'Dual 2.5G Ethernet OpenWrt router for a home gateway or node controller.', content: '', location: 'Berlin, DE', status: 'active', image: '', price: { amount: '€159', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-nano-pi', pubkey: 'community-demo-seller', title: 'NanoPi R5S OpenWrt gateway', summary: 'Compact RK3568 gateway with three Ethernet ports and community support.', content: '', location: 'Amsterdam, NL', status: 'active', image: '', price: { amount: '€74', currency: 'EUR' }, relay: 'development mock' },
]
const open = ref(false)
const query = ref('')
const listings = ref<RouterListing[]>([])
const loading = ref(false)
const error = ref('')
let requestId = 0
let sockets: WebSocket[] = []

const filtered = computed(() => {
  const term = query.value.trim().toLowerCase()
  if (!term) return listings.value
  return listings.value.filter(item => `${item.title} ${item.summary} ${item.content} ${item.location}`.toLowerCase().includes(term))
})

function tag(event: { tags?: string[][] }, name: string): string {
  return event.tags?.find(item => item[0] === name)?.[1]?.trim() ?? ''
}
function imageTag(event: { tags?: string[][] }): string {
  return event.tags?.find(item => item[0] === 'image')?.[1]?.trim() ?? ''
}
function isRouter(item: { title: string; summary: string; content: string; location?: string; tags?: string[][] }) {
  const tags = item.tags?.flat().join(' ') ?? ''
  return /router|openwrt|wifi|gateway|gl[- ]?inet|travel router|mesh/i.test(`${item.title} ${item.summary} ${item.content} ${item.location ?? ''} ${tags}`)
}
function parse(event: {
  id?: string; pubkey?: string; content?: string; tags?: string[][]
}, relay: string): RouterListing | null {
  if (!event.id || !event.pubkey || !isRouter({ title: tag(event, 'title'), summary: tag(event, 'summary'), content: event.content ?? '', location: '', tags: event.tags })) return null
  const price = event.tags?.find(item => item[0] === 'price')
  return {
    id: event.id,
    pubkey: event.pubkey,
    title: tag(event, 'title') || 'OpenWrt router',
    summary: tag(event, 'summary') || (event.content ?? '').split('\n')[0]?.slice(0, 150) || 'Router listing',
    content: event.content ?? '',
    location: tag(event, 'location'),
    status: tag(event, 'status') || 'active',
    image: imageTag(event),
    price: price?.[1] && price[2] ? { amount: price[1], currency: price[2], frequency: price[3] } : undefined,
    relay,
  }
}
function closeSockets() {
  sockets.forEach(socket => socket.close())
  sockets = []
}
async function search() {
  const run = ++requestId
  closeSockets(); loading.value = true; error.value = ''
  try {
    const configured = await rpcClient.call<{ relays?: Array<{ url?: string; enabled?: boolean }> }>({ method: 'nostr.list-relays', params: {} })
    relayUrls.value = (configured.relays ?? [])
      .filter(relay => relay.enabled !== false && typeof relay.url === 'string' && /^wss?:\/\//.test(relay.url))
      .map(relay => relay.url!.replace(/\/$/, ''))
  } catch {
    relayUrls.value = []
  }
  if (!relayUrls.value.length) {
    loading.value = false
    error.value = 'No Archipelago Nostr relays are configured. Add a relay in Settings → Nostr Relays and try again.'
    return
  }
  const found = new Map<string, RouterListing>()
  await Promise.all(relayUrls.value.map(relay => new Promise<void>(resolve => {
    let settled = false
    const finish = () => { if (!settled) { settled = true; resolve() } }
    let socket: WebSocket
    try { socket = new WebSocket(relay) } catch { finish(); return }
    sockets.push(socket)
    const timer = window.setTimeout(() => { socket.close(); finish() }, 8000)
    socket.addEventListener('open', () => {
      socket.send(JSON.stringify(['REQ', `archy-router-${run}`, {
        kinds: [30402],
        limit: 200,
      }]))
    })
    socket.addEventListener('message', message => {
      try {
        const packet = JSON.parse(String(message.data)) as [string, string, { id?: string; pubkey?: string; content?: string; tags?: string[][] }]
        if (packet[0] === 'EVENT') {
          const item = parse(packet[2], relay)
          if (item && item.status !== 'sold') found.set(item.id, item)
        }
        if (packet[0] === 'EOSE') { window.clearTimeout(timer); socket.close(); finish() }
      } catch { /* Ignore malformed relay frames. */ }
    })
    socket.addEventListener('error', () => { window.clearTimeout(timer); finish() })
    socket.addEventListener('close', () => { window.clearTimeout(timer); finish() })
  })))
  if (run !== requestId) return
  listings.value = [...found.values()]
  if (!listings.value.length && import.meta.env.DEV) listings.value = DEV_LISTINGS
  loading.value = false
  if (!listings.value.length) error.value = 'No active OpenWrt router listings answered from the configured public relays.'
}
function openShop() {
  open.value = true
  query.value = 'openwrt'
  emit('open')
  if (!listings.value.length) void search()
}
function closeShop() { open.value = false; closeSockets(); emit('close') }
function formatPrice(item: RouterListing) {
  if (!item.price) return 'Price on request'
  return `${item.price.amount} ${item.price.currency.toUpperCase()}${item.price.frequency ? ` / ${item.price.frequency}` : ''}`
}
watch(query, () => { if (open.value && query.value.trim().length >= 3) void search() })
onBeforeUnmount(closeSockets)
</script>

<template>
  <section class="router-marketplace lg:mt-0" :class="{ 'router-marketplace--expanded': props.expanded }" aria-labelledby="router-marketplace-title">
    <div class="glass-card p-5 md:p-6">
      <template v-if="!open">
        <p class="text-xs uppercase tracking-[.18em] text-white/40">OpenWrt gateway</p>
        <h2 id="router-marketplace-title" class="text-base font-semibold text-white mt-1">Not got an OpenWrt router?</h2>
        <p class="text-sm text-white/55 mt-2 max-w-2xl">Browse community-published router listings from Nostr marketplaces. We’ll add Archipelago checkout after discovery is in place.</p>
        <button type="button" class="glass-button router-marketplace__cta mt-4" @click="openShop">
          Buy router
          <span aria-hidden="true">→</span>
        </button>
      </template>
      <template v-else>
      <div class="flex flex-wrap items-start justify-between gap-3 mb-4">
        <div>
          <p class="text-xs uppercase tracking-[.18em] text-white/40">Nostr classifieds</p>
          <h2 id="router-marketplace-title" class="text-base font-semibold text-white mt-1">OpenWrt routers for sale</h2>
          <p class="text-xs text-white/50 mt-1">Live NIP-99 listings from Archipelago relays.</p>
        </div>
        <button type="button" class="text-xs text-white/45 hover:text-white" @click="closeShop">Close</button>
      </div>
      <div v-if="loading && !filtered.length" class="sr-only" aria-live="polite">Loading router listings</div>
      <div v-else-if="error && !filtered.length" class="text-sm text-white/50 py-5 text-center">{{ error }}</div>
      <div v-else class="router-marketplace__results" tabindex="0" aria-label="Router listings carousel">
        <article v-for="item in filtered" :key="item.id" class="router-marketplace__card">
          <img v-if="item.image" :src="item.image" :alt="item.title" class="router-marketplace__image" loading="lazy" />
          <div v-else class="router-marketplace__image router-marketplace__image--empty" aria-hidden="true">⌁</div>
          <div class="min-w-0 flex-1">
            <h3 class="text-sm font-medium text-white truncate">{{ item.title }}</h3>
            <p class="text-xs text-white/55 mt-1 line-clamp-2">{{ item.summary }}</p>
            <div class="flex flex-wrap gap-x-3 gap-y-1 mt-3 text-[11px] text-white/40">
              <span class="text-cyan-200/80">{{ formatPrice(item) }}</span>
              <span v-if="item.location">{{ item.location }}</span>
              <span>seller {{ item.pubkey.slice(0, 8) }}…</span>
            </div>
            <button type="button" class="mt-3 text-xs text-white/55 border border-white/10 rounded-md px-2.5 py-1.5 cursor-not-allowed" disabled title="Checkout is the next step">Continue in Archipelago</button>
          </div>
        </article>
      </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.router-marketplace { margin-top: 16px; }
.router-marketplace--expanded { grid-column: 1 / -1; margin-top: 0; }
.router-marketplace__cta { display: inline-flex; align-items: center; gap: 10px; }
@media (max-width: 1023px) {
  .router-marketplace__cta { width: 100%; justify-content: center; }
}
.router-marketplace__results { display: flex; gap: 16px; overflow-x: auto; overscroll-behavior-x: contain; scroll-snap-type: x mandatory; scrollbar-width: thin; padding-bottom: 4px; }
.router-marketplace__card { display: flex; flex: 0 0 100%; scroll-snap-align: start; gap: 12px; min-width: 0; padding: 12px; border: 1px solid rgba(255,255,255,.09); border-radius: 11px; background: rgba(255,255,255,.035); }
@media (min-width: 768px) {
  .router-marketplace__card { flex-basis: calc((100% - 16px) / 2); }
}
.router-marketplace__image { width: 64px; height: 64px; object-fit: cover; border-radius: 8px; flex-shrink: 0; background: rgba(255,255,255,.05); }
.router-marketplace__image--empty { display: grid; place-items: center; color: rgba(121,205,215,.6); font-size: 34px; }
</style>
