<template>
  <div class="flex flex-col h-full rounded-2xl overflow-visible transition-all duration-300">
    <ChatHeader
      :title="title"
      :conversation-id="displayId"
      :side="side"
      :show-close="showClose"
      @switch-side="$emit('switchSide')"
      @new-chat="handleNewChat"
      @close="$emit('close')"
      @open-settings="showSettings = true"
    />

    <BranchSwitcher />

    <ChatSearch
      ref="chatSearchRef"
      :messages="messages"
      @scroll-to-message="scrollToMessageIndex"
    />

    <ContextBar :messages="messages" :active-model="activeModel" />

    <PersonaSelector />
    <SettingsModal v-model:open="showSettings" />

    <!-- History: full conversation list -->
    <ChatHistory
      v-if="showHistory"
      @select="handleHistorySelect"
      @new-chat="handleNewChat"
    />

    <!-- Collapsed: prompt index -->
    <template v-else-if="chatCollapsed">
      <PromptIndex
        :messages="messages"
        @select="handlePromptSelect"
      />
      <div v-if="isStreaming" class="px-4 pb-3">
        <StreamingDots />
      </div>
    </template>

    <!-- Expanded: full message list (virtualized) -->
    <div
      v-else
      ref="messageListRef"
      class="relative z-0 flex-1 min-h-0 overflow-y-auto scrollbar-hide"
    >
      <div v-if="messages.length === 0" class="flex items-center justify-center h-full p-4">
        <div class="text-center space-y-4 animate-fade-up">
          <div class="empty-state-icon w-16 h-16 rounded-2xl path-glass-icon flex items-center justify-center mx-auto overflow-hidden">
            <span class="text-2xl text-[#fafafa]">✦</span>
          </div>
          <p class="text-sm text-white/30">
            Start a conversation
          </p>
          <router-link
            to="/guide"
            class="inline-flex items-center gap-1.5 text-xs text-white/25 hover:text-white/50 transition-colors"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 6.042A8.967 8.967 0 006 3.75c-1.052 0-2.062.18-3 .512v14.25A8.987 8.987 0 016 18c2.305 0 4.408.867 6 2.292m0-14.25a8.966 8.966 0 016-2.292c1.052 0 2.062.18 3 .512v14.25A8.987 8.987 0 0018 18a8.967 8.967 0 00-6 2.292m0-14.25v14.25" />
            </svg>
            AIUI Guide
          </router-link>
        </div>
      </div>

      <div
        v-else
        :style="{ height: `${virtualizer.getTotalSize()}px`, width: '100%', position: 'relative' }"
      >
        <div
          v-for="virtualRow in virtualizer.getVirtualItems()"
          :key="messages[virtualRow.index].id"
          :ref="(el) => el && virtualizer.measureElement(el as Element)"
          :data-index="virtualRow.index"
          :style="{
            position: 'absolute',
            top: 0,
            left: 0,
            width: '100%',
            transform: `translateY(${virtualRow.start}px)`,
          }"
          :class="['px-4 py-2.5', virtualRow.index === 0 ? 'pt-6' : '']"
        >
          <ErrorBoundary title="Message failed to render">
            <ChatMessage
              :message="messages[virtualRow.index]"
              :index="virtualRow.index"
              :triggering-query="getTriggeringQuery(messages, virtualRow.index)"
              @edit="handleEdit"
              @regenerate="handleRegenerate"
              @branch="handleBranch"
              @reply="handleReply"
              @feedback="handleFeedback"
            />
          </ErrorBoundary>
        </div>
      </div>

      <div v-if="isStreaming && lastMessageEmpty" class="px-4 pb-4">
        <StreamingDots />
      </div>
    </div>

    <!-- Comparison mode split view -->
    <ComparisonView
      v-if="comparison.isComparing.value && (comparison.response1.value || comparison.response2.value)"
      class="flex-1 min-h-0"
    />

    <!-- Code mode project indicator -->
    <div
      v-if="codeContext.isCodeMode.value && codeContext.activeProject.value"
      class="px-3 pb-1 flex items-center gap-1.5"
    >
      <span class="text-xs px-2 py-0.5 rounded-md bg-accent/15 text-accent font-medium truncate max-w-[200px]">
        {{ codeContext.activeProject.value.name }}
      </span>
      <button
        class="text-white/30 hover:text-white/60 transition-colors p-0.5"
        @click="codeContext.exitCodeMode()"
      >
        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <ChatInput
      :disabled="isStreaming || comparison.isAnyStreaming.value"
      :streaming="isStreaming || comparison.isAnyStreaming.value"
      :placeholder="activeTab === 'code' ? 'Code...' : isStreaming ? 'Waiting for response...' : 'Message AIUI...'"
      :active-tab="activeTab"
      :reply-to="replyTo"
      @send="handleSend"
      @extract="handleExtract"
      @stop="handleStop"
      @clear-reply="clearReply"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { useChatStore } from '@/stores/chat'
import { DEMO_CONTENT_ENABLED } from '@/utils/demoContent'
import { useAI, streamWithModel } from '@/composables/useAI'
import { useContentPanel } from '@/composables/useContentPanel'
import { useComparisonMode } from '@/composables/useComparisonMode'
import ChatHeader from './ChatHeader.vue'
import ChatMessage from './ChatMessage.vue'
import ChatInput from './ChatInput.vue'
import StreamingDots from './StreamingDots.vue'
import PromptIndex from './PromptIndex.vue'
import ChatHistory from './ChatHistory.vue'
import BranchSwitcher from './BranchSwitcher.vue'
import ChatSearch from './ChatSearch.vue'
import ContextBar from './ContextBar.vue'
import ComparisonView from './ComparisonView.vue'
import PersonaSelector from './PersonaSelector.vue'
import SettingsModal from './SettingsModal.vue'
import ErrorBoundary from '@/components/ui/ErrorBoundary.vue'
import type { Message, ImageAttachment } from '@aiui/core/types/message'

withDefaults(
  defineProps<{
    variant?: 'standalone' | 'modal'
    side?: 'left' | 'right'
    showClose?: boolean
  }>(),
  {
    variant: 'standalone',
    side: 'right',
    showClose: false,
  }
)

defineEmits<{
  switchSide: []
  close: []
}>()

const chatStore = useChatStore()
const { sendMessage, stopGeneration, editAndResend, regenerateLastResponse, activeModel, needsApiKey, needsFunding } = useAI()
const { updatePanelFromText, panelOpen, panelFilms, panelTitle, activeTab, availableTabs, setActiveTab, enterDesignSystemMode } = useContentPanel()
import { archyBridge } from '@/services/archyBridge'
import { useCodeContext } from '@/composables/useCodeContext'
import { useVisualViewport } from '@/composables/useVisualViewport'
const codeContext = useCodeContext()
import { usePersonaStore } from '@/stores/personas'
const personaStore = usePersonaStore()
const comparison = useComparisonMode()
const messageListRef = ref<HTMLElement | null>(null)
const chatSearchRef = ref<InstanceType<typeof ChatSearch> | null>(null)
const showSettings = ref(false)

// A send/regenerate/edit failure that looks like a missing or invalid API
// key (see useAI.ts's looksLikeMissingApiKey) opens Settings automatically
// instead of leaving the user stuck on a silent/dead error with no obvious
// next step. One-shot: reset immediately after acting so a later retry that
// fails the same way can re-trigger it (the user may have closed Settings
// without fixing anything).
watch(needsApiKey, (needs) => {
  if (needs) {
    if (archyBridge.isInArchy()) archyBridge.requestAISetup()
    else showSettings.value = true
    needsApiKey.value = false
  }
})

watch(needsFunding, needed => {
  if (!needed) return
  if (archyBridge.isInArchy()) archyBridge.requestAISetup('funding')
  else showSettings.value = true
  needsFunding.value = false
})

// Scroll position memory per conversation
const scrollPositions = new Map<string, number>()

function saveScrollPosition() {
  const el = messageListRef.value
  const id = chatStore.activeConversationId
  if (el && id) {
    scrollPositions.set(id, el.scrollTop)
  }
}

function restoreScrollPosition(convId: string) {
  const saved = scrollPositions.get(convId)
  if (saved !== undefined) {
    nextTick(() => {
      const el = messageListRef.value
      if (el) el.scrollTop = saved
    })
  }
}

function scrollToMessageIndex(index: number) {
  virtualizer.value.scrollToIndex(index, { align: 'center' })
}

// Reply-to threading state
const replyTo = ref<{ messageId: string; excerpt: string } | null>(null)

function handleReply(messageId: string, content: string) {
  const excerpt = content.slice(0, 100) + (content.length > 100 ? '...' : '')
  replyTo.value = { messageId, excerpt }
}

function clearReply() {
  replyTo.value = null
}

const messages = computed(() => chatStore.messages)

const virtualizer = useVirtualizer(computed(() => ({
  count: messages.value.length,
  getScrollElement: () => messageListRef.value,
  estimateSize: (index: number) => {
    // User messages are typically shorter
    return messages.value[index]?.role === 'user' ? 60 : 200
  },
  overscan: 5,
})))
const isStreaming = computed(() => chatStore.isStreaming)
const chatCollapsed = computed(() => chatStore.chatCollapsed)
const showHistory = computed(() => chatStore.showHistory)

const title = computed(
  () => chatStore.activeConversation?.title ?? 'New Chat'
)

const displayId = computed(
  () => chatStore.activeConversationId?.slice(0, 8) ?? '—'
)

const lastMessageEmpty = computed(() => {
  const msgs = messages.value
  if (msgs.length === 0) return true
  return msgs[msgs.length - 1].content === ''
})

function getTriggeringQuery(msgs: typeof messages.value, idx: number): string {
  if (msgs[idx]?.role !== 'assistant') return ''
  for (let i = idx - 1; i >= 0; i--) {
    if (msgs[i]?.role === 'user') return msgs[i].content ?? ''
  }
  return ''
}

function handleNewChat() {
  chatStore.createConversation('New Chat', personaStore.defaultPersona?.id)
  chatStore.showHistory = false
}

function handleHistorySelect(id: string) {
  chatStore.setActiveConversation(id)
  chatStore.showHistory = false
}

function handleStop() {
  stopGeneration()
  comparison.stopComparison()
}

async function handleEdit(messageId: string, newContent: string) {
  await editAndResend(messageId, newContent)
}

async function handleRegenerate() {
  await regenerateLastResponse()
}

function handleBranch(messageId: string) {
  const convId = chatStore.activeConversationId
  if (!convId) return
  chatStore.branchFromMessage(convId, messageId)
}

function handleFeedback(messageId: string, value: 'up' | 'down' | undefined) {
  const convId = chatStore.activeConversationId
  if (!convId) return
  chatStore.setMessageFeedback(convId, messageId, value)
}

function handleExtract(text: string) {
  // Run content extraction without sending to AI
  const convId = chatStore.activeConversationId ?? chatStore.createConversation()
  chatStore.addMessage(convId, { role: 'user', content: `[Extract] ${text.slice(0, 80)}${text.length > 80 ? '…' : ''}` })
  chatStore.addMessage(convId, { role: 'assistant', content: text })
  updatePanelFromText(text, '', [])
}

async function handleSend(text: string, images: ImageAttachment[] = []) {
  // Command handling
  const trimmed = text.trim().toLowerCase()
  if (trimmed === '/code') {
    codeContext.enterCodeMode()
    // Open the content panel with code tab
    panelOpen.value = true
    if (!availableTabs.value.includes('code')) {
      availableTabs.value = [...availableTabs.value, 'code']
    }
    setActiveTab('code')
    // Add system message to chat
    const convId = chatStore.activeConversationId
    if (convId) {
      chatStore.addMessage(convId, {
        role: 'user',
        content: '/code',
      })
      chatStore.addMessage(convId, {
        role: 'assistant',
        content: 'Code mode activated. Select a project from the content panel to start coding.',
      })
    }
    return
  }

  if (trimmed === '/seed') {
    // Demo-site/dev only (S7): the seed showcase is fabricated example
    // content and must not ship in the node build.
    if (!DEMO_CONTENT_ENABLED) {
      const convId = chatStore.activeConversationId
      if (convId) {
        chatStore.addMessage(convId, {
          role: 'assistant',
          content: 'The `/seed` showcase is only available in demo builds.',
        })
      }
      return
    }
    await chatStore.loadSeedChats()
    // loadSeedChats already switches to the seed conversation
    // Collapse chat so user sees PromptIndex for quick picking
    chatStore.chatCollapsed = true
    chatStore.showHistory = false
    return
  }

  if (trimmed === '/nostr') {
    panelOpen.value = true
    if (!availableTabs.value.includes('nostr')) {
      availableTabs.value = [...availableTabs.value, 'nostr']
    }
    setActiveTab('nostr')
    const convId = chatStore.activeConversationId
    if (convId) {
      chatStore.addMessage(convId, { role: 'user', content: '/nostr' })
      chatStore.addMessage(convId, { role: 'assistant', content: 'Nostr feed opened. Browse notes, articles, and zaps from the network.' })
    }
    return
  }

  if (trimmed === '/design') {
    enterDesignSystemMode()
    const convId = chatStore.activeConversationId
    if (convId) {
      chatStore.addMessage(convId, { role: 'user', content: '/design' })
      chatStore.addMessage(convId, { role: 'assistant', content: 'Design system viewer opened. Browse colors, typography, spacing, and components.' })
    }
    return
  }

  if (trimmed === '/freefilms') {
    const { freeFilms } = await import('@/data/freeFilms')
    panelFilms.value = freeFilms
    panelTitle.value = 'Free Documentary Films'
    panelOpen.value = true
    availableTabs.value = ['film', 'prompt']
    setActiveTab('film')
    const convId = chatStore.activeConversationId
    if (convId) {
      chatStore.addMessage(convId, { role: 'user', content: '/freefilms' })
      chatStore.addMessage(convId, { role: 'assistant', content: `Browse ${freeFilms.length} free documentary films from InDeeHub. Click any film to see details, and hit play to watch.` })
    }
    return
  }

  if (trimmed === '/code exit' || trimmed === '/exit') {
    if (codeContext.isCodeMode.value) {
      codeContext.exitCodeMode()
      availableTabs.value = availableTabs.value.filter(t => t !== 'code')
      const convId2 = chatStore.activeConversationId
      if (convId2) {
        chatStore.addMessage(convId2, {
          role: 'assistant',
          content: 'Code mode deactivated.',
        })
      }
      return
    }
  }

  // Prepend quote if replying to a message
  let finalText = text
  if (replyTo.value) {
    const quoteLine = replyTo.value.excerpt.split('\n').map(l => `> ${l}`).join('\n')
    finalText = `${quoteLine}\n\n${text}`
    clearReply()
  }

  // Comparison mode: stream to both models simultaneously
  if (comparison.isComparing.value) {
    const convId = chatStore.activeConversationId ?? chatStore.createConversation()
    chatStore.addMessage(convId, { role: 'user', content: finalText, images: images.length > 0 ? images : undefined })
    const history = chatStore.messages.map(m => ({ role: m.role, content: m.content }))
    await comparison.streamBothModels(streamWithModel, history)
    return
  }

  await sendMessage(finalText, images.length > 0 ? images : undefined)
}

function handlePromptSelect(_userMsg: Message, assistantMsg: Message | null) {
  const userText = _userMsg.content?.trim().toLowerCase() ?? ''
  if (userText === '/code') {
    codeContext.enterCodeMode()
    panelOpen.value = true
    if (!availableTabs.value.includes('code')) {
      availableTabs.value = [...availableTabs.value, 'code']
    }
    setActiveTab('code')
    return
  }
  if (assistantMsg?.content) {
    updatePanelFromText(assistantMsg.content, _userMsg.content, assistantMsg.webResults ?? [])
  }
}

function scrollToBottom() {
  // Container scroll ONLY — deliberately not virtualizer.scrollToIndex.
  //
  // Doing both is what produced "Failed to scroll to index N after 10
  // attempts" in the console on every send. scrollToIndex runs a retry loop
  // that nudges scrollTop toward the measured offset of the target row and
  // re-checks, up to 10 times, because dynamically-measured rows move the
  // target as they settle. Assigning scrollTop ourselves on the next tick
  // overwrote each of those nudges, so the loop never saw itself converge and
  // always exhausted its attempts — the warning was the two of us fighting
  // over the same scrollTop, not a real failure.
  //
  // For "go to the end" the virtualizer's index-settling machinery buys
  // nothing: scrollHeight is already the bottom, and the virtualizer renders
  // whatever window that offset implies. It also keeps working while a
  // response streams and the last row keeps growing, which is exactly the
  // case the old fallback was added for. scrollToMessageIndex still uses
  // scrollToIndex, which is the right tool for jumping to an arbitrary row.
  nextTick(() => {
    const el = messageListRef.value
    if (el) el.scrollTop = el.scrollHeight
  })
}

// Scroll to bottom when expanding from collapsed
watch(chatCollapsed, (collapsed, wasCollapsed) => {
  if (!collapsed && wasCollapsed) {
    scrollToBottom()
  }
})

// Scroll to bottom when mobile keyboard opens so latest messages + input stay visible
const { isKeyboardOpen } = useVisualViewport()
watch(isKeyboardOpen, (open) => {
  if (open) scrollToBottom()
})

// Save/restore scroll position on conversation switch
watch(
  () => chatStore.activeConversationId,
  (newId, oldId) => {
    if (oldId) saveScrollPosition()
    if (newId) restoreScrollPosition(newId)
  }
)

watch(
  () => messages.value.length,
  () => scrollToBottom()
)

watch(
  () => {
    const msgs = messages.value
    const last = msgs[msgs.length - 1]
    return last ? { content: last.content, webResults: last.webResults } : null
  },
  (val) => {
    scrollToBottom()
    if (val?.content) {
      const msgs = messages.value
      const lastMsg = msgs[msgs.length - 1]
      const lastUser = [...msgs].reverse().find((m) => m.role === 'user')
      // Skip panel updates for command messages (e.g. /code, /exit)
      const userText = lastUser?.content?.trim() ?? ''
      if (!userText.startsWith('/')) {
        updatePanelFromText(val.content, userText, lastMsg?.webResults ?? [])
      }
    }
  },
  { deep: true, immediate: true }
)
</script>
