<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="visible" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="$emit('close')">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div class="glass-card p-6 max-w-md w-full relative z-10">
          <h3 class="text-lg font-semibold text-white mb-2">Rotate Node DID</h3>
          <p class="text-sm text-white/60 mb-4">This generates a new identity keypair and notifies all federated peers. Your old DID will no longer be valid.</p>
          <input v-model="password" type="password" placeholder="Enter your password to confirm" class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-orange-500/50 mb-4" />
          <p v-if="error" class="text-red-400 text-xs mb-3">{{ error }}</p>
          <p v-if="success" class="text-green-400 text-xs mb-3">{{ success }}</p>
          <div class="flex gap-3">
            <button @click="handleClose" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
            <button @click="$emit('rotate', password)" :disabled="rotating || !password" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm font-medium bg-orange-500/20 border-orange-500/30 disabled:opacity-50">
              {{ rotating ? 'Rotating...' : 'Rotate & Notify Peers' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{
  visible: boolean
  rotating: boolean
  error: string
  success: string
}>()

const emit = defineEmits<{
  close: []
  rotate: [password: string]
}>()

const password = ref('')

function handleClose() {
  password.value = ''
  emit('close')
}

// Reset password when modal opens/closes
watch(() => props.visible, (val) => {
  if (!val) password.value = ''
})
</script>
