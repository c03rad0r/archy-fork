<template>
  <Teleport to="body">
    <div class="fixed right-4 z-[9999] flex flex-col gap-2 pointer-events-none max-w-sm w-full" style="top: calc(var(--safe-area-top, env(safe-area-inset-top, 0px)) + 16px);">
      <TransitionGroup name="toast-stack">
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="toast-stack-item pointer-events-auto flex items-center gap-3 px-4 py-3 rounded-xl border cursor-pointer"
          :class="variantClass(toast.variant)"
          @click="dismiss(toast.id)"
        >
          <div class="w-5 h-5 shrink-0 flex items-center justify-center">
            <!-- Success -->
            <svg v-if="toast.variant === 'success'" class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
            </svg>
            <!-- Error -->
            <svg v-else-if="toast.variant === 'error'" class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
            <!-- Info -->
            <svg v-else class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </div>
          <div class="flex-1 min-w-0">
            <span class="text-sm text-white/90">{{ toast.message }}</span>
            <button
              v-if="toast.action"
              @click.stop="runAction(toast)"
              class="block mt-1 text-sm font-semibold text-orange-400 hover:text-orange-300 transition-colors"
            >{{ toast.action.label }} →</button>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { useToast } from '@/composables/useToast'
import type { ToastItem, ToastVariant } from '@/composables/useToast'

const { toasts, dismiss } = useToast()

function runAction(toast: ToastItem | Readonly<ToastItem>) {
  toast.action?.onClick()
  dismiss(toast.id)
}

function variantClass(variant: ToastVariant): string {
  switch (variant) {
    case 'success': return 'bg-black/70 border-green-500/30 backdrop-blur-md'
    case 'error': return 'bg-black/70 border-red-500/30 backdrop-blur-md'
    default: return 'bg-black/70 border-blue-500/30 backdrop-blur-md'
  }
}
</script>

<style scoped>
.toast-stack-enter-active {
  transition: opacity 0.3s ease, transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.toast-stack-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.toast-stack-enter-from {
  opacity: 0;
  transform: translateX(100%);
}
.toast-stack-leave-to {
  opacity: 0;
  transform: translateX(50%);
}
.toast-stack-move {
  transition: transform 0.3s ease;
}
</style>
