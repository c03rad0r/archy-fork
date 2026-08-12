<template>
  <Teleport to="body">
    <Transition name="settings-modal">
      <div
        v-if="open"
        ref="dialogRef"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @keydown.escape="$emit('update:open', false)"
        @keydown.tab="trapFocus"
      >
        <div
          class="absolute inset-0 bg-black/70 backdrop-blur-sm"
          @click.self="$emit('update:open', false)"
        />

        <div class="glass-card relative w-full max-w-md p-5 space-y-5 animate-scale-in max-h-[85vh] overflow-y-auto scrollbar-hide">
          <!-- Header -->
          <div class="flex items-center justify-between">
            <h2 class="text-sm font-semibold text-white/96">Settings</h2>
            <button
              ref="closeButtonRef"
              class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/40 hover:text-white/70 hover:bg-white/10 transition-all"
              aria-label="Close settings"
              @click="$emit('update:open', false)"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <!-- Memory Section -->
          <div class="space-y-2">
            <h3 class="text-xs font-semibold uppercase tracking-wider text-white/50">
              Memory ({{ memoryStore.items.length }}/20)
            </h3>

            <div class="space-y-1.5">
              <div
                v-for="item in memoryStore.items"
                :key="item.id"
                class="group flex items-start gap-2 rounded-lg bg-white/5 border border-white/5 px-2.5 py-1.5"
              >
                <div v-if="editingId === item.id" class="flex-1 flex gap-1.5">
                  <input
                    v-model="editText"
                    type="text"
                    class="flex-1 bg-transparent text-base text-white/80 outline-none"
                    @keydown.enter="saveEdit(item.id)"
                    @keydown.escape="cancelEdit"
                  />
                  <button
                    class="text-xs text-accent/70 hover:text-accent"
                    @click="saveEdit(item.id)"
                  >
                    Save
                  </button>
                </div>
                <template v-else>
                  <p class="flex-1 text-xs text-white/60 leading-relaxed">{{ item.text }}</p>
                  <div class="shrink-0 flex gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded text-white/30 hover:text-white/60 hover:bg-white/10 transition-all"
                      title="Edit"
                      @click="startEdit(item)"
                    >
                      <svg class="w-3 h-3" viewBox="0 0 20 20" fill="currentColor"><path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z" /></svg>
                    </button>
                    <button
                      class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded text-white/30 hover:text-red-400/80 hover:bg-white/10 transition-all"
                      title="Delete"
                      @click="memoryStore.deleteItem(item.id)"
                    >
                      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                </template>
              </div>

              <div v-if="!memoryStore.isFull" class="flex gap-1.5">
                <input
                  v-model="newMemoryText"
                  type="text"
                  class="flex-1 bg-white/5 border border-white/10 rounded-lg px-2.5 py-1.5 text-base text-white/80 outline-none focus:border-accent/40 placeholder:text-white/25"
                  placeholder="Add a memory..."
                  @keydown.enter="addMemory"
                />
                <button
                  class="px-2.5 py-1.5 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-all"
                  :disabled="!newMemoryText.trim()"
                  @click="addMemory"
                >
                  Add
                </button>
              </div>
              <p v-else class="text-xs text-white/25">
                Maximum 20 memories reached
              </p>
            </div>
          </div>

          <!-- Divider -->
          <div class="border-t border-white/5" />

          <!-- Advanced Section -->
          <div class="space-y-3">
            <h3 class="text-xs font-semibold uppercase tracking-wider text-white/50">
              Advanced
            </h3>

            <template v-if="conv">
              <div class="space-y-1">
                <div class="flex items-center justify-between">
                  <label class="text-xs text-white/50">Temperature</label>
                  <span class="text-xs text-white/50 tabular-nums">{{ temperature.toFixed(2) }}</span>
                </div>
                <input
                  v-model.number="temperature"
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
                  @input="persistParams"
                />
              </div>

              <div class="space-y-1">
                <div class="flex items-center justify-between">
                  <label class="text-xs text-white/50">Max Tokens</label>
                  <span class="text-xs text-white/50 tabular-nums">{{ maxTokens }}</span>
                </div>
                <input
                  v-model.number="maxTokens"
                  type="range"
                  min="256"
                  max="8192"
                  step="256"
                  class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
                  @input="persistParams"
                />
              </div>

              <div class="space-y-1">
                <div class="flex items-center justify-between">
                  <label class="text-xs text-white/50">Top P</label>
                  <span class="text-xs text-white/50 tabular-nums">{{ topP.toFixed(2) }}</span>
                </div>
                <input
                  v-model.number="topP"
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
                  @input="persistParams"
                />
              </div>

              <div class="space-y-1">
                <label class="text-xs text-white/50">Stop Sequences</label>
                <div v-if="stopSequences.length > 0" class="flex gap-1 flex-wrap mb-1">
                  <span
                    v-for="(seq, i) in stopSequences"
                    :key="i"
                    class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-xs text-white/50"
                  >
                    {{ seq }}
                    <button class="text-white/30 hover:text-white/60" @click="removeStopSequence(i)">&times;</button>
                  </span>
                </div>
                <input
                  v-model="newStopSeq"
                  type="text"
                  class="w-full bg-white/5 border border-white/10 rounded-lg px-2.5 py-1.5 text-base text-white/80 outline-none focus:border-accent/40 placeholder:text-white/25"
                  placeholder="Add stop sequence (Enter to add)"
                  @keydown.enter="addStopSequence"
                />
              </div>

              <button
                class="text-xs text-white/30 hover:text-white/50 transition-colors"
                @click="resetDefaults"
              >
                Reset to defaults
              </button>
            </template>
            <p v-else class="text-xs text-white/30">Start a conversation to configure parameters</p>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useMemoryStore, type MemoryItem } from '@/stores/memory'
import { useChatStore } from '@/stores/chat'

const props = defineProps<{
  open: boolean
}>()

defineEmits<{
  'update:open': [value: boolean]
}>()

const dialogRef = ref<HTMLElement | null>(null)
const closeButtonRef = ref<HTMLElement | null>(null)

function trapFocus(e: KeyboardEvent) {
  const dialog = dialogRef.value
  if (!dialog) return
  const focusable = dialog.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
  )
  if (focusable.length === 0) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault()
    first.focus()
  }
}

watch(() => props.open, async (isOpen) => {
  if (isOpen) {
    await nextTick()
    closeButtonRef.value?.focus()
  }
})

// --- Memory ---
const memoryStore = useMemoryStore()
const newMemoryText = ref('')
const editingId = ref<string | null>(null)
const editText = ref('')

function addMemory() {
  if (!newMemoryText.value.trim()) return
  memoryStore.addItem(newMemoryText.value)
  newMemoryText.value = ''
}

function startEdit(item: MemoryItem) {
  editingId.value = item.id
  editText.value = item.text
}

function saveEdit(id: string) {
  if (editText.value.trim()) {
    memoryStore.updateItem(id, editText.value)
  }
  editingId.value = null
  editText.value = ''
}

function cancelEdit() {
  editingId.value = null
  editText.value = ''
}

// --- Advanced ---
const chatStore = useChatStore()
const conv = computed(() => chatStore.activeConversation)
const newStopSeq = ref('')

const temperature = ref(1.0)
const maxTokens = ref(4096)
const topP = ref(1.0)
const stopSequences = ref<string[]>([])

watch(
  () => chatStore.activeConversationId,
  () => loadFromConv(),
  { immediate: true }
)

function loadFromConv() {
  const c = conv.value
  temperature.value = c?.temperature ?? 1.0
  maxTokens.value = c?.maxTokens ?? 4096
  topP.value = c?.topP ?? 1.0
  stopSequences.value = c?.stopSequences ? [...c.stopSequences] : []
}

function persistParams() {
  const c = conv.value
  if (!c) return
  c.temperature = temperature.value
  c.maxTokens = maxTokens.value
  c.topP = topP.value
  c.updatedAt = Date.now()
}

function addStopSequence() {
  const seq = newStopSeq.value.trim()
  if (!seq) return
  stopSequences.value.push(seq)
  newStopSeq.value = ''
  const c = conv.value
  if (c) {
    c.stopSequences = [...stopSequences.value]
    c.updatedAt = Date.now()
  }
}

function removeStopSequence(index: number) {
  stopSequences.value.splice(index, 1)
  const c = conv.value
  if (c) {
    c.stopSequences = [...stopSequences.value]
    c.updatedAt = Date.now()
  }
}

function resetDefaults() {
  temperature.value = 1.0
  maxTokens.value = 4096
  topP.value = 1.0
  stopSequences.value = []
  newStopSeq.value = ''
  const c = conv.value
  if (c) {
    c.temperature = undefined
    c.maxTokens = undefined
    c.topP = undefined
    c.stopSequences = undefined
    c.updatedAt = Date.now()
  }
}
</script>

<style scoped>
.settings-modal-enter-active {
  transition: opacity 0.2s ease-out;
}
.settings-modal-enter-active .glass-card {
  transition: all 0.25s cubic-bezier(0.22, 1, 0.36, 1);
}
.settings-modal-leave-active {
  transition: opacity 0.15s ease-in;
}
.settings-modal-enter-from,
.settings-modal-leave-to {
  opacity: 0;
}
</style>
