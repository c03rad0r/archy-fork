<template>
  <BaseModal
    :show="show"
    title="Request to Peer"
    max-width="max-w-md"
    :z-index="'z-[3200]'"
    @close="$emit('cancel')"
  >
    <p class="text-white/70 text-sm mb-1">
      Send a connection request to
      <span class="text-white font-medium">{{ targetLabel }}</span>
    </p>
    <p class="text-white/45 text-xs mb-4">
      They'll see your request and approve or decline it. Approved peers connect
      at the Peer level — never trusted automatically.
    </p>

    <p v-if="error" role="alert" class="mb-3 text-sm text-red-400">{{ error }}</p>

    <label class="block text-xs text-white/60 mb-1">Message (optional)</label>
    <textarea
      v-model="message"
      rows="3"
      maxlength="280"
      placeholder="Hey — mind if we peer?"
      class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm focus:outline-none focus:border-orange-400/60 resize-none"
    ></textarea>
    <p class="text-[11px] text-white/30 text-right mt-1">{{ message.length }}/280</p>

    <div class="flex gap-2 mt-4">
      <button class="flex-1 glass-button px-4 py-2 rounded-lg text-sm" @click="$emit('cancel')">
        Cancel
      </button>
      <button
        class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
        :disabled="sending"
        @click="$emit('send', message.trim() || undefined)"
      >
        {{ sending ? 'Sending…' : 'Send Request' }}
      </button>
    </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import BaseModal from '@/components/BaseModal.vue'

const props = defineProps<{
  show: boolean
  targetLabel: string
  sending?: boolean
  error?: string
}>()

defineEmits<{
  send: [message: string | undefined]
  cancel: []
}>()

const message = ref('')

watch(
  () => props.show,
  (visible) => {
    if (visible) message.value = ''
  },
)
</script>
