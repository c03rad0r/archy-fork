<template>
  <slot v-if="!error" />
  <div
    v-else
    class="flex items-center justify-center p-4"
    role="alert"
  >
    <div
      class="rounded-xl p-4 max-w-sm w-full space-y-3"
      :class="isDark
        ? 'bg-red-500/10 border border-red-500/20'
        : 'bg-red-50 border border-red-200'"
    >
      <div class="flex items-start gap-3">
        <svg
          class="w-5 h-5 text-red-500 shrink-0 mt-0.5"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z"
          />
        </svg>
        <div class="min-w-0">
          <h3
            class="text-sm font-semibold"
            :class="isDark ? 'text-red-400' : 'text-red-900'"
          >
            {{ title }}
          </h3>
          <p
            v-if="errorMessage"
            class="text-xs mt-1 break-words"
            :class="isDark ? 'text-red-300/70' : 'text-red-700/70'"
          >
            {{ errorMessage }}
          </p>
        </div>
      </div>
      <button
        class="text-xs px-3 py-1.5 rounded-lg transition-colors"
        :class="isDark
          ? 'text-red-300 hover:bg-red-500/20'
          : 'text-red-600 hover:bg-red-100'"
        @click="reset"
      >
        Try again
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue'
import { useTheme } from '@/composables/useTheme'

withDefaults(
  defineProps<{
    title?: string
  }>(),
  {
    title: 'Something went wrong',
  }
)

const { isDark } = useTheme()

const error = ref<Error | null>(null)

const errorMessage = ref('')

onErrorCaptured((err: Error) => {
  error.value = err
  errorMessage.value = err.message || 'An unexpected error occurred'
  console.error('[ErrorBoundary]', err)
  return false
})

function reset() {
  error.value = null
  errorMessage.value = ''
}
</script>
