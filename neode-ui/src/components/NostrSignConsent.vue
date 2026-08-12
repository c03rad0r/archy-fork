<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
        @click="deny"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div
          ref="modalRef"
          @click.stop
          class="glass-card p-6 max-w-md w-full relative z-10"
        >
          <div class="flex items-start justify-between gap-4 mb-4">
            <h3 class="text-xl font-semibold text-white">Nostr Signing Request</h3>
            <button
              @click="deny"
              class="p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors"
              aria-label="Close"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="space-y-3 mb-6">
            <div class="bg-black/20 rounded-xl border border-white/10 p-3">
              <p class="text-white/50 text-xs uppercase tracking-wider mb-1">App</p>
              <p class="text-white text-sm font-medium">{{ appName }}</p>
            </div>

            <div class="bg-black/20 rounded-xl border border-white/10 p-3">
              <p class="text-white/50 text-xs uppercase tracking-wider mb-1">Method</p>
              <p class="text-white text-sm font-medium">{{ method }}</p>
            </div>

            <div v-if="contentPreview" class="bg-black/20 rounded-xl border border-white/10 p-3">
              <p class="text-white/50 text-xs uppercase tracking-wider mb-1">Content</p>
              <p class="text-white/80 text-sm font-mono break-all">{{ contentPreview }}</p>
            </div>

            <div v-if="eventKind !== undefined" class="bg-black/20 rounded-xl border border-white/10 p-3">
              <p class="text-white/50 text-xs uppercase tracking-wider mb-1">Event Kind</p>
              <p class="text-white text-sm font-medium">{{ eventKind }} <span class="text-white/50">({{ eventKindLabel }})</span></p>
            </div>
          </div>

          <label class="flex items-center gap-2 mb-4 cursor-pointer">
            <input
              v-model="rememberChoice"
              type="checkbox"
              class="w-4 h-4 rounded border-white/30 bg-white/10 text-orange-400 focus:ring-orange-400/50"
            />
            <span class="text-white/70 text-sm">Remember for this app</span>
          </label>

          <div class="flex gap-3">
            <button @click="deny" class="glass-button flex-1 py-2.5 rounded-lg text-sm font-medium">
              Deny
            </button>
            <button @click="approve" class="glass-button flex-1 py-2.5 rounded-lg text-sm font-medium text-orange-400 border-orange-400/30">
              Approve
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'

const EVENT_KIND_LABELS: Record<number, string> = {
  0: 'Metadata',
  1: 'Short Text Note',
  2: 'Recommend Relay',
  3: 'Contacts',
  4: 'Encrypted DM',
  5: 'Event Deletion',
  6: 'Repost',
  7: 'Reaction',
  9734: 'Zap Request',
  9735: 'Zap Receipt',
  10002: 'Relay List',
  30023: 'Long-form Content',
}

const props = defineProps<{
  show: boolean
  appName: string
  method: string
  eventKind?: number
  content?: string
}>()

const emit = defineEmits<{
  approve: [remember: boolean]
  deny: []
}>()

const modalRef = ref<HTMLElement | null>(null)
const rememberChoice = ref(false)

useModalKeyboard(modalRef, computed(() => props.show), () => emit('deny'))

const contentPreview = computed(() => {
  if (!props.content) return ''
  return props.content.length > 200 ? props.content.slice(0, 200) + '...' : props.content
})

const eventKindLabel = computed(() => {
  if (props.eventKind === undefined) return ''
  return EVENT_KIND_LABELS[props.eventKind] ?? 'Unknown'
})

function approve() {
  emit('approve', rememberChoice.value)
}

function deny() {
  emit('deny')
}
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.3s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-active .glass-card,
.modal-leave-active .glass-card {
  transition: transform 0.3s ease;
}

.modal-enter-from .glass-card {
  transform: scale(0.95);
}

.modal-leave-to .glass-card {
  transform: scale(0.95);
}
</style>
