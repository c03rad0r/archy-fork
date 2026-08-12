<template>
  <div class="h-full flex flex-col">
    <div class="flex items-center gap-2 px-4 py-3 border-b border-white/[0.08]">
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h3 class="text-sm font-bold text-white/90">Thread</h3>
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-3 pb-16 space-y-2">
      <!-- Loading -->
      <div v-if="isLoading" class="flex items-center justify-center py-12">
        <p class="text-xs text-white/30">Loading thread...</p>
      </div>

      <!-- Root note -->
      <div v-if="rootNote" class="rounded-xl bg-white/[0.05] border border-white/10 p-3">
        <div class="flex items-center gap-1.5 mb-1">
          <div class="w-6 h-6 rounded-full shrink-0 flex items-center justify-center text-xs font-bold bg-purple-500/20 text-purple-400">
            {{ rootNote.authorName?.charAt(0)?.toUpperCase() ?? '?' }}
          </div>
          <span class="text-xs font-semibold text-white/80">{{ rootNote.authorName ?? 'anon' }}</span>
          <span class="text-xs ml-auto text-white/20">{{ formatTime(rootNote.created_at) }}</span>
        </div>
        <p class="text-xs text-white/70 leading-relaxed whitespace-pre-wrap">{{ rootNote.content }}</p>
      </div>

      <!-- Replies -->
      <div v-if="threadTree.length > 0" class="space-y-1">
        <p class="text-xs text-white/30 font-medium mt-3 mb-1">{{ threadTree.length }} replies</p>
        <ThreadNode
          v-for="node in threadTree"
          :key="node.note.id"
          :node="node"
          :depth="0"
          @reply="startReply"
        />
      </div>

      <div v-if="!isLoading && !rootNote" class="flex items-center justify-center py-12">
        <p class="text-xs text-white/30">Thread not found</p>
      </div>
    </div>

    <!-- Reply input -->
    <div v-if="rootNote && isLoggedIn" class="px-4 py-3 border-t border-white/[0.08]">
      <p v-if="replyTo" class="text-xs text-white/30 mb-1">
        Replying to {{ replyTo.authorName ?? 'anon' }}
        <button class="text-accent/60 ml-1" @click="replyTo = null">cancel</button>
      </p>
      <div class="flex gap-2">
        <input
          v-model="replyText"
          type="text"
          placeholder="Reply..."
          class="flex-1 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
          @keydown.enter="sendReply"
        />
        <button
          class="px-3 py-2 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
          :disabled="!replyText.trim()"
          @click="sendReply"
        >
          Reply
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, defineAsyncComponent } from 'vue'
import { useNostr, type NostrNote, type NostrEvent } from '@/composables/useNostr'
import { useNostrIdentity } from '@/composables/useNostrIdentity'

const ThreadNode = defineAsyncComponent(() => import('./ThreadNode.vue'))

const props = defineProps<{
  noteId: string
}>()

defineEmits<{ back: [] }>()

const { fetchNote, publishEvent } = useNostr()
const { isLoggedIn, signEvent, pubkey } = useNostrIdentity()

interface ThreadTreeNode {
  note: NostrNote
  children: ThreadTreeNode[]
}

const rootNote = ref<NostrNote | null>(null)
const threadTree = ref<ThreadTreeNode[]>([])
const isLoading = ref(true)
const replyTo = ref<NostrNote | null>(null)
const replyText = ref('')

function formatTime(ts: number): string {
  const d = new Date(ts * 1000)
  return d.toLocaleTimeString('en', { hour: '2-digit', minute: '2-digit' })
    + ' ' + d.toLocaleDateString('en', { month: 'short', day: 'numeric' })
}

function truncatePubkey(pk: string): string {
  if (pk.length <= 12) return pk
  return pk.slice(0, 8) + '...' + pk.slice(-4)
}

async function loadThread() {
  isLoading.value = true

  // Fetch root note
  const root = await fetchNote(props.noteId)
  if (!root) {
    isLoading.value = false
    return
  }
  rootNote.value = root

  // Fetch replies (kind:1 with #e tag referencing this note)
  const replies = await fetchReplies(props.noteId)
  threadTree.value = buildTree(replies, props.noteId)

  isLoading.value = false
}

async function fetchReplies(rootId: string): Promise<NostrNote[]> {
  return new Promise((resolve) => {
    const results: NostrNote[] = []
    const subId = 'thread-' + Math.random().toString(36).slice(2, 8)
    let resolved = false

    const timer = setTimeout(() => {
      if (!resolved) { resolved = true; resolve(results) }
    }, 8000)

    // Connect to first available NIP-50 relay for broader search
    const relayUrl = 'wss://relay.nostr.band'
    try {
      const ws = new WebSocket(relayUrl)

      ws.onopen = () => {
        ws.send(JSON.stringify([
          'REQ', subId,
          { kinds: [1], '#e': [rootId], limit: 100 },
        ]))
      }

      ws.onmessage = (msg) => {
        try {
          const data = JSON.parse(msg.data)
          if (Array.isArray(data) && data[0] === 'EVENT' && data[1] === subId && data[2]) {
            const evt = data[2] as NostrEvent
            if (!results.find(r => r.id === evt.id)) {
              results.push({
                id: evt.id,
                pubkey: evt.pubkey,
                authorName: truncatePubkey(evt.pubkey),
                kind: evt.kind,
                content: evt.content,
                created_at: evt.created_at,
                tags: evt.tags ?? [],
              })
            }
          }
          if (Array.isArray(data) && data[0] === 'EOSE' && data[1] === subId) {
            clearTimeout(timer)
            ws.close()
            if (!resolved) { resolved = true; resolve(results) }
          }
        } catch { /* skip */ }
      }

      ws.onerror = () => {
        clearTimeout(timer)
        if (!resolved) { resolved = true; resolve(results) }
      }

      ws.onclose = () => {
        if (!resolved) { resolved = true; resolve(results) }
      }
    } catch {
      clearTimeout(timer)
      resolve(results)
    }
  })
}

function buildTree(replies: NostrNote[], rootId: string, maxDepth = 5): ThreadTreeNode[] {
  const childrenMap = new Map<string, NostrNote[]>()

  for (const reply of replies) {
    // Find the parent — last 'e' tag with 'reply' marker, or last 'e' tag
    let parentId = rootId
    const eTags = reply.tags.filter(t => t[0] === 'e')
    if (eTags.length > 0) {
      const replyTag = eTags.find(t => t[3] === 'reply')
      parentId = replyTag ? replyTag[1] : eTags[eTags.length - 1][1]
    }

    const siblings = childrenMap.get(parentId) ?? []
    siblings.push(reply)
    childrenMap.set(parentId, siblings)
  }

  function build(parentId: string, depth: number): ThreadTreeNode[] {
    const children = childrenMap.get(parentId) ?? []
    children.sort((a, b) => a.created_at - b.created_at)

    return children.map(note => ({
      note,
      children: depth < maxDepth ? build(note.id, depth + 1) : [],
    }))
  }

  return build(rootId, 0)
}

function startReply(note: NostrNote) {
  replyTo.value = note
}

async function sendReply() {
  if (!replyText.value.trim() || !pubkey.value) return

  const target = replyTo.value ?? rootNote.value
  if (!target) return

  const tags: string[][] = [
    ['e', props.noteId, '', 'root'],
  ]
  if (target.id !== props.noteId) {
    tags.push(['e', target.id, '', 'reply'])
  }
  tags.push(['p', target.pubkey])

  const unsigned = {
    kind: 1,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: replyText.value.trim(),
  }

  const signed = await signEvent(unsigned)
  if (!signed) return

  await publishEvent(signed)
  replyText.value = ''
  replyTo.value = null

  // Reload thread
  await loadThread()
}

onMounted(() => {
  loadThread()
})
</script>
