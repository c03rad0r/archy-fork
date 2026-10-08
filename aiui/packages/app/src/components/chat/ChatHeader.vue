<template>
  <div
    ref="headerRef"
    class="flex flex-col gap-0 p-3 relative z-[60] shrink-0"
  >
    <div class="flex items-center justify-between gap-2">
      <button
        ref="modelPickerTriggerRef"
        class="touch-target rounded-xl path-glass-icon shrink-0 transition-colors cursor-pointer text-[#fafafa] hover:text-white"
        :title="`AI model: ${modelDisplayName}`"
        aria-label="Select AI model"
        @click="showModelPicker = !showModelPicker"
      >
        <span class="text-base">✦</span>
      </button>
      <div class="flex items-center gap-2 shrink-0">
      <button
        class="touch-target rounded-xl path-glass-icon transition-colors"
        :class="chatStore.showHistory
          ? 'text-accent'
          : 'text-white/70 hover:text-white'"
        :title="chatStore.showHistory ? 'Back to chat' : 'Chat history'"
        aria-label="Toggle chat history"
        @click="chatStore.toggleHistory()"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      </button>

      <button
        class="touch-target rounded-xl path-glass-icon transition-colors"
        :class="chatStore.chatCollapsed
          ? 'text-accent'
          : 'text-white/70 hover:text-white'"
        :title="chatStore.chatCollapsed ? 'Expand chat' : 'Prompt index'"
        aria-label="Toggle prompt index"
        @click="chatStore.toggleChatCollapse()"
      >
        <svg v-if="!chatStore.chatCollapsed" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h10M4 18h16" />
        </svg>
        <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
      </button>

      <button
        class="touch-target rounded-xl path-glass-icon transition-colors"
        :class="comparison.isComparing.value
          ? 'text-accent'
          : 'text-white/70 hover:text-white'"
        :title="comparison.isComparing.value ? 'Comparison mode on' : 'Compare models'"
        aria-label="Toggle model comparison"
        @click="comparison.toggleComparison()"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 17V7m0 10a2 2 0 01-2 2H5a2 2 0 01-2-2V7a2 2 0 012-2h2a2 2 0 012 2m0 10a2 2 0 002 2h2a2 2 0 002-2M9 7a2 2 0 012-2h2a2 2 0 012 2m0 10V7" />
        </svg>
      </button>

      <button
        class="touch-target rounded-xl path-glass-icon transition-colors text-white/70 hover:text-white"
        aria-label="Settings"
        title="Settings"
        @click="$emit('openSettings')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>

      <button
        ref="menuTriggerRef"
        class="touch-target rounded-xl path-glass-icon transition-colors text-white/70 hover:text-white"
        aria-label="Conversation menu"
        @click="showMenu = !showMenu"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
          <path d="M10 6a2 2 0 110-4 2 2 0 010 4zM10 12a2 2 0 110-4 2 2 0 010 4zM10 18a2 2 0 110-4 2 2 0 010 4z" />
        </svg>
      </button>

      <button
        class="touch-target rounded-xl path-glass-icon transition-colors text-white/70 hover:text-white"
        aria-label="New conversation"
        @click="$emit('newChat')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
      </button>

      <button
        v-if="showClose"
        class="touch-target rounded-xl path-glass-icon transition-colors text-white/70 hover:text-white"
        aria-label="Close"
        @click="$emit('close')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
      </div>
    </div>

    <div class="w-full text-left pt-3 pb-1 min-w-0">
      <h2 class="text-sm font-semibold truncate text-white/96">{{ title }}</h2>
      <div class="flex items-center gap-1.5 mt-0.5">
        <p class="text-xs truncate font-mono text-white/40">{{ conversationId }}</p>
        <span class="text-xs text-white/20">·</span>
        <span class="text-xs truncate text-white/50">{{ modelDisplayName }}</span>
      </div>
    </div>

    <Teleport to="body">
      <div v-if="showModelPicker" class="fixed inset-0 z-[9998]" aria-hidden="true" @click="showModelPicker = false" />
      <Transition name="picker">
        <div
          v-if="showModelPicker"
          class="fixed z-[9999] path-glass-card header-overlay-panel p-3 space-y-3 animate-fade-up-fast shadow-2xl min-w-[220px] max-h-[70vh] overflow-y-auto"
          :style="modelPickerDropdownStyle"
          @click.stop
        >
        <button v-if="archyBridge.isInArchy()" class="w-full text-left rounded-lg px-3 py-2 text-sm text-white/90 hover:bg-white/10" @click="showModelPicker = false; archyBridge.requestAISetup()">AI connection</button>
        <div v-for="provider in availableProviders" :key="provider.id">
          <p class="text-xs font-semibold uppercase tracking-wider mb-1.5 px-1 text-white/40">
            {{ provider.name }}
          </p>
          <div class="space-y-0.5">
            <button
              v-for="model in provider.models"
              :key="model.id"
              class="w-full text-left px-3 py-2 rounded-lg text-xs transition-all duration-200 flex items-center gap-2"
              :class="model.id === activeModel && provider.id === activeProvider
                ? 'nav-tab-active'
                : 'text-white/60 hover:text-white hover:bg-white/10'"
              @click="selectModel(provider.id, model.id)"
            >
              <span class="flex-1">{{ model.name }}</span>
              <span class="flex gap-0.5 shrink-0">
                <span
                  v-if="getModelCaps(model.id).vision"
                  class="text-xs opacity-60"
                  title="Supports vision input"
                >👁</span>
                <span
                  v-if="getModelCaps(model.id).tools"
                  class="text-xs opacity-60"
                  title="Supports tool use"
                >🔧</span>
                <span
                  v-if="getModelCaps(model.id).longContext"
                  class="text-xs opacity-60"
                  title="Long context window"
                >📄</span>
              </span>
            </button>
          </div>
        </div>
          <!-- Design System -->
          <div class="border-t mt-2 pt-2 border-white/10">
            <button
              class="w-full text-left px-3 py-2 rounded-lg text-xs transition-all duration-200 flex items-center gap-2 text-white/60 hover:text-white hover:bg-white/10"
              @click="openDesignSystem"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01" />
              </svg>
              Design System
            </button>
          </div>
      </div>
      </Transition>
    </Teleport>

    <!-- Conversation menu (export, delete) -->
    <Teleport to="body">
      <div v-if="showMenu" class="fixed inset-0 z-[9998]" aria-hidden="true" @click="showMenu = false" />
      <Transition name="picker">
        <div
          v-if="showMenu"
          class="fixed z-[9999] path-glass-card header-overlay-panel p-2 animate-fade-up-fast shadow-2xl min-w-[160px]"
          :style="menuDropdownStyle"
          @click.stop
        >
          <p class="text-xs font-semibold uppercase tracking-wider mb-1.5 px-2 text-white/40">Export</p>
          <button
            class="w-full text-left px-3 py-2 rounded-lg text-xs text-white/60 hover:text-white hover:bg-white/10 transition-all"
            @click="handleExport('markdown')"
          >
            Markdown (.md)
          </button>
          <button
            class="w-full text-left px-3 py-2 rounded-lg text-xs text-white/60 hover:text-white hover:bg-white/10 transition-all"
            @click="handleExport('json')"
          >
            JSON
          </button>
          <button
            class="w-full text-left px-3 py-2 rounded-lg text-xs text-white/60 hover:text-white hover:bg-white/10 transition-all"
            @click="handleExport('text')"
          >
            Plain text (.txt)
          </button>
          <div class="border-t border-white/10 mt-1 pt-1">
            <button
              class="w-full text-left px-3 py-2 rounded-lg text-xs text-white/60 hover:text-white hover:bg-white/10 transition-all"
              @click="triggerImport"
            >
              Import conversations
            </button>
            <button
              class="w-full text-left px-3 py-2 rounded-lg text-xs text-red-400/80 hover:text-red-400 hover:bg-white/10 transition-all"
              @click="handleDelete"
            >
              Delete conversation
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <input
      ref="importInputRef"
      type="file"
      accept=".json"
      class="hidden"
      @change="handleImportFile"
    />

    <!-- Import status -->
    <div
      v-if="importStatus"
      class="absolute top-full left-3 right-3 mt-1 z-[100] glass px-3 py-2 rounded-lg text-xs animate-fade-up-fast"
      :class="importStatus.startsWith('Error') ? 'text-red-400' : 'text-accent'"
    >
      {{ importStatus }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useChatStore } from '@/stores/chat'
import { useAI } from '@/composables/useAI'
import { useContentPanel } from '@/composables/useContentPanel'
import { downloadConversation, type ExportFormat } from '@/utils/conversation-export'
import { parseImportFile } from '@/utils/conversation-import'
import { archyBridge } from '@/services/archyBridge'
import { useComparisonMode } from '@/composables/useComparisonMode'

defineProps<{
  title: string
  conversationId: string
  side: 'left' | 'right'
  showClose?: boolean
}>()

defineEmits<{
  switchSide: []
  newChat: []
  close: []
  openSettings: []
}>()

const { activeProvider, activeModel, availableProviders, setProvider, setModel } = useAI()
const comparison = useComparisonMode()

// Model capabilities map
const MODEL_CAPS: Record<string, { vision: boolean; tools: boolean; longContext: boolean }> = {
  'claude-haiku-4.5': { vision: true, tools: true, longContext: true },
  'claude-sonnet-4': { vision: true, tools: true, longContext: true },
  'claude-opus-4': { vision: true, tools: true, longContext: true },
  'meta-llama/llama-4-maverick': { vision: true, tools: true, longContext: true },
  'qwen/qwen3-235b-a22b-thinking-2507': { vision: false, tools: false, longContext: true },
  'mistralai/mistral-small-3.1-24b-instruct:free': { vision: true, tools: true, longContext: false },
  'google/gemma-3-27b-it:free': { vision: true, tools: false, longContext: false },
  'echo': { vision: false, tools: false, longContext: false },
}

function getModelCaps(modelId: string) {
  return MODEL_CAPS[modelId] ?? { vision: false, tools: false, longContext: false }
}
const chatStore = useChatStore()
const showModelPicker = ref(false)
const showMenu = ref(false)
const menuTriggerRef = ref<HTMLElement | null>(null)
const menuDropdownStyle = ref<Record<string, string>>({})
const headerRef = ref<HTMLElement | null>(null)
const modelPickerTriggerRef = ref<HTMLElement | null>(null)
const modelPickerDropdownStyle = ref<Record<string, string>>({})

function updateModelPickerPosition() {
  nextTick(() => {
    const el = modelPickerTriggerRef.value
    if (el) {
      const r = el.getBoundingClientRect()
      modelPickerDropdownStyle.value = {
        top: `${r.bottom + 4}px`,
        right: 'auto',
        left: `${r.left}px`,
        width: `${Math.max(r.width + 24, 220)}px`,
      }
    }
  })
}

function updateMenuPosition() {
  nextTick(() => {
    const el = menuTriggerRef.value
    if (el) {
      const r = el.getBoundingClientRect()
      menuDropdownStyle.value = {
        top: `${r.bottom + 4}px`,
        right: `${window.innerWidth - r.right}px`,
      }
    }
  })
}

watch(showModelPicker, (v) => { if (v) updateModelPickerPosition() })
watch(showMenu, (v) => { if (v) updateMenuPosition() })

const conversationList = computed(() => chatStore.conversationList)

const modelDisplayName = computed(() => {
  for (const p of availableProviders.value) {
    const m = p.models.find((mm) => mm.id === activeModel.value)
    if (m) return m.name
  }
  return activeModel.value
})

function selectModel(providerId: string, modelId: string) {
  if (setProvider(providerId as Parameters<typeof setProvider>[0])) setModel(modelId)
  showModelPicker.value = false
}

const { enterDesignSystemMode } = useContentPanel()

function openDesignSystem() {
  enterDesignSystemMode()
  showModelPicker.value = false
}

async function handleExport(format: ExportFormat) {
  const conv = chatStore.activeConversation
  if (!conv) return
  await downloadConversation(conv, format)
  showMenu.value = false
}

function handleDelete() {
  const id = chatStore.activeConversationId
  if (!id) return
  chatStore.deleteConversation(id)
  showMenu.value = false
}

const importInputRef = ref<HTMLInputElement | null>(null)
const importStatus = ref<string | null>(null)

function triggerImport() {
  showMenu.value = false
  importInputRef.value?.click()
}

async function handleImportFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return

  try {
    const text = await file.text()
    const result = parseImportFile(text)

    if (result.error || result.conversations.length === 0) {
      importStatus.value = `Error: ${result.error ?? 'No conversations found'}`
    } else {
      for (const conv of result.conversations) {
        chatStore.conversations.set(conv.id, conv)
      }
      const count = result.conversations.length
      importStatus.value = `Imported ${count} conversation${count > 1 ? 's' : ''} (${result.format})`
      // Switch to first imported conversation
      chatStore.setActiveConversation(result.conversations[0].id)
    }
  } catch {
    importStatus.value = 'Error: Failed to read file'
  }

  // Clear file input for re-use
  input.value = ''
  // Auto-hide status after 3 seconds
  setTimeout(() => { importStatus.value = null }, 3000)
}
</script>

<style scoped>
/* The two overlays this header opens — the model picker and the conversation
 * menu — float over live chat content, so the shared `path-glass-card`
 * translucency (rgba(0,0,0,0.65)) leaves their text hard to read against a
 * busy backdrop. Operator-reported 2026-08-04.
 *
 * Deliberately scoped to this component rather than changing `.path-glass-card`
 * itself: that class is also used by BookDetail, ArticleDetail, TVSeriesDetail,
 * WebsiteDetail, ContentPanel, ChatWindow and every ChatPage panel, none of
 * which have this problem. Only these two get the extra opacity.
 *
 * Scoped styles reach these nodes even though they are inside <Teleport to="body">
 * (Vue stamps the scope attribute on the element itself), and this rule is
 * unlayered while `.path-glass-card` lives in `@layer components` — unlayered
 * wins over layered regardless of specificity, so no !important is needed.
 */
.header-overlay-panel {
  background: rgba(0, 0, 0, 0.88);
}

.picker-enter-active {
  transition: all 0.2s cubic-bezier(0.22, 1, 0.36, 1);
}
.picker-leave-active {
  transition: all 0.15s ease-in;
}
.picker-enter-from,
.picker-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
