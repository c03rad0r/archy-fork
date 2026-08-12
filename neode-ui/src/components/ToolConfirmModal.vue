<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        data-testid="tool-confirm-overlay"
        class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
        @click="dismiss"
      >
        <div
          data-testid="tool-confirm-backdrop"
          class="absolute inset-0 bg-black/60 backdrop-blur-sm"
        ></div>
        <div ref="modalRef" @click.stop class="glass-card p-6 max-w-md w-full relative z-10">
          <div class="flex items-start justify-between gap-4 mb-4">
            <h3 class="text-xl font-semibold text-white">Approve this action?</h3>
            <button
              @click="dismiss"
              class="p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors"
              aria-label="Close"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </button>
          </div>

          <!--
            The description is node-authored: fetched by the host page over
            its own authenticated RPC session (assistant.pending), never
            received from the AIUI iframe and never model text. Plain
            interpolation only — peer-influenced argument values must render
            as inert text, so the raw-HTML directive is banned in this file.
            There is deliberately NO code path in this component that reads
            from the frame's message channel.
          -->
          <div class="bg-black/20 rounded-xl border border-white/10 p-4 mb-4">
            <p class="text-white text-sm leading-relaxed whitespace-pre-wrap">{{ description }}</p>
          </div>

          <p class="text-white/40 text-xs mb-4">
            The assistant asked to do this. Nothing happens unless you approve — closing this
            window decides nothing, and the request expires on its own.
          </p>

          <div class="flex gap-3">
            <button
              data-testid="tool-confirm-deny"
              @click="deny"
              class="glass-button flex-1 py-2.5 rounded-lg text-sm font-medium"
            >
              Deny
            </button>
            <button
              data-testid="tool-confirm-approve"
              @click="approve"
              class="glass-button flex-1 py-2.5 rounded-lg text-sm font-medium text-orange-400 border-orange-400/30"
            >
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

const props = defineProps<{
  show: boolean
  description: string
}>()

const emit = defineEmits<{
  approve: []
  deny: []
  /** Closed without a decision: nothing is sent anywhere — the node's own
   * timeout declines the pending action. Never treated as an approval. */
  dismiss: []
}>()

const modalRef = ref<HTMLElement | null>(null)

useModalKeyboard(
  modalRef,
  computed(() => props.show),
  () => emit('dismiss'),
)

function approve() {
  emit('approve')
}

function deny() {
  emit('deny')
}

function dismiss() {
  emit('dismiss')
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
