<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="visible" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="handleClose">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div class="glass-card p-6 max-w-md w-full relative z-10">
          <h3 class="text-lg font-semibold text-white mb-2">Confirm Trusted Access</h3>
          <p class="text-sm text-white/60 mb-4">{{ context }}</p>
          <input
            ref="passwordInput"
            v-model="password"
            type="password"
            autocomplete="current-password"
            placeholder="Enter your node password to confirm"
            class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-orange-500/50 mb-4"
            @keyup.enter="submit"
          />
          <p v-if="error" class="text-red-400 text-xs mb-3">{{ error }}</p>
          <div class="flex gap-3">
            <button @click="handleClose" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
            <button
              @click="submit"
              :disabled="busy || !password"
              class="flex-1 glass-button px-4 py-2 rounded-lg text-sm font-medium bg-orange-500/20 border-orange-500/30 disabled:opacity-50"
            >
              {{ busy ? 'Verifying…' : 'Grant Trusted' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, nextTick, watch } from 'vue'

const props = defineProps<{
  visible: boolean
  /** What is about to be granted, in the operator's terms. */
  context: string
  busy: boolean
  error: string
}>()

const emit = defineEmits<{
  close: []
  confirm: [password: string]
}>()

const password = ref('')
const passwordInput = ref<HTMLInputElement | null>(null)

function submit() {
  if (!password.value || props.busy) return
  emit('confirm', password.value)
}

function handleClose() {
  password.value = ''
  emit('close')
}

// Never leave the password sitting in memory once the modal is dismissed,
// and put the cursor where the operator has to type anyway.
watch(() => props.visible, async (val) => {
  if (!val) {
    password.value = ''
    return
  }
  await nextTick()
  passwordInput.value?.focus()
})
</script>
