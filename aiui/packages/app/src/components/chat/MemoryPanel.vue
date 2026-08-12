<template>
  <div class="px-3 md:px-4 pb-1">
    <!-- Collapsed toggle -->
    <button
      class="flex items-center gap-1.5 text-xs text-white/40 hover:text-white/60 transition-colors"
      @click="isExpanded = !isExpanded"
    >
      <svg
        class="w-3 h-3 transition-transform duration-200"
        :class="isExpanded ? 'rotate-90' : ''"
        fill="currentColor"
        viewBox="0 0 20 20"
      >
        <path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd" />
      </svg>
      Memory ({{ memoryStore.items.length }}/20)
    </button>

    <!-- Expanded panel -->
    <div v-if="isExpanded" class="mt-2 space-y-1.5 animate-fade-up-fast">
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

      <!-- Add new -->
      <div v-if="!memoryStore.isFull" class="flex gap-1.5">
        <input
          v-model="newText"
          type="text"
          class="flex-1 bg-white/5 border border-white/10 rounded-lg px-2.5 py-1.5 text-base text-white/80 outline-none focus:border-accent/40 placeholder:text-white/25"
          placeholder="Add a memory..."
          @keydown.enter="addMemory"
        />
        <button
          class="px-2.5 py-1.5 rounded-lg text-xs bg-accent/15 text-accent/80 hover:bg-accent/25 transition-all"
          :disabled="!newText.trim()"
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
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useMemoryStore, type MemoryItem } from '@/stores/memory'

const memoryStore = useMemoryStore()

const isExpanded = ref(false)
const newText = ref('')
const editingId = ref<string | null>(null)
const editText = ref('')

function addMemory() {
  if (!newText.value.trim()) return
  memoryStore.addItem(newText.value)
  newText.value = ''
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
</script>
