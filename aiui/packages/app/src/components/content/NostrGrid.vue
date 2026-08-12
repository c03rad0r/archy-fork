<template>
  <div class="h-full flex flex-col">
    <!-- Sub-tab switcher -->
    <div class="flex gap-2 px-4 pt-3 pb-1">
      <button
        v-for="tab in subTabs"
        :key="tab.id"
        class="text-xs px-2.5 min-h-[44px] rounded-md transition-all duration-150 flex items-center justify-center"
        :class="activeSubTab === tab.id
          ? 'nav-tab-active'
          : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
        @click="activeSubTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <!-- DMs sub-tab -->
    <NostrDMs v-if="activeSubTab === 'dms'" />

    <!-- Relays sub-tab -->
    <NostrRelayManager v-else-if="activeSubTab === 'relays'" />

    <!-- Profile sub-tab -->
    <NostrProfileEditor v-else-if="activeSubTab === 'profile'" />

    <!-- Lists sub-tab -->
    <NostrLists v-else-if="activeSubTab === 'lists'" />

    <!-- Articles sub-tab -->
    <NostrArticles v-else-if="activeSubTab === 'articles'" />

    <!-- Thread view -->
    <NostrThread
      v-else-if="activeSubTab === 'feed' && selectedNoteId"
      :note-id="selectedNoteId"
      @back="selectedNoteId = null"
    />

    <!-- Feed sub-tab -->
    <template v-else>
    <div class="p-4 space-y-3 border-b border-white/[0.08]">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold text-white/90">
          Nostr Feed
        </h3>
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-mono text-white/30">
            {{ filteredNotes.length }} notes
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <!-- Compose toggle -->
      <button
        v-if="isLoggedIn"
        class="w-full text-left px-3 py-2 rounded-lg text-xs text-white/40 bg-white/5 hover:bg-white/10 transition-colors"
        @click="showCompose = !showCompose"
      >
        {{ showCompose ? 'Cancel' : 'Write a note...' }}
      </button>

      <!-- Compose panel -->
      <div v-if="showCompose && isLoggedIn" class="rounded-lg bg-white/5 border border-white/10 p-3 space-y-2">
        <textarea
          ref="composeRef"
          v-model="composeText"
          class="w-full bg-transparent text-base text-white/80 placeholder:text-white/25 outline-none resize-none min-h-[80px]"
          placeholder="What's on your mind?"
          @keydown.meta.enter="publishNote"
          @keydown.ctrl.enter="publishNote"
        />
        <div class="flex items-center justify-between gap-2">
          <span
            class="text-xs tabular-nums"
            :class="composeText.length > 280 ? 'text-accent/80' : 'text-white/25'"
          >
            {{ composeText.length }}
          </span>
          <button
            class="text-xs px-3 py-1.5 rounded-lg bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
            :disabled="!composeText.trim() || isPublishing"
            @click="publishNote"
          >
            {{ isPublishing ? 'Publishing...' : 'Publish' }}
          </button>
        </div>

        <!-- Publish results -->
        <div v-if="publishResults.length > 0" class="space-y-1">
          <div
            v-for="result in publishResults"
            :key="result.url"
            class="flex items-center gap-2 text-xs px-2 py-1 rounded bg-white/[0.02]"
          >
            <span
              class="w-1.5 h-1.5 rounded-full shrink-0"
              :class="result.success ? 'bg-emerald-500' : 'bg-red-400/60'"
            />
            <span class="truncate font-mono text-white/40">{{ result.url }}</span>
            <span class="ml-auto shrink-0" :class="result.success ? 'text-emerald-400/60' : 'text-red-400/60'">
              {{ result.message }}
            </span>
          </div>
        </div>
      </div>

      <div class="flex gap-2">
        <input
          v-model="search"
          type="text"
          placeholder="Search notes, npubs..."
          class="flex-1 px-3 py-2 rounded-lg text-base outline-none transition-colors bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10"
          @keydown.enter="triggerNostrSearch"
        />
        <button
          v-if="search.trim()"
          class="px-2.5 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30 shrink-0"
          :disabled="isSearching"
          @click="triggerNostrSearch"
        >
          {{ isSearching ? '...' : 'NIP-50' }}
        </button>
      </div>

      <div class="flex gap-2">
        <button
          v-for="kind in noteKinds"
          :key="kind.id"
          class="text-xs px-2.5 min-h-[44px] rounded-md transition-all duration-150 flex items-center justify-center"
          :class="activeKind === kind.id
            ? 'nav-tab-active'
            : 'text-white/40 hover:text-white/70 hover:bg-white/5'"
          @click="activeKind = activeKind === kind.id ? null : kind.id"
        >
          {{ kind.label }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <!-- Loading state -->
      <div
        v-if="!isConnected && notes.length === 0"
        class="flex flex-col items-center justify-center py-12 gap-3"
      >
        <svg class="w-5 h-5 animate-spin text-white/30" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
        <p class="text-xs text-white/30">Connecting to relays...</p>
      </div>

      <button
        v-for="note in filteredNotes"
        :key="note.id"
        class="w-full text-left p-3 rounded-xl transition-all duration-150 bg-white/[0.03] hover:bg-white/[0.07] border border-white/5"
        @click="selectedNoteId = note.id"
      >
        <div class="flex items-start gap-2.5">
          <div class="w-8 h-8 rounded-full shrink-0 overflow-hidden">
            <img
              v-if="note.authorPicture && !failedAvatars.has(note.pubkey)"
              :src="note.authorPicture"
              :alt="note.authorName ?? 'profile'"
              class="w-full h-full object-cover"
              loading="lazy"
              @error="failedAvatars.add(note.pubkey)"
            />
            <div
              v-else
              class="w-full h-full flex items-center justify-center text-xs font-bold bg-purple-500/20 text-purple-400"
            >
              {{ note.authorName?.charAt(0)?.toUpperCase() ?? '?' }}
            </div>
          </div>
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5">
              <span class="text-xs font-semibold truncate text-white/80">
                {{ note.authorName ?? 'anon' }}
              </span>
              <span v-if="note.nip05" class="text-xs truncate text-purple-400/60 flex items-center gap-0.5">
                <svg
                  v-if="nip05Status[note.id] === true"
                  class="w-2.5 h-2.5 text-emerald-400 shrink-0"
                  fill="currentColor"
                  viewBox="0 0 20 20"
                  :title="note.nip05"
                >
                  <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                </svg>
                {{ note.nip05 }}
              </span>
              <span class="text-xs ml-auto shrink-0 text-white/20">
                {{ formatTime(note.created_at) }}
              </span>
            </div>
            <p class="text-xs mt-1 leading-relaxed line-clamp-3 text-white/60">
              {{ note.content }}
            </p>
            <div class="flex items-center gap-3 mt-2">
              <button
                class="text-xs px-3 py-2 min-h-[44px] min-w-[44px] rounded bg-white/5 text-accent/60 hover:text-accent hover:bg-accent/10 transition-colors flex items-center justify-center"
                @click.stop="openZap(note)"
              >
                Zap
              </button>
              <span
                v-if="note.kind !== 1"
                class="text-xs px-1.5 py-0.5 rounded bg-white/5 text-white/30"
              >
                kind:{{ note.kind }}
              </span>
            </div>
          </div>
        </div>
      </button>

      <!-- Relay status -->
      <div class="mt-4 pt-4 border-t border-white/5">
        <p class="text-xs font-medium mb-2 text-white/30">Relays</p>
        <div class="space-y-1">
          <div
            v-for="relay in relayStates"
            :key="relay.url"
            class="flex items-center gap-2 text-xs px-2 py-1 rounded-lg bg-white/[0.02]"
          >
            <span
              class="w-1.5 h-1.5 rounded-full shrink-0"
              :class="relay.connected ? 'bg-emerald-500' : 'bg-red-400/60'"
            />
            <span class="truncate font-mono text-white/40">{{ relay.url }}</span>
          </div>
        </div>
      </div>

      <div
        v-if="isConnected && filteredNotes.length === 0"
        class="flex items-center justify-center py-12"
      >
        <p class="text-sm text-white/30">No notes match your search</p>
      </div>
    </div>
    </template>

    <!-- Zap dialog -->
    <ZapDialog
      :is-open="zapOpen"
      :target-name="zapTargetName"
      :lightning-address="zapLightningAddress"
      @close="zapOpen = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive, onMounted, nextTick, watch } from 'vue'
import NostrDMs from './NostrDMs.vue'
import NostrRelayManager from './NostrRelayManager.vue'
import NostrProfileEditor from './NostrProfileEditor.vue'
import ZapDialog from './ZapDialog.vue'
import NostrThread from './NostrThread.vue'
import NostrLists from './NostrLists.vue'
import NostrArticles from './NostrArticles.vue'
import { useNip05Verification } from '@/composables/useNip05Verification'
import { useNostr, type NostrNote, type PublishResult } from '@/composables/useNostr'
import { useNostrIdentity } from '@/composables/useNostrIdentity'


const { events: notes, isConnected, relayStates, connect, publishEvent, searchResults, isSearching, searchNostr } = useNostr()
const { isLoggedIn, signEvent } = useNostrIdentity()
const { verifyNip05 } = useNip05Verification()

const activeSubTab = ref<'feed' | 'dms' | 'relays' | 'profile' | 'lists' | 'articles'>('feed')
const subTabs = [
  { id: 'feed' as const, label: 'Feed' },
  { id: 'articles' as const, label: 'Articles' },
  { id: 'dms' as const, label: 'Messages' },
  { id: 'lists' as const, label: 'Lists' },
  { id: 'relays' as const, label: 'Relays' },
  { id: 'profile' as const, label: 'Profile' },
]

const selectedNoteId = ref<string | null>(null)
const failedAvatars = reactive(new Set<string>())
const search = ref('')
const activeKind = ref<number | null>(null)
const showCompose = ref(false)
const composeText = ref('')
const composeRef = ref<HTMLTextAreaElement | null>(null)
const isPublishing = ref(false)
const publishResults = ref<PublishResult[]>([])

const zapOpen = ref(false)
const zapTargetName = ref('')
const zapLightningAddress = ref<string | undefined>(undefined)

function openZap(note: NostrNote) {
  zapTargetName.value = note.authorName ?? note.pubkey.slice(0, 12)
  // In a full implementation, we'd fetch the profile to get their Lightning address
  zapLightningAddress.value = undefined
  zapOpen.value = true
}

const nip05Status = reactive<Record<string, boolean | null>>({})

const noteKinds = [
  { id: 1, label: 'Notes' },
  { id: 30023, label: 'Articles' },
  { id: 9735, label: 'Zaps' },
  { id: 6, label: 'Reposts' },
]

function formatTime(ts: number): string {
  const diff = Math.floor(Date.now() / 1000 - ts)
  if (diff < 60) return 'now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`
  return `${Math.floor(diff / 86400)}d`
}

const usingNostrSearch = ref(false)

const filteredNotes = computed(() => {
  // If NIP-50 search active, show those results
  if (usingNostrSearch.value && searchResults.value.length > 0) {
    let result = searchResults.value
    if (activeKind.value !== null) {
      result = result.filter(n => n.kind === activeKind.value)
    }
    return result
  }

  let result = notes.value
  if (activeKind.value !== null) {
    result = result.filter(n => n.kind === activeKind.value)
  }
  if (search.value) {
    const q = search.value.toLowerCase()
    result = result.filter(n =>
      n.content.toLowerCase().includes(q) ||
      (n.authorName ?? '').toLowerCase().includes(q) ||
      (n.nip05 ?? '').toLowerCase().includes(q)
    )
  }
  return result
})

function triggerNostrSearch() {
  if (!search.value.trim()) return
  usingNostrSearch.value = true
  searchNostr(search.value.trim(), activeKind.value ? [activeKind.value] : undefined)
}

// Clear NIP-50 mode when search is cleared
watch(search, (val) => {
  if (!val.trim()) usingNostrSearch.value = false
})

// Verify NIP-05 for notes that have it
watch(filteredNotes, (visibleNotes) => {
  for (const note of visibleNotes) {
    if (note.nip05 && nip05Status[note.id] === undefined) {
      nip05Status[note.id] = null
      verifyNip05(note.nip05, note.pubkey).then(result => {
        nip05Status[note.id] = result
      })
    }
  }
}, { immediate: true })

async function publishNote() {
  if (!composeText.value.trim() || isPublishing.value) return

  isPublishing.value = true
  publishResults.value = []

  const unsigned = {
    kind: 1,
    created_at: Math.floor(Date.now() / 1000),
    tags: [],
    content: composeText.value.trim(),
  }

  const signed = await signEvent(unsigned)
  if (!signed) {
    isPublishing.value = false
    return
  }

  const results = await publishEvent(signed)
  publishResults.value = results
  isPublishing.value = false

  if (results.some(r => r.success)) {
    composeText.value = ''
    setTimeout(() => {
      publishResults.value = []
      showCompose.value = false
    }, 3000)
  }
}

onMounted(async () => {
  connect()
  if (showCompose.value) {
    await nextTick()
    composeRef.value?.focus()
  }
})
</script>
