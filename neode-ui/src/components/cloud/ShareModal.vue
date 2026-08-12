<template>
  <Teleport to="body">
    <div class="share-modal-backdrop" @click.self="$emit('close')">
      <div class="share-modal glass-card">
        <!-- Header -->
        <div class="flex items-center justify-between mb-5">
          <div class="flex items-center gap-3">
            <div class="w-9 h-9 rounded-lg bg-orange-500/15 flex items-center justify-center">
              <svg class="w-5 h-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z" />
              </svg>
            </div>
            <div>
              <h3 class="text-base font-semibold text-white">Share with Peers</h3>
              <p class="text-xs text-white/50 truncate max-w-[200px]">{{ filename }}</p>
            </div>
          </div>
          <button class="share-modal-close" @click="$emit('close')">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- Share Toggle -->
        <div class="share-modal-row">
          <div class="flex-1">
            <p class="text-sm font-medium text-white/90">Share this {{ isDir ? 'folder' : 'file' }}</p>
            <p class="text-xs text-white/50 mt-0.5">Make visible to connected peers</p>
          </div>
          <ToggleSwitch v-model="shared" />
        </div>

        <!-- Access Type (only when shared) -->
        <div v-if="shared" class="mt-4 space-y-3">
          <p class="text-xs font-medium text-white/60 uppercase tracking-wider">Access Type</p>
          <div class="share-access-options">
            <button
              class="share-access-option"
              :class="{ 'share-access-option-active': accessType === 'free' }"
              @click="accessType = 'free'"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span class="text-sm font-medium">Free</span>
              <span class="text-xs text-white/40">Open access</span>
            </button>
            <button
              class="share-access-option"
              :class="{ 'share-access-option-active': accessType === 'peers_only' }"
              @click="accessType = 'peers_only'"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
              </svg>
              <span class="text-sm font-medium">Peers Only</span>
              <span class="text-xs text-white/40">Authenticated</span>
            </button>
            <button
              class="share-access-option"
              :class="{ 'share-access-option-active': accessType === 'paid' }"
              @click="accessType = 'paid'"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              <span class="text-sm font-medium">Paid</span>
              <span class="text-xs text-white/40">Earn sats</span>
            </button>
          </div>

          <!-- Price Input (only for paid) -->
          <div v-if="accessType === 'paid'" class="share-price-input-wrap">
            <div class="share-price-icon">
              <svg class="w-4 h-4 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
            </div>
            <input
              v-model.number="priceSats"
              type="number"
              min="1"
              max="1000000"
              placeholder="Price in sats"
              class="share-price-input"
            />
            <span class="share-price-unit">sats</span>
          </div>

          <!-- Accepted payment methods (only for paid) — gated on what this
               node can actually receive; unavailable rails are disabled with
               an ⓘ that explains how to enable them. -->
          <div v-if="accessType === 'paid'" class="mt-3">
            <p class="text-xs font-medium text-white/60 uppercase tracking-wider mb-2">Payments you accept</p>
            <div class="space-y-2">
              <div v-for="m in PAY_METHODS" :key="m.key" class="share-modal-row">
                <div class="flex-1 flex items-center gap-2 min-w-0">
                  <p class="text-sm text-white/90">{{ m.label }}</p>
                  <span v-if="capability[m.key] === undefined" class="text-[10px] text-white/40">checking…</span>
                  <button
                    v-else-if="!capability[m.key]"
                    class="w-4 h-4 rounded-full bg-white/10 text-white/60 hover:text-white text-[10px] leading-4 text-center shrink-0"
                    title="Why is this unavailable?"
                    @click="adviceFor = m.key"
                  >i</button>
                </div>
                <ToggleSwitch
                  :model-value="acceptedSet.has(m.key)"
                  :disabled="!capability[m.key]"
                  :aria-label="`Accept ${m.label}`"
                  @update:model-value="toggleMethod(m.key, $event)"
                />
              </div>
            </div>
            <p v-if="acceptedSet.size === 0" class="text-xs text-red-400 mt-2">
              Pick at least one payment method buyers can use.
            </p>
          </div>
        </div>

        <!-- Advice modal for an unavailable payment method -->
        <div v-if="adviceFor" class="mt-4 p-3 rounded-lg bg-white/5 border border-white/10">
          <div class="flex items-center justify-between mb-1">
            <p class="text-sm font-medium text-white">{{ PAY_METHODS.find(m => m.key === adviceFor)?.label }} isn't ready on this node</p>
            <button class="text-white/50 hover:text-white text-xs" @click="adviceFor = null">Dismiss</button>
          </div>
          <ul class="text-xs text-white/60 list-disc pl-4 space-y-1">
            <li v-for="line in adviceLines[adviceFor] || []" :key="line">{{ line }}</li>
          </ul>
        </div>

        <!-- Status messages -->
        <div v-if="saving" class="share-modal-status mt-4">
          <div class="w-4 h-4 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
          <span class="text-sm text-white/60">Saving...</span>
        </div>
        <div v-if="errorMsg" class="share-modal-error mt-4">
          <span class="text-sm text-red-400">{{ errorMsg }}</span>
        </div>
        <div v-if="successMsg" class="share-modal-success mt-4">
          <svg class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
          </svg>
          <span class="text-sm text-green-400">{{ successMsg }}</span>
        </div>

        <!-- Action -->
        <div class="flex justify-end gap-3 mt-5">
          <button class="glass-button px-4 py-2 rounded-lg text-sm" @click="$emit('close')">Cancel</button>
          <button
            class="glass-button px-5 py-2 rounded-lg text-sm font-medium share-modal-save"
            :disabled="saving || (shared && accessType === 'paid' && (!priceSats || priceSats < 1 || acceptedSet.size === 0))"
            @click="save"
          >
            {{ shared ? 'Share' : 'Stop Sharing' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script lang="ts">
/**
 * Extension → MIME map used when sharing a file that isn't already in the
 * catalog (`save()` below, `content.add`). Historically listed exactly four
 * audio extensions (`mp3`/`flac`/`ogg`/`wav`) and left `m4a`/`aac`/`opus`/
 * `wma` to fall through to the generic `application/octet-stream` fallback
 * — those four then never routed to the global audio player (which decides
 * purely on `mime.startsWith('audio/')`, see `usePaidItemViewer.ts`) and
 * auto-filed to Documents instead of Music (`content.rs`'s paid-download
 * auto-filing, same `starts_with("audio/")` check) — 13-CONTEXT.md's
 * landmine, T-13-72. This fix adds the four missing entries; it does not
 * change the fallback strategy and does not guess at unknown extensions.
 *
 * Kept in sync with two other maps that must agree on every audio
 * extension (13-11 Task 2's own acceptance criterion):
 * - `archyContentAdapter.ts`'s `classifyByMime`/`AUDIO_EXT_FALLBACK` (13-06)
 * - `content.rs`'s auto-filing check (`mime_type.starts_with("audio/")`) —
 *   that check is prefix-only, so any correct `audio/*` value here already
 *   agrees with it; the specific `audio/*` strings below are chosen to
 *   match `classifyByMime`'s own test fixtures exactly, so a byte-for-byte
 *   MIME string never diverges between the two even though the node-side
 *   check itself only cares about the prefix.
 *
 * Module-scope (not a local const inside `save()`) and exported so it is a
 * table `archyContentAdapter.test.ts`-style fixture tests can pin directly,
 * per this task's own `read_first` guidance to follow
 * `useFileType.test.ts`'s fixture-table convention.
 */
export const SHARE_MIME_MAP: Record<string, string> = {
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  png: 'image/png',
  gif: 'image/gif',
  webp: 'image/webp',
  mp4: 'video/mp4',
  webm: 'video/webm',
  mkv: 'video/x-matroska',
  mp3: 'audio/mpeg',
  flac: 'audio/flac',
  ogg: 'audio/ogg',
  wav: 'audio/wav',
  m4a: 'audio/mp4',
  aac: 'audio/aac',
  opus: 'audio/opus',
  wma: 'audio/x-ms-wma',
  pdf: 'application/pdf',
  zip: 'application/zip',
  txt: 'text/plain',
}
</script>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const props = defineProps<{
  filename: string
  filepath: string
  isDir: boolean
  existingItemId?: string
}>()

const emit = defineEmits<{
  close: []
  saved: []
}>()

const shared = ref(false)
const accessType = ref<'free' | 'peers_only' | 'paid'>('free')
const priceSats = ref<number>(100)
const saving = ref(false)
const errorMsg = ref<string | null>(null)
const successMsg = ref<string | null>(null)

// --- Accepted payment methods, gated on what this node can actually receive ---

const PAY_METHODS = [
  { key: 'lightning', label: 'Lightning' },
  { key: 'onchain', label: 'On-chain' },
  { key: 'ecash', label: 'Cashu ecash' },
  { key: 'fedimint', label: 'Fedimint' },
] as const
type PayMethod = (typeof PAY_METHODS)[number]['key']

// undefined = probe in flight; then true/false per method.
const capability = ref<Partial<Record<PayMethod, boolean>>>({})
const adviceLines = ref<Partial<Record<PayMethod, string[]>>>({})
const acceptedSet = ref<Set<PayMethod>>(new Set())
const adviceFor = ref<PayMethod | null>(null)
// Only default-select capable methods when the item had no saved list.
let acceptedLoadedFromItem = false

function toggleMethod(key: PayMethod, on: boolean) {
  const next = new Set(acceptedSet.value)
  if (on) next.add(key)
  else next.delete(key)
  acceptedSet.value = next
}

/** Probe the node's rails and build advice for the unavailable ones. */
async function probeCapabilities() {
  // Lightning + on-chain both live on LND.
  try {
    const info = await rpcClient.call<{ num_active_channels?: number; synced_to_chain?: boolean }>({
      method: 'lnd.getinfo', timeout: 8000,
    })
    capability.value.onchain = true
    const channels = info?.num_active_channels ?? 0
    capability.value.lightning = channels > 0
    if (channels === 0) {
      adviceLines.value.lightning = [
        'Your Lightning node is running but has no active channel — buyers cannot pay you over Lightning yet.',
        'Open a channel from Wallet → Lightning Channels (funds on your on-chain balance can back it).',
        'Once the channel is active, come back and enable Lightning here.',
      ]
    }
  } catch {
    capability.value.lightning = false
    capability.value.onchain = false
    adviceLines.value.lightning = [
      'The Lightning (LND) app isn\'t running on this node.',
      'Install/start Lightning from the App Store, let it sync, then open a channel.',
    ]
    adviceLines.value.onchain = [
      'On-chain receiving uses the Lightning (LND) app\'s wallet, which isn\'t running.',
      'Install/start Lightning from the App Store — no channel needed for on-chain.',
    ]
  }
  try {
    await rpcClient.call({ method: 'wallet.ecash-balance', timeout: 8000 })
    capability.value.ecash = true
  } catch {
    capability.value.ecash = false
    adviceLines.value.ecash = [
      'The Cashu ecash wallet isn\'t set up on this node.',
      'Open Wallet → Ecash to connect a mint, then enable Cashu here.',
    ]
  }
  try {
    await rpcClient.call({ method: 'wallet.fedimint-balance', timeout: 8000 })
    capability.value.fedimint = true
  } catch {
    capability.value.fedimint = false
    adviceLines.value.fedimint = [
      'This node hasn\'t joined a Fedimint federation.',
      'Install the Fedimint app and join (or create) a federation, then enable it here.',
    ]
  }
  // Defaults: everything the node can receive — unless the item already
  // carried an explicit list. Never auto-enable an incapable rail.
  if (!acceptedLoadedFromItem) {
    acceptedSet.value = new Set(PAY_METHODS.filter((m) => capability.value[m.key]).map((m) => m.key))
  } else {
    acceptedSet.value = new Set([...acceptedSet.value].filter((k) => capability.value[k]))
  }
}

// If we have an existing item, load its state

/** Catalog entries store the slash-stripped path; props carry a leading
 *  slash (filepath) or just the basename (filename). Normalize both sides —
 *  the old exact compare never matched, so every re-share created a brand
 *  new priced entry and buyers could pay twice for one file (2026-07-22). */
function matchesThisFile(catalogFilename: string): boolean {
  const strip = (v: string) => v.replace(/^\/+/, '')
  return (
    strip(catalogFilename) === strip(props.filepath || '') ||
    strip(catalogFilename) === strip(props.filename || '')
  )
}

onMounted(async () => {
  try {
    const res = await rpcClient.call<{ items: Array<{
      id: string
      filename: string
      access: { free?: unknown; peersonly?: unknown; paid?: { price_sats: number; accepted?: string[] } } | string
      availability: string | { allpeers?: unknown; nobody?: unknown }
    }> }>({ method: 'content.list-mine' })
    const match = res.items.find(
      (i) => matchesThisFile(i.filename)
    )
    if (match) {
      shared.value = true
      const access = match.access
      if (typeof access === 'string') {
        if (access === 'free') accessType.value = 'free'
        else if (access === 'peersonly') accessType.value = 'peers_only'
      } else if (access && typeof access === 'object') {
        if ('paid' in access && access.paid) {
          accessType.value = 'paid'
          priceSats.value = access.paid.price_sats || 100
          if (Array.isArray(access.paid.accepted) && access.paid.accepted.length) {
            acceptedLoadedFromItem = true
            acceptedSet.value = new Set(
              access.paid.accepted.filter((m): m is PayMethod =>
                PAY_METHODS.some((p) => p.key === m),
              ),
            )
          }
        } else if ('peersonly' in access) {
          accessType.value = 'peers_only'
        }
      }
    }
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Not shared yet, defaults are fine', e)
  }
  void probeCapabilities()
})

async function save() {
  saving.value = true
  errorMsg.value = null
  successMsg.value = null

  try {
    if (!shared.value) {
      // Find and remove from catalog
      const res = await rpcClient.call<{ items: Array<{ id: string; filename: string }> }>({
        method: 'content.list-mine',
      })
      const match = res.items.find(
        (i) => matchesThisFile(i.filename)
      )
      if (match) {
        await rpcClient.call({ method: 'content.remove', params: { id: match.id } })
      }
      successMsg.value = 'Sharing disabled'
    } else {
      // Check if already in catalog
      const res = await rpcClient.call<{ items: Array<{ id: string; filename: string }> }>({
        method: 'content.list-mine',
      })
      let itemId = res.items.find(
        (i) => matchesThisFile(i.filename)
      )?.id

      // Add if not in catalog
      if (!itemId) {
        const ext = props.filename.split('.').pop()?.toLowerCase() || ''
        const addRes = await rpcClient.call<{ item: { id: string } }>({
          method: 'content.add',
          params: {
            filename: (props.filepath || props.filename).replace(/^\/+/, ''),
            mime_type: SHARE_MIME_MAP[ext] || 'application/octet-stream',
            description: '',
          },
        })
        itemId = addRes.item.id
      }

      // Set pricing
      const pricingParams: Record<string, unknown> = { id: itemId, access: accessType.value }
      if (accessType.value === 'paid') {
        pricingParams.price_sats = priceSats.value
        pricingParams.accepted_methods = [...acceptedSet.value]
      }
      await rpcClient.call({ method: 'content.set-pricing', params: pricingParams })

      // Set availability to all peers
      await rpcClient.call({
        method: 'content.set-availability',
        params: { id: itemId, availability: 'all_peers' },
      })

      const label =
        accessType.value === 'paid'
          ? `Shared for ${priceSats.value} sats`
          : accessType.value === 'peers_only'
            ? 'Shared with peers'
            : 'Shared (free)'
      successMsg.value = label
    }

    setTimeout(() => emit('saved'), 800)
  } catch (e) {
    errorMsg.value = e instanceof Error ? e.message : 'Failed to update sharing'
  } finally {
    saving.value = false
  }
}
</script>
