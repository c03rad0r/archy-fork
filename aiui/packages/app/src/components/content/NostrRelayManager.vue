<template>
  <div class="h-full flex flex-col">
    <div class="p-4 border-b border-white/[0.08]">
      <h3 class="text-sm font-bold text-white/90 mb-3">Relay Management</h3>

      <!-- Add relay -->
      <div class="flex gap-2">
        <input
          v-model="newRelayUrl"
          type="text"
          placeholder="wss://relay.example.com"
          class="flex-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
          @keydown.enter="addNewRelay"
        />
        <button
          class="px-4 min-h-[44px] rounded-lg text-sm bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
          :disabled="!newRelayUrl.trim()"
          @click="addNewRelay"
        >
          Add
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <div
        v-for="relay in relayStates"
        :key="relay.url"
        class="rounded-xl bg-white/[0.03] border border-white/5 p-3 space-y-2"
      >
        <!-- Relay URL + status -->
        <div class="flex items-center gap-2">
          <span
            class="w-2 h-2 rounded-full shrink-0"
            :class="relay.connected ? 'bg-emerald-500' : 'bg-red-400/60'"
          />
          <span class="text-xs font-mono text-white/70 truncate flex-1">{{ relay.url }}</span>
          <span
            v-if="relay.latencyMs !== null"
            class="text-xs tabular-nums shrink-0"
            :class="relay.latencyMs < 200 ? 'text-emerald-400/60' : relay.latencyMs < 500 ? 'text-yellow-400/60' : 'text-red-400/60'"
          >
            {{ relay.latencyMs }}ms
          </span>
          <span
            class="text-xs shrink-0"
            :class="relay.connected ? 'text-emerald-400/60' : 'text-red-400/60'"
          >
            {{ relay.connected ? 'Connected' : 'Disconnected' }}
          </span>
        </div>

        <!-- Controls -->
        <div class="flex items-center gap-2 flex-wrap">
          <button
            class="text-sm px-3 min-h-[44px] rounded-lg transition-colors"
            :class="relay.read
              ? 'bg-accent/15 text-accent/80'
              : 'bg-white/5 text-white/30 hover:text-white/50'"
            @click="toggleRelayRead(relay.url)"
          >
            Read
          </button>
          <button
            class="text-sm px-3 min-h-[44px] rounded-lg transition-colors"
            :class="relay.write
              ? 'bg-accent/15 text-accent/80'
              : 'bg-white/5 text-white/30 hover:text-white/50'"
            @click="toggleRelayWrite(relay.url)"
          >
            Write
          </button>
          <div class="flex-1" />
          <button
            class="text-sm px-3 min-h-[44px] rounded-lg bg-white/5 text-white/30 hover:text-white/50 transition-colors"
            :disabled="testingRelay === relay.url"
            @click="testConnection(relay.url)"
          >
            {{ testingRelay === relay.url ? 'Testing...' : 'Test' }}
          </button>
          <button
            class="text-sm px-3 min-h-[44px] rounded-lg bg-white/5 text-red-400/50 hover:text-red-400/80 hover:bg-red-400/10 transition-colors"
            @click="removeRelay(relay.url)"
          >
            Remove
          </button>
        </div>

        <!-- Test result -->
        <p
          v-if="testResults[relay.url] !== undefined"
          class="text-xs"
          :class="testResults[relay.url] !== null ? 'text-emerald-400/60' : 'text-red-400/60'"
        >
          {{ testResults[relay.url] !== null ? `Reachable (${testResults[relay.url]}ms)` : 'Unreachable' }}
        </p>
      </div>

      <!-- Import NIP-65 -->
      <div v-if="isLoggedIn" class="mt-4 pt-4 border-t border-white/5">
        <button
          class="w-full text-left px-3 min-h-[44px] rounded-lg text-sm bg-white/5 text-white/40 hover:text-white/60 hover:bg-white/10 transition-colors"
          :disabled="isImporting"
          @click="importFromNIP65"
        >
          {{ isImporting ? 'Importing...' : 'Import relays from NIP-65 (kind:10002)' }}
        </button>
        <p v-if="importMessage" class="text-xs mt-1 text-white/30">{{ importMessage }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive } from 'vue'
import { useNostr } from '@/composables/useNostr'
import { useNostrIdentity } from '@/composables/useNostrIdentity'

const { relayStates, addRelay, removeRelay, toggleRelayRead, toggleRelayWrite, testRelay, importNIP65Relays, fetchNote } = useNostr()
const { isLoggedIn, pubkey } = useNostrIdentity()

const newRelayUrl = ref('')
const testingRelay = ref<string | null>(null)
const testResults = reactive<Record<string, number | null>>({})
const isImporting = ref(false)
const importMessage = ref('')

function addNewRelay() {
  let url = newRelayUrl.value.trim()
  if (!url) return
  if (!url.startsWith('wss://') && !url.startsWith('ws://')) {
    url = 'wss://' + url
  }
  addRelay(url)
  newRelayUrl.value = ''
}

async function testConnection(url: string) {
  testingRelay.value = url
  const latency = await testRelay(url)
  testResults[url] = latency
  testingRelay.value = null
}

async function importFromNIP65() {
  if (!pubkey.value) return
  isImporting.value = true
  importMessage.value = 'Fetching relay list...'

  // Fetch kind:10002 for our pubkey from connected relays
  const note = await fetchNote(pubkey.value, 5000)
  if (note) {
    importNIP65Relays({
      id: note.id,
      pubkey: note.pubkey,
      kind: 10002,
      content: note.content,
      created_at: note.created_at,
      tags: note.tags,
      sig: '',
    })
    importMessage.value = 'Imported relays from NIP-65'
  } else {
    importMessage.value = 'No NIP-65 relay list found'
  }
  isImporting.value = false
}
</script>
