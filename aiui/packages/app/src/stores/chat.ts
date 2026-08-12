import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import type { Message, Conversation, WebSearchResult } from '@aiui/core/types/message'
import { apiFetch } from '@/utils/api-fetch'
import { DEMO_CONTENT_ENABLED } from '@/utils/demoContent'

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  // Fallback for non-secure contexts (HTTP over network IP)
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}
import {
  saveConversation as idbSave,
  loadAllConversations as idbLoadAll,
  deleteConversation as idbDelete,
  isIDBAvailable,
} from '@/utils/idb-storage'

const isDev = import.meta.env.DEV
const useIDB = isIDBAvailable()

// ─── Dev-chats server middleware (file-based fallback) ────────

let _loaded = false
let devSaveTimer: ReturnType<typeof setTimeout> | null = null
const DEV_SAVE_DEBOUNCE = 800

async function loadServerChats(): Promise<{ conversations: Map<string, Conversation>; activeId: string | null }> {
  const empty = { conversations: new Map<string, Conversation>(), activeId: null }
  if (!isDev) return empty
  try {
    const res = await apiFetch('/api/dev-chats')
    if (!res.ok) return empty
    const data = await res.json() as {
      conversations?: Record<string, Conversation>
      activeConversationId?: string | null
    }
    if (!data.conversations) return empty
    const conversations = new Map(Object.entries(data.conversations))
    return {
      conversations,
      activeId: data.activeConversationId ?? null,
    }
  } catch {
    return empty
  }
}

function saveServerChats(conversations: Map<string, Conversation>, activeId: string | null) {
  if (!isDev || !_loaded) return
  if (devSaveTimer) clearTimeout(devSaveTimer)
  devSaveTimer = setTimeout(() => {
    const obj = Object.fromEntries(conversations)
    apiFetch('/api/dev-chats', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ conversations: obj, activeConversationId: activeId }),
    }).catch(() => {})
  }, DEV_SAVE_DEBOUNCE)
}

// ─── IDB save with debounce + flush-on-unload ────────────────

const idbSaveTimers = new Map<string, ReturnType<typeof setTimeout>>()
const pendingSaves = new Map<string, Conversation>()

function debouncedIDBSave(conv: Conversation) {
  if (!useIDB) return
  pendingSaves.set(conv.id, conv)
  const existing = idbSaveTimers.get(conv.id)
  if (existing) clearTimeout(existing)
  idbSaveTimers.set(conv.id, setTimeout(() => {
    idbSave(conv).then(() => {
      pendingSaves.delete(conv.id)
    }).catch((err) => {
      console.warn('[chat] IDB save failed:', err)
    })
    idbSaveTimers.delete(conv.id)
  }, 800))
}

/** Immediately save a conversation to IDB (no debounce). */
function immediateIDBSave(conv: Conversation) {
  if (!useIDB) return
  pendingSaves.delete(conv.id)
  const existing = idbSaveTimers.get(conv.id)
  if (existing) clearTimeout(existing)
  idbSaveTimers.delete(conv.id)
  idbSave(conv).catch((err) => {
    console.warn('[chat] IDB save failed:', err)
  })
}

/** Flush all pending debounced saves — called on page unload. */
function flushPendingSaves() {
  for (const [id, timer] of idbSaveTimers) {
    clearTimeout(timer)
    idbSaveTimers.delete(id)
  }
  for (const [id, conv] of pendingSaves) {
    idbSave(conv).catch(() => {})
    pendingSaves.delete(id)
  }
}

// Flush pending saves before the page unloads
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', flushPendingSaves)
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      flushPendingSaves()
    }
  })
}

// ─── Store definition ─────────────────────────────────────────

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Map<string, Conversation>>(new Map())
  const activeConversationId = ref<string | null>(null)
  const isStreaming = ref(false)
  const loaded = ref(false)

  const savedSide = localStorage.getItem('aiui-panel-side') as 'left' | 'right' | null
  const panelSide = ref<'left' | 'right'>(savedSide ?? 'left')

  const webSearchEnabled = ref(localStorage.getItem('aiui-web-search') !== 'false')

  // D-14a (Archipelago phase 02-ui-performance): when the host explicitly asks
  // for an expanded start via ?chatExpanded, open in the full message list
  // rather than the collapsed prompt-index — this is a presentation-only,
  // one-time initial-state override and is never written back to
  // localStorage, so it can never change the standalone (non-embedded) app's
  // own persisted default. A user who manually collapses while embedded still
  // gets that choice persisted for next time, same as before this change.
  const _startExpanded = new URLSearchParams(window.location.search).has('chatExpanded')
  const chatCollapsed = ref(_startExpanded ? false : localStorage.getItem('aiui-chat-collapsed') !== 'false')
  const showHistory = ref(false)

  // Load chats: try IndexedDB first, fall back to dev-chats middleware
  async function loadChats() {
    if (useIDB) {
      try {
        const idbConversations = await idbLoadAll()
        if (idbConversations.size > 0) {
          conversations.value = idbConversations
          const savedActiveId = localStorage.getItem('aiui-active-conversation')
          activeConversationId.value =
            savedActiveId && idbConversations.has(savedActiveId)
              ? savedActiveId
              : [...idbConversations.keys()].pop() ?? null
          loaded.value = true
          _loaded = true
          return
        }
      } catch (err) {
        console.warn('[chat] IDB load failed, falling back:', err)
      }
    }

    // Fall through: IDB empty or unavailable — try dev-chats seed
    if (isDev) {
      const data = await loadServerChats()
      if (data.conversations.size > 0) {
        conversations.value = data.conversations
        activeConversationId.value =
          data.activeId && data.conversations.has(data.activeId)
            ? data.activeId
            : [...data.conversations.keys()][0] ?? null
        // Migrate seed data to IDB immediately (no debounce)
        if (useIDB) {
          for (const conv of data.conversations.values()) {
            immediateIDBSave(conv)
          }
        }
      }
    }
    loaded.value = true
    _loaded = true
  }

  loadChats().then(() => {
    // Seed guide + node demo conversations on first use
    seedDemoConversations()
  })

  // Persist active conversation ID to localStorage
  watch(activeConversationId, (id) => {
    if (id) localStorage.setItem('aiui-active-conversation', id)
    else localStorage.removeItem('aiui-active-conversation')
  })

  watch(panelSide, (val) => {
    localStorage.setItem('aiui-panel-side', val)
  })

  watch(webSearchEnabled, (val) => {
    localStorage.setItem('aiui-web-search', String(val))
  })

  watch(chatCollapsed, (val) => {
    localStorage.setItem('aiui-chat-collapsed', String(val))
  })

  watch(
    [conversations, activeConversationId],
    ([conv, active]) => {
      // Dev-chats middleware fallback
      saveServerChats(conv as Map<string, Conversation>, active as string | null)
    },
    { deep: true }
  )

  const activeConversation = computed(() => {
    if (!activeConversationId.value) return null
    return conversations.value.get(activeConversationId.value) ?? null
  })

  const messages = computed(() => activeConversation.value?.messages ?? [])

  const conversationList = computed(() =>
    Array.from(conversations.value.values()).sort((a, b) => b.updatedAt - a.updatedAt)
  )

  function createConversation(title = 'New Chat', personaId?: string): string {
    const id = generateId()
    const conversation: Conversation = {
      id,
      title,
      messages: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      personaId,
    }
    conversations.value.set(id, conversation)
    activeConversationId.value = id
    immediateIDBSave(conversation)
    return id
  }

  function addMessage(conversationId: string, message: Omit<Message, 'id' | 'timestamp'>) {
    const conv = conversations.value.get(conversationId)
    if (!conv) return

    const msg: Message = {
      ...message,
      id: generateId(),
      timestamp: Date.now(),
    }
    conv.messages.push(msg)
    conv.updatedAt = Date.now()

    if (conv.messages.length === 1 && message.role === 'user') {
      conv.title = message.content.slice(0, 60) + (message.content.length > 60 ? '...' : '')
    }

    debouncedIDBSave(conv)
    return msg
  }

  function appendToLastMessage(conversationId: string, text: string) {
    const conv = conversations.value.get(conversationId)
    if (!conv || conv.messages.length === 0) return
    const last = conv.messages[conv.messages.length - 1]
    last.content += text
    debouncedIDBSave(conv)
  }

  function setMessageWebResults(conversationId: string, messageId: string, results: WebSearchResult[]) {
    const conv = conversations.value.get(conversationId)
    if (!conv) return
    const msg = conv.messages.find((m: { id: string }) => m.id === messageId)
    if (msg) {
      msg.webResults = results
      debouncedIDBSave(conv)
    }
  }

  function setMessageFeedback(conversationId: string, messageId: string, feedback: 'up' | 'down' | undefined) {
    const conv = conversations.value.get(conversationId)
    if (!conv) return
    const msg = conv.messages.find((m: { id: string }) => m.id === messageId)
    if (msg) {
      msg.feedback = feedback
      debouncedIDBSave(conv)
    }
  }

  function switchSide() {
    panelSide.value = panelSide.value === 'right' ? 'left' : 'right'
  }

  function toggleChatCollapse() {
    chatCollapsed.value = !chatCollapsed.value
  }

  function toggleHistory() {
    showHistory.value = !showHistory.value
  }

  function setActiveConversation(id: string) {
    if (conversations.value.has(id)) {
      activeConversationId.value = id
    }
  }

  function deleteConversation(id: string) {
    conversations.value.delete(id)
    if (useIDB) idbDelete(id).catch(() => {})
    pendingSaves.delete(id)
    if (activeConversationId.value === id) {
      const remaining = conversationList.value
      activeConversationId.value = remaining.length > 0 ? remaining[0].id : null
    }
  }

  /** Update a message's content (for editing) */
  function updateMessageContent(conversationId: string, messageId: string, newContent: string) {
    const conv = conversations.value.get(conversationId)
    if (!conv) return
    const msg = conv.messages.find((m: { id: string }) => m.id === messageId)
    if (msg) {
      msg.content = newContent
      msg.editedAt = Date.now()
      conv.updatedAt = Date.now()
      debouncedIDBSave(conv)
    }
  }

  /** Delete all messages after the given index (inclusive of afterIndex) */
  function deleteMessagesAfter(conversationId: string, afterIndex: number) {
    const conv = conversations.value.get(conversationId)
    if (!conv) return
    conv.messages.splice(afterIndex)
    conv.updatedAt = Date.now()
    debouncedIDBSave(conv)
  }

  /** Branch from a message — creates a new conversation with messages up to (and including) the given message */
  function branchFromMessage(conversationId: string, messageId: string): string | null {
    const conv = conversations.value.get(conversationId)
    if (!conv) return null

    const msgIndex = conv.messages.findIndex((m: { id: string }) => m.id === messageId)
    if (msgIndex === -1) return null

    const branchId = generateId()
    const branchMessages = conv.messages.slice(0, msgIndex + 1).map(m => ({
      ...m,
      id: generateId(),
      timestamp: m.timestamp,
    }))

    // Count existing branches to label them
    const existingBranches = (conv.childBranchIds ?? []).length
    const branchConv: Conversation = {
      id: branchId,
      title: `${conv.title} (Branch ${existingBranches + 2})`,
      messages: branchMessages,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      model: conv.model,
      systemPrompt: conv.systemPrompt,
      parentConversationId: conversationId,
      branchPoint: messageId,
    }

    conversations.value.set(branchId, branchConv)
    immediateIDBSave(branchConv)

    // Track branch in parent
    if (!conv.childBranchIds) conv.childBranchIds = []
    conv.childBranchIds.push(branchId)
    conv.updatedAt = Date.now()
    debouncedIDBSave(conv)

    activeConversationId.value = branchId
    return branchId
  }

  /** Get sibling branches for a conversation (parent + all its children) */
  function getSiblingBranches(conversationId: string): { id: string; title: string; isCurrent: boolean }[] {
    const conv = conversations.value.get(conversationId)
    if (!conv) return []

    // Find the family (parent + children)
    const parentId = conv.parentConversationId ?? conversationId
    const parent = conversations.value.get(parentId)
    if (!parent) return []

    const siblings: { id: string; title: string; isCurrent: boolean }[] = [
      { id: parentId, title: parent.title, isCurrent: parentId === conversationId },
    ]

    for (const childId of parent.childBranchIds ?? []) {
      const child = conversations.value.get(childId)
      if (child) {
        siblings.push({ id: childId, title: child.title, isCurrent: childId === conversationId })
      }
    }

    return siblings
  }

  /** Load all seed prompts as a single conversation — demo builds only (S7) */
  async function loadSeedChats(): Promise<number> {
    if (!DEMO_CONTENT_ENABLED) return 0
    try {
      const { seedPromptsToConversation } = await import('@/__tests__/fixtures/seedPrompts')
      const conv = seedPromptsToConversation() as Conversation

      if (conversations.value.has(conv.id)) {
        // Already loaded — just switch to it
        activeConversationId.value = conv.id
        return 0
      }

      const merged = new Map(conversations.value)
      merged.set(conv.id, conv)
      conversations.value = merged
      activeConversationId.value = conv.id
      immediateIDBSave(conv)
      return conv.messages.length / 2 // number of prompt/response pairs
    } catch {
      return 0
    }
  }

  /** Load node demo conversation — demo builds only (S7); the content is
   *  fabricated node state and must not ship on real nodes */
  async function loadNodeDemoChat(): Promise<number> {
    if (!DEMO_CONTENT_ENABLED) return 0
    try {
      const { nodeDemoToConversation } = await import('@/__tests__/fixtures/nodeDemoPrompts')
      const conv = nodeDemoToConversation() as Conversation

      if (conversations.value.has(conv.id)) {
        activeConversationId.value = conv.id
        return 0
      }

      const merged = new Map(conversations.value)
      merged.set(conv.id, conv)
      conversations.value = merged
      activeConversationId.value = conv.id
      immediateIDBSave(conv)
      return conv.messages.length / 2
    } catch {
      return 0
    }
  }

  /** Load guide conversation (auto-loaded on first visit) */
  async function loadGuide(): Promise<void> {
    try {
      const { guideToConversation } = await import('@/__tests__/fixtures/guideConversation')
      const conv = guideToConversation() as Conversation

      if (conversations.value.has(conv.id)) {
        activeConversationId.value = conv.id
        return
      }

      const merged = new Map(conversations.value)
      merged.set(conv.id, conv)
      conversations.value = merged
      activeConversationId.value = conv.id
      immediateIDBSave(conv)
    } catch {
      // ignore
    }
  }

  /** Seed demo conversations (guide always; node demo only in demo builds) */
  async function seedDemoConversations(): Promise<void> {
    // The guide is genuine onboarding documentation — always seed it.
    // `node-demo` is FABRICATED node state (invented balances, file listings,
    // a fake bitcoin.conf) — demo-site/dev builds only (S7, operator
    // decision 2026-08-07: mocks belong to the demo site, not shipped code).
    const wantDemo = DEMO_CONTENT_ENABLED
    const hasGuide = conversations.value.has('aiui-guide')
    const hasDemo = conversations.value.has('node-demo')
    if (hasGuide && (hasDemo || !wantDemo)) {
      // Already seeded — still select guide if nothing active
      if (!activeConversationId.value) {
        activeConversationId.value = 'aiui-guide'
      }
      return
    }
    try {
      const { guideToConversation } = await import('@/__tests__/fixtures/guideConversation')
      const guide = guideToConversation() as Conversation
      const merged = new Map(conversations.value)
      if (!merged.has(guide.id)) {
        merged.set(guide.id, guide)
        immediateIDBSave(guide)
      }
      if (wantDemo && !merged.has('node-demo')) {
        const { nodeDemoToConversation } = await import('@/__tests__/fixtures/nodeDemoPrompts')
        const demo = nodeDemoToConversation() as Conversation
        merged.set(demo.id, demo)
        immediateIDBSave(demo)
      }
      conversations.value = merged
      // Show guide conversation on first load
      if (!activeConversationId.value) {
        activeConversationId.value = guide.id
      }
    } catch {
      // ignore
    }
  }

  return {
    conversations,
    activeConversationId,
    activeConversation,
    messages,
    conversationList,
    isStreaming,
    loaded,
    panelSide,
    webSearchEnabled,
    chatCollapsed,
    showHistory,
    createConversation,
    addMessage,
    appendToLastMessage,
    setMessageWebResults,
    setMessageFeedback,
    switchSide,
    toggleChatCollapse,
    toggleHistory,
    setActiveConversation,
    deleteConversation,
    updateMessageContent,
    deleteMessagesAfter,
    branchFromMessage,
    getSiblingBranches,
    loadSeedChats,
    loadNodeDemoChat,
    loadGuide,
    seedDemoConversations,
  }
})
