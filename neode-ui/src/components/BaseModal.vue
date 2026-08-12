<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        class="fixed inset-0 flex items-center justify-center p-4"
        :class="zClass"
        @click.self="close"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-md"></div>
        <!-- Column layout (2026-07-22 modal contract): title row and the
             optional #header slot (tabs) stay pinned at the top, the #footer
             slot (action buttons) stays pinned at the bottom, and ONLY the
             default slot scrolls. Callers that previously made the whole
             card scroll via contentClass keep working — the inner region
             simply never lets the card overflow. -->
        <div
          ref="modalRef"
          class="glass-card p-6 w-full relative z-10 flex flex-col"
          :class="[maxWidth, contentClass, defaultMaxH]"
          role="dialog"
          aria-modal="true"
          @click.stop
        >
          <div class="flex items-start justify-between gap-4 mb-4 shrink-0">
            <h3 class="text-xl font-semibold text-white">{{ title }}</h3>
            <button
              @click="close"
              class="p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors"
              aria-label="Close"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
          <div v-if="$slots.header" class="shrink-0">
            <slot name="header" />
          </div>
          <div class="flex-1 min-h-0 overflow-y-auto">
            <slot />
          </div>
          <div v-if="$slots.footer" class="shrink-0 pt-4">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import { useBodyScrollLock } from '@/composables/useBodyScrollLock'
import { useModalHistory } from '@/composables/useModalHistory'

const props = withDefaults(defineProps<{
  show: boolean
  title: string
  maxWidth?: string
  zIndex?: string
  contentClass?: string
}>(), {
  maxWidth: 'max-w-md',
  zIndex: 'z-[3000]',
  contentClass: '',
})

const emit = defineEmits<{
  close: []
}>()

const modalRef = ref<HTMLElement | null>(null)

// A modal must not outlive the screen that raised it. Tab views are
// KeepAlive'd, so navigating away deactivates the owner rather than
// unmounting it, and a Teleported modal keeps floating over the destination
// (seen with the Lightning "Open a channel" / "Setup Guide" actions, which
// route away from inside the wallet's own modal). Closing on any route change
// is the general fix — every modal here is a transient dialog.
//
// `useRoute()` returns undefined when no router is installed (component
// tests mount BaseModal bare), so the getter is optional-chained.
const route = useRoute()
watch(
  () => route?.fullPath,
  (to, from) => {
    if (to !== from && props.show) emit('close')
  },
)

const zClass = computed(() => props.zIndex)
// The pinned-footer layout needs a height bound or tall content pushes the
// footer off-screen anyway. Callers that set their own max-h (e.g. the
// Transactions modal's visual-viewport calc on mobile) keep authority —
// adding a second max-h class would make the CSS winner order-dependent.
const defaultMaxH = computed(() =>
  props.contentClass.includes('max-h-') ? '' : 'max-h-[90vh]'
)

function close() {
  emit('close')
}

useModalKeyboard(modalRef, computed(() => props.show), close)
useBodyScrollLock(computed(() => props.show))
// Browser/mouse/gesture Back closes the modal instead of navigating the
// router out from under it — the native-app behaviour kiosk and mobile
// browsers expect (the companion webview already provides it natively).
useModalHistory(computed(() => props.show), close)
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
.modal-enter-active .glass-card,
.modal-leave-active .glass-card {
  transition: transform 0.2s ease;
}
.modal-enter-from .glass-card {
  transform: scale(0.95);
}
.modal-leave-to .glass-card {
  transform: scale(0.95);
}
</style>
