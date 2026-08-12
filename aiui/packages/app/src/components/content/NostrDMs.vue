<template>
  <div class="h-full flex flex-col">
    <!-- Thread view -->
    <template v-if="activeThread">
      <div class="flex items-center gap-2 px-4 py-3 border-b border-white/[0.08]">
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          @click="clearActiveContact"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <div class="flex-1 min-w-0">
          <p class="text-xs font-semibold text-white/80 truncate">{{ activeThread.contactName }}</p>
          <p class="text-xs text-white/30 font-mono truncate">{{ activeThread.contactPubkey }}</p>
        </div>
      </div>

      <div ref="messagesRef" class="flex-1 overflow-y-auto custom-scrollbar px-4 py-3 space-y-2">
        <div
          v-for="msg in activeThread.messages"
          :key="msg.id"
          class="flex"
          :class="msg.fromPubkey === pubkey ? 'justify-end' : 'justify-start'"
        >
          <div
            class="max-w-[80%] rounded-xl px-3 py-2"
            :class="msg.fromPubkey === pubkey
              ? 'bg-accent/15 text-white/80'
              : 'bg-white/5 text-white/70'"
          >
            <p class="text-xs leading-relaxed break-words">{{ msg.content }}</p>
            <p class="text-xs mt-1 text-white/25 tabular-nums">{{ formatTime(msg.created_at) }}</p>
          </div>
        </div>
      </div>

      <!-- Message input -->
      <div class="px-4 py-3 border-t border-white/[0.08]">
        <div class="flex gap-2">
          <input
            v-model="messageInput"
            type="text"
            placeholder="Type a message..."
            class="flex-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
            @keydown.enter="sendMessage"
          />
          <button
            class="px-3 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
            :disabled="!messageInput.trim() || isSending"
            @click="sendMessage"
          >
            Send
          </button>
        </div>
      </div>
    </template>

    <!-- Contact list / inbox -->
    <template v-else>
      <div class="p-4 border-b border-white/[0.08]">
        <div class="flex items-center justify-between gap-2 mb-3">
          <h3 class="text-sm font-bold text-white/90">Messages</h3>
          <button
            class="text-xs px-2.5 py-1 rounded bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
            @click="showNewDM = !showNewDM"
          >
            {{ showNewDM ? 'Cancel' : 'New' }}
          </button>
        </div>

        <!-- New DM input -->
        <div v-if="showNewDM" class="space-y-2 mb-3">
          <input
            v-model="newContactPubkey"
            type="text"
            placeholder="Recipient hex pubkey or npub..."
            class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors font-mono"
          />
          <button
            class="text-xs px-2.5 py-1 rounded bg-white/5 text-white/60 hover:bg-white/10 transition-colors disabled:opacity-30"
            :disabled="!newContactPubkey.trim()"
            @click="startNewDM"
          >
            Start conversation
          </button>
        </div>
      </div>

      <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-1">
        <div
          v-if="!isLoggedIn"
          class="flex items-center justify-center py-12"
        >
          <p class="text-xs text-white/30">Sign in with Nostr to use DMs</p>
        </div>

        <div
          v-else-if="isLoading"
          class="flex items-center justify-center py-12"
        >
          <p class="text-xs text-white/30">Loading messages...</p>
        </div>

        <div
          v-else-if="threads.length === 0"
          class="flex items-center justify-center py-12"
        >
          <p class="text-xs text-white/30">No messages yet</p>
        </div>

        <button
          v-for="thread in threads"
          :key="thread.contactPubkey"
          class="w-full text-left p-3 rounded-xl transition-all duration-150 bg-white/[0.03] hover:bg-white/[0.07] border border-white/5"
          @click="selectContact(thread.contactPubkey)"
        >
          <div class="flex items-start gap-2.5">
            <div class="w-8 h-8 rounded-full shrink-0 flex items-center justify-center text-xs font-bold bg-accent/20 text-accent">
              {{ thread.contactName.charAt(0).toUpperCase() }}
            </div>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-1.5">
                <span class="text-xs font-semibold truncate text-white/80">
                  {{ thread.contactName }}
                </span>
                <span v-if="thread.lastMessage" class="text-xs ml-auto shrink-0 text-white/20">
                  {{ formatTime(thread.lastMessage.created_at) }}
                </span>
              </div>
              <p v-if="thread.lastMessage" class="text-xs mt-1 text-white/40 truncate">
                {{ thread.lastMessage.content }}
              </p>
            </div>
          </div>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick, watch } from 'vue'
import { useNostrDMs } from '@/composables/useNostrDMs'
import { useNostrIdentity } from '@/composables/useNostrIdentity'
import { decodeNpub } from '@/utils/bech32'

const { threads, activeThread, activeContact, isLoading, loadDMs, sendDM, selectContact, clearActiveContact } = useNostrDMs()
const { pubkey, isLoggedIn } = useNostrIdentity()

const messageInput = ref('')
const isSending = ref(false)
const messagesRef = ref<HTMLElement | null>(null)
const showNewDM = ref(false)
const newContactPubkey = ref('')

function formatTime(ts: number): string {
  const d = new Date(ts * 1000)
  const now = new Date()
  const diffDays = Math.floor((now.getTime() - d.getTime()) / 86400000)
  if (diffDays === 0) return d.toLocaleTimeString('en', { hour: '2-digit', minute: '2-digit' })
  if (diffDays < 7) return d.toLocaleDateString('en', { weekday: 'short' })
  return d.toLocaleDateString('en', { month: 'short', day: 'numeric' })
}

async function sendMessage() {
  if (!messageInput.value.trim() || isSending.value || !activeContact.value) return
  isSending.value = true
  const success = await sendDM(activeContact.value, messageInput.value.trim())
  if (success) {
    messageInput.value = ''
    await nextTick()
    scrollToBottom()
  }
  isSending.value = false
}

function scrollToBottom() {
  if (messagesRef.value) {
    messagesRef.value.scrollTop = messagesRef.value.scrollHeight
  }
}

function startNewDM() {
  let hex = newContactPubkey.value.trim()
  if (hex.startsWith('npub')) {
    try {
      hex = decodeNpub(hex)
    } catch {
      return
    }
  }
  if (hex.length === 64) {
    selectContact(hex)
    showNewDM.value = false
    newContactPubkey.value = ''
  }
}

watch(activeContact, async () => {
  await nextTick()
  scrollToBottom()
})

onMounted(() => {
  loadDMs()
})
</script>
