<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'

const props = withDefaults(defineProps<{ expanded?: boolean; kind?: 'router' | 'phone' }>(), { expanded: false, kind: 'router' })
const emit = defineEmits<{ open: []; close: [] }>()

interface HardwareListing {
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
const DEV_ROUTER_LISTINGS: HardwareListing[] = [
  { id: 'dev-beryl-ax', pubkey: 'archipelago-demo-seller', title: 'GL.iNet Beryl AX (GL-MT3000)', summary: 'Wi‑Fi 6 travel router with OpenWrt, WireGuard and mesh-ready firmware.', content: '', location: 'London, UK', status: 'active', image: '/assets/img/router-mockups/beryl-ax.webp', price: { amount: '89', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-flint-2', pubkey: 'openwrt-demo-seller', title: 'GL.iNet Flint 2', summary: 'Dual 2.5G Ethernet OpenWrt router for a home gateway or node controller.', content: '', location: 'Berlin, DE', status: 'active', image: '/assets/img/router-mockups/flint-2.webp', price: { amount: '159', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-nano-pi', pubkey: 'community-demo-seller', title: 'NanoPi R5S OpenWrt gateway', summary: 'Compact RK3568 gateway with three Ethernet ports and community support.', content: '', location: 'Amsterdam, NL', status: 'active', image: '/assets/img/router-mockups/nanopi-r5s.webp', price: { amount: '74', currency: 'EUR' }, relay: 'development mock' },
]
const DEV_PHONE_LISTINGS: HardwareListing[] = [
  { id: 'dev-pixel-9', pubkey: 'privacy-demo-seller', title: 'Pixel 9 · GrapheneOS ready', summary: 'Unlocked Pixel with a long update runway and a fresh install path.', content: '', location: 'London, UK', status: 'active', image: '/assets/img/phone-mockups/pixel-9.webp', price: { amount: '499', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-pixel-9a', pubkey: 'mobile-demo-seller', title: 'Pixel 9a · unlocked', summary: 'Compact everyday Pixel. Check the bootloader can be unlocked before buying.', content: '', location: 'Berlin, DE', status: 'active', image: '/assets/img/phone-mockups/pixel-9a.webp', price: { amount: '339', currency: 'EUR' }, relay: 'development mock' },
  { id: 'dev-pixel-10', pubkey: 'phone-demo-seller', title: 'Pixel 10 · GrapheneOS ready', summary: 'A newer Pixel for private, independent mobile computing.', content: '', location: 'Amsterdam, NL', status: 'active', image: '/assets/img/phone-mockups/pixel-10.webp', price: { amount: '699', currency: 'EUR' }, relay: 'development mock' },
]
const devListings = computed(() => props.kind === 'phone' ? DEV_PHONE_LISTINGS : DEV_ROUTER_LISTINGS)
const topicTags = computed(() => props.kind === 'phone' ? ['pixel', 'grapheneos', 'privacy-phone'] : ['openwrt', 'router', 'gl-inet', 'beryl', 'flint'])
const open = ref(false)
const carousel = ref<HTMLElement>()
const listings = ref<HardwareListing[]>([])
const loading = ref(false)
const error = ref('')
let requestId = 0
let sockets: WebSocket[] = []

function tag(event: { tags?: string[][] }, name: string): string {
  return event.tags?.find(item => item[0] === name)?.[1]?.trim() ?? ''
}
function imageTag(event: { tags?: string[][] }): string {
  return event.tags?.find(item => item[0] === 'image')?.[1]?.trim() ?? ''
}
function matchesHardware(item: { title: string; summary: string; content: string; tags?: string[][] }) {
  const tags = item.tags?.flat().join(' ') ?? ''
  const text = `${item.title} ${item.summary} ${item.content} ${tags}`
  return props.kind === 'phone'
    ? /\bpixel\b|grapheneos/i.test(text)
    : /openwrt|gl[- .]?inet|beryl|flint|nanopi|gl-mt3000|gl-mt6000/i.test(text)
}
function parse(event: {
  id?: string; pubkey?: string; content?: string; tags?: string[][]
}, relay: string): HardwareListing | null {
  if (!event.id || !event.pubkey || !matchesHardware({ title: tag(event, 'title'), summary: tag(event, 'summary'), content: event.content ?? '', tags: event.tags })) return null
  const price = event.tags?.find(item => item[0] === 'price')
  return {
    id: event.id,
    pubkey: event.pubkey,
    title: tag(event, 'title') || (props.kind === 'phone' ? 'Pixel phone' : 'OpenWrt router'),
    summary: tag(event, 'summary') || (event.content ?? '').split('\n')[0]?.slice(0, 150) || 'Hardware listing',
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
  if (run !== requestId) return
  if (!relayUrls.value.length) {
    if (import.meta.env.DEV) {
      listings.value = devListings.value
      loading.value = false
      return
    }
    loading.value = false
    error.value = 'No Archipelago Nostr relays are configured. Add a relay in Settings → Nostr Relays and try again.'
    return
  }
  const found = new Map<string, HardwareListing>()
  await Promise.all(relayUrls.value.map(relay => new Promise<void>(resolve => {
    let settled = false
    const finish = () => { if (!settled) { settled = true; resolve() } }
    let socket: WebSocket
    try { socket = new WebSocket(relay) } catch { finish(); return }
    sockets.push(socket)
    const timer = window.setTimeout(() => { socket.close(); finish() }, 8000)
    socket.addEventListener('open', () => {
      socket.send(JSON.stringify(['REQ', `archy-${props.kind}-${run}`, {
        kinds: [30402],
        '#t': topicTags.value,
        limit: 100,
      }, {
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
  listings.value = found.size ? [...found.values()] : import.meta.env.DEV ? devListings.value : []
  loading.value = false
  if (!listings.value.length) error.value = `No active ${props.kind === 'phone' ? 'Pixel phone' : 'OpenWrt router'} listings found on your configured relays.`
}
function openShop() {
  open.value = true
  if (import.meta.env.DEV) listings.value = devListings.value
  emit('open')
  void search()
}
function closeShop() { requestId++; open.value = false; closeSockets(); emit('close') }
function moveCarousel(direction: number) {
  const list = carousel.value
  const card = list?.querySelector<HTMLElement>('.router-marketplace__card')
  if (list && card) list.scrollBy({ left: direction * (card.offsetWidth + 16), behavior: 'smooth' })
}
function formatPrice(item: HardwareListing) {
  if (!item.price) return 'Price on request'
  return `${item.price.amount} ${item.price.currency.toUpperCase()}${item.price.frequency ? ` / ${item.price.frequency}` : ''}`
}
onBeforeUnmount(() => { requestId++; closeSockets() })
</script>

<template>
  <section class="router-marketplace lg:mt-0" :class="{ 'router-marketplace--expanded': props.expanded }" aria-labelledby="router-marketplace-title">
    <div class="glass-card p-5 md:p-6">
      <template v-if="!open">
        <p class="text-xs uppercase tracking-[.18em] text-white/40">{{ props.kind === 'phone' ? 'Private mobile' : 'OpenWrt gateway' }}</p>
        <h2 id="router-marketplace-title" class="text-base font-semibold text-white mt-1">{{ props.kind === 'phone' ? 'Ready for a phone that answers to you?' : 'Not got an OpenWrt router?' }}</h2>
        <p class="text-sm text-white/55 mt-2 max-w-2xl">{{ props.kind === 'phone' ? 'Choose an unlockable Pixel. Install GrapheneOS. Lock the bootloader for verified boot.' : 'Browse community-published router listings from Nostr marketplaces. We’ll add Archipelago checkout after discovery is in place.' }}</p>
        <button type="button" class="glass-button router-marketplace__cta mt-4" @click="openShop">
          {{ props.kind === 'phone' ? 'Buy phone' : 'Buy router' }}
          <span aria-hidden="true">→</span>
        </button>
      </template>
      <template v-else>
      <div class="flex flex-wrap items-start justify-between gap-3 mb-2">
        <div>
          <h2 id="router-marketplace-title" class="text-base font-semibold text-white">{{ props.kind === 'phone' ? 'Pixel phones for sale' : 'OpenWrt routers for sale' }}</h2>
          <p class="text-xs text-white/50 mt-1">{{ listings[0]?.relay === 'development mock' ? 'Development preview · conceptual images and sample prices' : 'Live NIP-99 listings from Archipelago relays.' }}</p>
        </div>
        <div class="flex items-center gap-2">
          <button type="button" class="router-marketplace__nav" :aria-label="`Previous ${props.kind} listing`" @click="moveCarousel(-1)">‹</button>
          <button type="button" class="router-marketplace__nav" :aria-label="`Next ${props.kind} listing`" @click="moveCarousel(1)">›</button>
          <button type="button" class="text-xs text-white/45 hover:text-white ml-2" @click="closeShop">Close</button>
        </div>
      </div>
      <div v-if="loading && !listings.length" class="sr-only" aria-live="polite">Loading {{ props.kind }} listings</div>
      <div v-else-if="error && !listings.length" class="text-sm text-white/50 py-5 text-center">{{ error }}</div>
      <div v-else ref="carousel" class="router-marketplace__results" tabindex="0" :aria-label="`${props.kind === 'phone' ? 'Phone' : 'Router'} listings carousel`">
        <article v-for="item in listings" :key="item.id" class="router-marketplace__card">
          <img v-if="item.image" :src="item.image" :alt="item.title" class="router-marketplace__image" loading="lazy" />
          <div v-else class="router-marketplace__image router-marketplace__image--empty" aria-hidden="true">⌁</div>
          <div class="router-marketplace__details">
            <h3 class="text-sm font-medium text-white truncate">{{ item.title }}</h3>
            <p class="text-xs text-white/55 mt-1 line-clamp-1">{{ item.summary }}</p>
            <div class="flex flex-wrap gap-x-3 gap-y-1 mt-3 text-[11px] text-white/40">
              <span v-if="item.location">{{ item.location }}</span>
              <span>seller {{ item.pubkey.slice(0, 8) }}…</span>
            </div>
            <div class="router-marketplace__footer">
              <strong class="router-marketplace__price">{{ formatPrice(item) }}</strong>
              <div class="router-marketplace__actions">
                <button type="button" class="router-marketplace__action" disabled title="Listing details are next">View</button>
                <button type="button" class="router-marketplace__action router-marketplace__action--buy" disabled title="Checkout is the next step">Buy</button>
              </div>
            </div>
          </div>
        </article>
      </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.router-marketplace { margin-top: 16px; }
.router-marketplace--expanded { grid-column: 1 / -1; width: 100%; min-width: 0; height: 100%; min-height: 0; margin-top: 0; }
.router-marketplace--expanded > .glass-card { height: 100%; min-height: 0; display: flex; flex-direction: column; padding: 14px; }
.router-marketplace--expanded .router-marketplace__results { flex: 1 1 0; min-height: 0; }
.router-marketplace__cta { display: inline-flex; align-items: center; gap: 10px; }
.router-marketplace__nav { display: grid; place-items: center; width: 26px; height: 26px; border: 1px solid rgba(255,255,255,.14); border-radius: 50%; color: rgba(255,255,255,.72); font-size: 18px; line-height: 1; }
.router-marketplace__nav:hover { background: rgba(255,255,255,.09); }
@media (max-width: 1023px) {
  .router-marketplace__cta { width: 100%; justify-content: center; }
  /* The dashboard's fixed mobile Back bar must not cover card actions. */
  .router-marketplace--expanded > .glass-card { padding-bottom: 64px; }
}
.router-marketplace__results { display: flex; align-items: stretch; gap: 16px; overflow-x: auto; overflow-y: hidden; overscroll-behavior-x: contain; scroll-snap-type: x mandatory; scrollbar-width: none; mask-image: linear-gradient(to right, #000 0%, #000 calc(100% - 52px), transparent 100%); }
.router-marketplace__results::-webkit-scrollbar { display: none; }
.router-marketplace__card { display: flex; flex: 0 0 88%; scroll-snap-align: start; flex-direction: column; gap: 12px; min-width: 0; min-height: 0; padding: 12px; border: 1px solid rgba(255,255,255,.09); border-radius: 11px; background: rgba(255,255,255,.035); }
.router-marketplace__details { display: flex; flex: 0 0 auto; min-width: 0; flex-direction: column; }
.router-marketplace__footer { margin-top: auto; padding-top: 8px; }
.router-marketplace__price { display: block; color: rgba(180,244,235,.9); font-size: 18px; }
.router-marketplace__actions { display: flex; gap: 8px; margin-top: 8px; }
.router-marketplace__action { flex: 1; padding: 8px 12px; border: 1px solid rgba(255,255,255,.14); border-radius: 8px; color: rgba(255,255,255,.55); font-size: 12px; }
.router-marketplace__action--buy { border-color: rgba(118,230,191,.35); color: rgba(180,244,235,.9); }
@media (min-width: 768px) {
  .router-marketplace__card { flex-basis: min(360px, 44%); }
}
.router-marketplace__image { width: 100%; height: 0; min-height: 0; flex: 1 1 0; object-fit: contain; border-radius: 8px; background: linear-gradient(135deg, rgba(112,214,205,.18), rgba(255,255,255,.04)); }
.router-marketplace__image--empty { display: grid; place-items: center; color: rgba(121,205,215,.6); font-size: 34px; }
</style>
