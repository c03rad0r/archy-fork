<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="visible" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="$emit('close')">
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div class="glass-card p-6 max-w-md w-full relative z-10">
          <div class="flex items-center justify-between mb-6">
            <h2 class="text-xl font-semibold text-white">Join Federation</h2>
            <button @click="$emit('close')" class="text-white/40 hover:text-white/70 transition-colors">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <p class="text-sm text-white/60 mb-4">Paste the invite code from the node you want to federate with.</p>

          <textarea
            v-model="joinCode"
            placeholder="fed1:..."
            rows="3"
            class="w-full bg-black/30 text-white text-sm rounded-lg p-3 border border-white/10 focus:border-orange-400/50 focus:outline-none font-mono resize-none"
          ></textarea>

          <div v-if="error" class="mt-3 text-sm text-red-400">{{ error }}</div>
          <div v-if="success" class="mt-3 text-sm text-green-400">Successfully joined federation</div>

          <div class="flex gap-3 mt-4">
            <button
              @click="$emit('close')"
              class="flex-1 px-4 py-2 glass-button rounded text-sm text-white/70"
            >Cancel</button>
            <button
              @click="handleJoin"
              class="flex-1 px-4 py-2 glass-button rounded text-sm text-white font-medium disabled:opacity-50"
              :disabled="joining || !joinCode.trim()"
            >
              {{ joining ? 'Joining...' : 'Join' }}
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
  joining: boolean
  error: string
  success: boolean
}>()

const emit = defineEmits<{
  close: []
  join: [code: string]
}>()

const joinCode = ref('')

function handleJoin() {
  if (joinCode.value.trim()) {
    emit('join', joinCode.value.trim())
  }
}

// Clear code on successful join
watch(() => props.success, (val) => {
  if (val) joinCode.value = ''
})
</script>
