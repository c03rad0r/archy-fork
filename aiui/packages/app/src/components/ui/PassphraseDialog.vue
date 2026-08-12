<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-50 flex items-center justify-center p-4"
    >
      <!-- Backdrop -->
      <div
        class="absolute inset-0 bg-black/70 backdrop-blur-sm"
        @click.self="handleSkip"
      />

      <!-- Dialog — uses glass-card from design system -->
      <div class="glass-card relative w-full max-w-sm p-6 space-y-5 animate-scale-in">

        <!-- Header -->
        <div class="flex items-center gap-3">
          <div class="flex items-center justify-center w-10 h-10 rounded-xl bg-accent/15 border border-accent/30 shrink-0">
            <svg class="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z" />
            </svg>
          </div>
          <div>
            <h2 class="text-base font-semibold text-white/96 leading-tight">
              {{ isCreating ? 'Create Passphrase' : 'Unlock AIUI' }}
            </h2>
            <p class="text-xs text-white/40 mt-0.5">
              {{ isCreating ? 'Encrypts all your data locally' : 'Enter your passphrase to decrypt' }}
            </p>
          </div>
        </div>

        <!-- Inputs -->
        <div class="space-y-2.5">
          <input
            ref="inputRef"
            v-model="passphrase"
            type="password"
            :placeholder="isCreating ? 'Create a passphrase…' : 'Enter passphrase…'"
            class="w-full px-4 py-3 rounded-xl bg-white/5 border border-white/10 text-white/90 placeholder:text-white/25 outline-none focus:border-white/30 focus:bg-white/8 transition-colors"
            style="font-size: 16px"
            autocomplete="current-password"
            @keydown.enter="handleSubmit"
          />

          <input
            v-if="isCreating"
            v-model="confirmPassphrase"
            type="password"
            placeholder="Confirm passphrase…"
            class="w-full px-4 py-3 rounded-xl bg-white/5 border border-white/10 text-white/90 placeholder:text-white/25 outline-none focus:border-white/30 focus:bg-white/8 transition-colors"
            style="font-size: 16px"
            autocomplete="new-password"
            @keydown.enter="handleSubmit"
          />

          <p v-if="errorMessage || props.error" class="text-xs text-red-400 px-1">
            {{ errorMessage || props.error }}
          </p>
        </div>

        <!-- Actions -->
        <div class="flex gap-2">
          <button
            class="glass-button glass-button-sm flex-1 rounded-xl text-sm text-white/50 hover:text-white/80 transition-colors"
            @click="handleSkip"
          >
            Skip
          </button>
          <button
            class="glass-button flex-1 h-11 rounded-xl text-sm font-medium text-white transition-opacity"
            :disabled="!canSubmit"
            :class="{ 'opacity-40 cursor-not-allowed': !canSubmit }"
            @click="handleSubmit"
          >
            {{ isCreating ? 'Create' : 'Unlock' }}
          </button>
        </div>

        <p class="text-xs text-white/25 text-center">
          Data never leaves your device · AES-256-GCM
        </p>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'

const props = defineProps<{
  visible: boolean
  isCreating: boolean
  error?: string
}>()

const emit = defineEmits<{
  submit: [passphrase: string]
  skip: []
}>()

const passphrase = ref('')
const confirmPassphrase = ref('')
const errorMessage = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

const canSubmit = computed(() => {
  if (!passphrase.value.trim()) return false
  if (props.isCreating && passphrase.value !== confirmPassphrase.value) return false
  return true
})

function handleSubmit() {
  if (!canSubmit.value) {
    if (props.isCreating && passphrase.value !== confirmPassphrase.value) {
      errorMessage.value = 'Passphrases do not match'
    }
    return
  }
  errorMessage.value = ''
  emit('submit', passphrase.value)
}

function handleSkip() {
  emit('skip')
}

watch(() => props.visible, (v) => {
  if (v) nextTick(() => inputRef.value?.focus())
}, { immediate: true })
</script>
