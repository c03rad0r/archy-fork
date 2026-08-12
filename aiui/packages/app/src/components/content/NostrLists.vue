<template>
  <div class="h-full flex flex-col">
    <div class="p-4 border-b border-white/[0.08]">
      <h3 class="text-sm font-bold text-white/90 mb-3">Nostr Lists</h3>

      <!-- List type selector -->
      <div class="flex gap-1.5 flex-wrap">
        <button
          v-for="lt in listTypes"
          :key="lt.kind"
          class="text-xs px-2 py-1 rounded-md transition-all duration-150"
          :class="activeListKind === lt.kind
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeListKind = lt.kind; loadList(lt.kind)"
        >
          {{ lt.label }}
        </button>
      </div>
    </div>

    <div v-if="!isLoggedIn" class="flex-1 flex items-center justify-center">
      <p class="text-xs text-white/30">Sign in with Nostr to manage lists</p>
    </div>

    <div v-else class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <!-- Loading -->
      <div v-if="isLoading" class="flex items-center justify-center py-12">
        <p class="text-xs text-white/30">Loading list...</p>
      </div>

      <!-- Add item -->
      <div class="flex gap-2 mb-3">
        <input
          v-model="newItemValue"
          type="text"
          :placeholder="activeListKind === 3 ? 'Add npub or hex pubkey...' : 'Add item (hex id or npub)...'"
          class="flex-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
          @keydown.enter="addItem"
        />
        <button
          class="px-2.5 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
          :disabled="!newItemValue.trim()"
          @click="addItem"
        >
          Add
        </button>
      </div>

      <!-- List items -->
      <div
        v-for="item in listItems"
        :key="item.value"
        class="flex items-center gap-2 p-2.5 rounded-xl bg-white/[0.03] border border-white/5"
      >
        <div class="w-6 h-6 rounded-full shrink-0 flex items-center justify-center text-xs font-bold bg-purple-500/20 text-purple-400">
          {{ item.tag === 'p' ? 'P' : item.tag === 'e' ? 'E' : item.tag === 't' ? '#' : '?' }}
        </div>
        <div class="flex-1 min-w-0">
          <p class="text-xs text-white/60 font-mono truncate">{{ item.displayValue }}</p>
          <p v-if="item.petname" class="text-xs text-white/30">{{ item.petname }}</p>
        </div>
        <button
          class="text-xs px-2 py-1 rounded bg-white/5 text-red-400/50 hover:text-red-400/80 hover:bg-red-400/10 transition-colors shrink-0"
          @click="removeItem(item)"
        >
          Remove
        </button>
      </div>

      <div v-if="!isLoading && listItems.length === 0" class="flex items-center justify-center py-12">
        <p class="text-xs text-white/30">List is empty</p>
      </div>

      <!-- Publish button -->
      <button
        v-if="isDirty"
        class="w-full py-2.5 rounded-lg text-xs font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30 mt-4"
        :disabled="isPublishingList"
        @click="publishList"
      >
        {{ isPublishingList ? 'Publishing...' : 'Publish updated list' }}
      </button>

      <p v-if="publishStatus" class="text-xs text-center" :class="publishOk ? 'text-emerald-400/60' : 'text-red-400/60'">
        {{ publishStatus }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useNostrIdentity } from '@/composables/useNostrIdentity'
import { useNostr, type NostrEvent } from '@/composables/useNostr'
import { decodeNpub, encodeNpub } from '@/utils/bech32'

const { isLoggedIn, signEvent, pubkey } = useNostrIdentity()
const { publishEvent } = useNostr()

interface ListItem {
  tag: string
  value: string
  displayValue: string
  relay?: string
  petname?: string
}

const listTypes = [
  { kind: 3, label: 'Follows' },
  { kind: 10000, label: 'Mute' },
  { kind: 10001, label: 'Pin' },
  { kind: 10003, label: 'Bookmarks' },
]

const activeListKind = ref(3)
const listItems = ref<ListItem[]>([])
const isLoading = ref(false)
const isDirty = ref(false)
const newItemValue = ref('')
const isPublishingList = ref(false)
const publishStatus = ref('')
const publishOk = ref(false)

function truncate(hex: string): string {
  if (hex.length <= 16) return hex
  return hex.slice(0, 8) + '...' + hex.slice(-8)
}

function tagsToItems(tags: string[][]): ListItem[] {
  return tags
    .filter(t => t[0] === 'p' || t[0] === 'e' || t[0] === 't')
    .map(t => {
      let displayValue = truncate(t[1])
      if (t[0] === 'p') {
        try { displayValue = encodeNpub(t[1]) } catch { /* keep hex */ }
      }
      return {
        tag: t[0],
        value: t[1],
        displayValue,
        relay: t[2] || undefined,
        petname: t[3] || undefined,
      }
    })
}

async function loadList(kind: number) {
  if (!pubkey.value) return
  isLoading.value = true
  isDirty.value = false
  listItems.value = []

  try {
    const ws = new WebSocket('wss://relay.nostr.band')
    const subId = 'list-' + Math.random().toString(36).slice(2, 8)

    const timer = setTimeout(() => {
      ws.close()
      isLoading.value = false
    }, 8000)

    ws.onopen = () => {
      ws.send(JSON.stringify([
        'REQ', subId,
        { kinds: [kind], authors: [pubkey.value], limit: 1 },
      ]))
    }

    ws.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data)
        if (Array.isArray(data) && data[0] === 'EVENT' && data[1] === subId && data[2]) {
          const evt = data[2] as NostrEvent
          listItems.value = tagsToItems(evt.tags)
        }
        if (Array.isArray(data) && data[0] === 'EOSE') {
          clearTimeout(timer)
          ws.close()
          isLoading.value = false
        }
      } catch { /* skip */ }
    }

    ws.onerror = () => {
      clearTimeout(timer)
      isLoading.value = false
    }
  } catch {
    isLoading.value = false
  }
}

function addItem() {
  let val = newItemValue.value.trim()
  if (!val) return

  let hex = val
  const tag = activeListKind.value === 3 || activeListKind.value === 10000 ? 'p' : 'e'

  if (val.startsWith('npub')) {
    try { hex = decodeNpub(val) } catch { return }
  }

  if (listItems.value.find(i => i.value === hex)) return

  let displayValue = truncate(hex)
  if (tag === 'p') {
    try { displayValue = encodeNpub(hex) } catch { /* keep hex */ }
  }

  listItems.value.push({ tag, value: hex, displayValue })
  isDirty.value = true
  newItemValue.value = ''
}

function removeItem(item: ListItem) {
  listItems.value = listItems.value.filter(i => i.value !== item.value)
  isDirty.value = true
}

async function publishList() {
  if (!pubkey.value) return

  isPublishingList.value = true
  publishStatus.value = ''

  const tags = listItems.value.map(i => {
    const t = [i.tag, i.value]
    if (i.relay) t.push(i.relay)
    if (i.petname) t.push(i.petname)
    return t
  })

  const unsigned = {
    kind: activeListKind.value,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  }

  const signed = await signEvent(unsigned)
  if (!signed) {
    isPublishingList.value = false
    publishStatus.value = 'Signing failed'
    publishOk.value = false
    return
  }

  const results = await publishEvent(signed)
  const successes = results.filter(r => r.success).length

  isPublishingList.value = false
  isDirty.value = false

  if (successes > 0) {
    publishStatus.value = `Published to ${successes}/${results.length} relays`
    publishOk.value = true
  } else {
    publishStatus.value = 'Failed to publish'
    publishOk.value = false
  }
}

onMounted(() => {
  loadList(activeListKind.value)
})
</script>
