<template>
  <div class="min-h-screen bg-[#0a0a0a] text-white">
    <!-- Header -->
    <header class="sticky top-0 z-10 glass border-b border-white/5">
      <div class="max-w-3xl mx-auto px-4 py-3 flex items-center gap-3">
        <router-link to="/" class="text-white/40 hover:text-white/70 transition-colors">
          <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M10 19l-7-7m0 0l7-7m-7 7h18" />
          </svg>
        </router-link>
        <div class="flex-1 min-w-0">
          <h1 class="text-sm font-semibold text-white/90 truncate">{{ title }}</h1>
          <p class="text-xs text-white/40">
            <template v-if="authorName">by {{ authorName }}</template>
            <template v-if="publishedAt"> · {{ formattedDate }}</template>
          </p>
        </div>
        <span class="text-xs px-2 py-1 rounded-full bg-white/5 text-white/40">Read-only</span>
      </div>
    </header>

    <!-- Loading -->
    <div v-if="isLoading" class="flex items-center justify-center h-64">
      <div class="w-6 h-6 rounded-full border-2 border-accent/30 border-t-accent animate-spin" />
    </div>

    <!-- Error -->
    <div v-else-if="error" class="max-w-3xl mx-auto px-4 py-12 text-center">
      <p class="text-white/40 text-sm">{{ error }}</p>
      <router-link to="/" class="mt-4 inline-block text-accent text-sm hover:underline">
        Go to AIUI
      </router-link>
    </div>

    <!-- Content -->
    <main v-else class="max-w-3xl mx-auto px-4 py-6 space-y-4">
      <div
        v-for="(msg, i) in messages"
        :key="i"
        class="rounded-xl p-4"
        :class="msg.role === 'user'
          ? 'bg-white/[0.03] border border-white/5 ml-8'
          : 'mr-8'"
      >
        <div class="flex items-center gap-2 mb-2">
          <span
            class="text-xs font-bold uppercase tracking-wider"
            :class="msg.role === 'user' ? 'text-accent/70' : 'text-white/30'"
          >
            {{ msg.role === 'user' ? 'Human' : 'Assistant' }}
          </span>
        </div>
        <div
          class="text-sm text-white/80 leading-relaxed whitespace-pre-wrap break-words"
          v-text="msg.content"
        />
      </div>
    </main>

    <!-- Footer -->
    <footer class="max-w-3xl mx-auto px-4 py-8 text-center">
      <p class="text-xs text-white/20">
        Shared via AIUI · Powered by Nostr
      </p>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { useNostr } from '@/composables/useNostr'

const route = useRoute()
const { connect, fetchNote } = useNostr()

const isLoading = ref(true)
const error = ref<string | null>(null)
const title = ref('Shared Conversation')
const authorName = ref<string | null>(null)
const publishedAt = ref<number | null>(null)
const messages = ref<{ role: string; content: string }[]>([])

const formattedDate = computed(() => {
  if (!publishedAt.value) return ''
  return new Date(publishedAt.value * 1000).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })
})

function parseMarkdownConversation(markdown: string): { role: string; content: string }[] {
  const msgs: { role: string; content: string }[] = []
  const lines = markdown.split('\n')
  let currentRole = ''
  let currentContent: string[] = []

  for (const line of lines) {
    const userMatch = line.match(/^##?\s*(?:Human|User|You)/)
    const assistantMatch = line.match(/^##?\s*(?:Assistant|AI|Claude)/)

    if (userMatch || assistantMatch) {
      if (currentRole && currentContent.length > 0) {
        msgs.push({ role: currentRole, content: currentContent.join('\n').trim() })
      }
      currentRole = userMatch ? 'user' : 'assistant'
      currentContent = []
    } else {
      currentContent.push(line)
    }
  }

  if (currentRole && currentContent.length > 0) {
    msgs.push({ role: currentRole, content: currentContent.join('\n').trim() })
  }

  // If no structured format detected, treat entire content as a single message
  if (msgs.length === 0 && markdown.trim()) {
    msgs.push({ role: 'assistant', content: markdown.trim() })
  }

  return msgs
}

onMounted(async () => {
  try {
    const nostrAddr = route.params.nostrAddr as string
    if (!nostrAddr) {
      error.value = 'No Nostr address provided.'
      return
    }

    await connect()

    // Attempt to decode the simplified naddr
    let decoded: { dTag: string; pubkey: string } | null = null
    try {
      const raw = atob(nostrAddr)
      const parts = raw.split(':')
      if (parts.length >= 2) {
        decoded = { dTag: parts[0], pubkey: parts[1] }
      }
    } catch {
      // Try fetching as a hex ID
    }

    if (decoded) {
      // Fetch by d-tag from relays
      const note = await fetchNote(decoded.dTag)
      if (note) {
        const titleTag = note.tags.find((t) => t[0] === 'title')
        if (titleTag) title.value = titleTag[1]
        authorName.value = note.authorName ?? null
        publishedAt.value = note.created_at
        messages.value = parseMarkdownConversation(note.content)
      } else {
        error.value = 'Conversation not found on relays.'
      }
    } else {
      error.value = 'Invalid Nostr address format.'
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load conversation.'
  } finally {
    isLoading.value = false
  }
})
</script>
