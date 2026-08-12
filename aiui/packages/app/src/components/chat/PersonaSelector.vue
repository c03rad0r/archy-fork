<template>
  <div v-if="personaStore.personas.length > 0" class="px-3 md:px-4">
    <div class="flex items-center gap-1.5 flex-wrap">
      <button
        v-for="p in personaStore.sortedPersonas"
        :key="p.id"
        class="px-2.5 py-1 rounded-full text-xs font-medium transition-all duration-200 border"
        :class="isActive(p.id)
          ? 'bg-accent/20 text-accent border-accent/30'
          : 'bg-white/5 text-white/50 border-white/10 hover:text-white/70 hover:bg-white/10'"
        @click="selectPersona(p.id)"
      >
        <span
          v-if="p.accentColor"
          class="inline-block w-2 h-2 rounded-full mr-1"
          :style="{ backgroundColor: p.accentColor }"
        />
        {{ p.name }}
      </button>
      <button
        class="px-2 py-1 rounded-full text-xs text-white/30 hover:text-white/60 border border-transparent hover:border-white/10 transition-all duration-200"
        @click="showEditor = true"
      >
        + New
      </button>
      <button
        v-if="activePersonaId"
        class="px-2 py-1 rounded-full text-xs text-white/30 hover:text-white/60 transition-all"
        title="Clear persona"
        @click="clearPersona"
      >
        ✕
      </button>
    </div>

    <!-- Persona editor modal -->
    <Teleport to="body">
      <div
        v-if="showEditor"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="closeEditor"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" />
        <div class="relative glass-card w-full max-w-md p-5 space-y-4 animate-scale-in">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-semibold text-white/90">
              {{ editingPersona ? 'Edit Persona' : 'New Persona' }}
            </h3>
            <button
              class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/40 hover:text-white/70 hover:bg-white/10 transition-all"
              @click="closeEditor"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="space-y-3">
            <div>
              <label class="block text-xs text-white/40 mb-1">Name</label>
              <input
                v-model="formName"
                type="text"
                class="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-base text-white/90 focus:outline-none focus:border-accent/50"
                placeholder="e.g. Film Critic"
              />
            </div>

            <div>
              <label class="block text-xs text-white/40 mb-1">System Prompt</label>
              <textarea
                v-model="formPrompt"
                class="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-base text-white/90 resize-none focus:outline-none focus:border-accent/50"
                rows="5"
                placeholder="You are a film critic who..."
              />
            </div>

            <div class="flex gap-3">
              <div class="flex-1">
                <label class="block text-xs text-white/40 mb-1">Model Preference</label>
                <select
                  v-model="formModel"
                  class="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-base text-white/90 focus:outline-none focus:border-accent/50"
                >
                  <option value="">Default</option>
                  <option value="claude-haiku-4.5">Claude 4.5 Haiku</option>
                  <option value="claude-sonnet-4">Claude Sonnet 4</option>
                  <option value="claude-opus-4">Claude Opus 4</option>
                </select>
              </div>
              <div class="w-20">
                <label class="block text-xs text-white/40 mb-1">Colour</label>
                <input
                  v-model="formColor"
                  type="color"
                  class="w-full h-9 rounded-lg bg-white/5 border border-white/10 cursor-pointer"
                />
              </div>
            </div>

            <label class="flex items-center gap-2 text-xs text-white/60">
              <input
                v-model="formDefault"
                type="checkbox"
                class="accent-[#F7931A]"
              />
              Set as default for new conversations
            </label>
          </div>

          <div class="flex items-center gap-2 pt-1">
            <button
              v-if="editingPersona"
              class="px-3 py-1.5 rounded-lg text-xs text-red-400/80 hover:text-red-400 hover:bg-red-400/10 transition-all"
              @click="handleDelete"
            >
              Delete
            </button>
            <div class="flex-1" />
            <button
              class="px-3 py-1.5 rounded-lg text-xs text-white/50 hover:text-white/70 hover:bg-white/10 transition-all"
              @click="closeEditor"
            >
              Cancel
            </button>
            <button
              class="px-4 py-1.5 rounded-lg text-xs bg-accent/20 text-accent hover:bg-accent/30 transition-all"
              :disabled="!formName.trim()"
              @click="handleSave"
            >
              {{ editingPersona ? 'Save' : 'Create' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { usePersonaStore, type Persona } from '@/stores/personas'
import { useChatStore } from '@/stores/chat'

const personaStore = usePersonaStore()
const chatStore = useChatStore()

const showEditor = ref(false)
const editingPersona = ref<Persona | null>(null)

// Form state
const formName = ref('')
const formPrompt = ref('')
const formModel = ref('')
const formColor = ref('#F7931A')
const formDefault = ref(false)

const activePersonaId = computed(() => chatStore.activeConversation?.personaId ?? null)

function isActive(id: string): boolean {
  return activePersonaId.value === id
}

function selectPersona(id: string) {
  const conv = chatStore.activeConversation
  if (!conv) return

  if (conv.personaId === id) {
    // Double-click to edit
    openEditorFor(personaStore.getPersona(id))
    return
  }

  conv.personaId = id
  conv.updatedAt = Date.now()
}

function clearPersona() {
  const conv = chatStore.activeConversation
  if (!conv) return
  conv.personaId = undefined
  conv.updatedAt = Date.now()
}

function openEditorFor(persona?: Persona) {
  if (persona) {
    editingPersona.value = persona
    formName.value = persona.name
    formPrompt.value = persona.systemPrompt
    formModel.value = persona.modelPreference ?? ''
    formColor.value = persona.accentColor ?? '#F7931A'
    formDefault.value = persona.isDefault ?? false
  } else {
    editingPersona.value = null
    formName.value = ''
    formPrompt.value = ''
    formModel.value = ''
    formColor.value = '#F7931A'
    formDefault.value = false
  }
  showEditor.value = true
}

function closeEditor() {
  showEditor.value = false
  editingPersona.value = null
}

function handleSave() {
  if (!formName.value.trim()) return

  const data = {
    name: formName.value.trim(),
    systemPrompt: formPrompt.value.trim(),
    modelPreference: formModel.value || undefined,
    accentColor: formColor.value,
    isDefault: formDefault.value,
  }

  if (editingPersona.value) {
    personaStore.updatePersona(editingPersona.value.id, data)
  } else {
    const created = personaStore.addPersona(data)
    // Auto-select the new persona
    const conv = chatStore.activeConversation
    if (conv) {
      conv.personaId = created.id
      conv.updatedAt = Date.now()
    }
  }

  closeEditor()
}

function handleDelete() {
  if (!editingPersona.value) return
  const id = editingPersona.value.id
  // Clear from any active conversation
  const conv = chatStore.activeConversation
  if (conv?.personaId === id) {
    conv.personaId = undefined
  }
  personaStore.deletePersona(id)
  closeEditor()
}
</script>
