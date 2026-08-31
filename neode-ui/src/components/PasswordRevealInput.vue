<template>
  <div class="relative">
    <input
      :type="revealed ? 'text' : 'password'"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :autocomplete="autocomplete"
      class="w-full px-3 py-2 pr-10 bg-white/5 border border-white/10 rounded-lg text-white text-sm placeholder-white/30 focus:outline-none focus:border-white/30 disabled:opacity-50 disabled:cursor-not-allowed"
      @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      @keyup.enter="$emit('enter')"
    >
    <button
      type="button"
      class="absolute inset-y-0 right-0 px-3 text-white/40 hover:text-white/80 transition-colors"
      :aria-label="revealed ? 'Hide password' : 'Show password'"
      :title="revealed ? 'Hide password' : 'Show password'"
      @click="revealed = !revealed"
    >
      <!-- eye -->
      <svg v-if="!revealed" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
      </svg>
      <!-- eye-off -->
      <svg v-else class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

/**
 * Password input with a reveal toggle (#145). Introduced for the WiFi SSID
 * password — a fresh-install user typing a long wifi key into a TV from
 * across the room needs to see what they typed — and written reusable so
 * other password fields can adopt it without re-deriving the eye icon.
 */
defineProps<{
  modelValue: string
  placeholder?: string
  disabled?: boolean
  autocomplete?: string
}>()

defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'enter'): void
}>()

const revealed = ref(false)
</script>
