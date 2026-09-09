<template>
  <div class="py-3 text-center">
    <NostrIdentityOrb class="mx-auto" size="large" state="success" aria-label="Nostr identity updated" />
    <p class="mt-3 text-[10px] uppercase tracking-[0.24em] text-green-300/70">Identity saved</p>
    <h2 id="profile-editor-title" class="mt-1 text-2xl font-bold tracking-wide text-white">IDENTITY UPDATED</h2>
    <p class="mt-2 text-sm text-white/55">{{ identityName }} is updated on this node.</p>

    <div class="mt-6 space-y-3 rounded-xl bg-white/5 p-4 text-left">
      <div>
        <p class="mb-1 text-xs text-white/45">Relay publication</p>
        <p class="text-sm text-white/80">{{ relaySummary }}</p>
      </div>
      <div v-if="eventId">
        <p class="mb-1 text-xs text-white/45">Nostr event ID</p>
        <div class="flex items-center gap-2">
          <p class="min-w-0 flex-1 break-all font-mono text-xs text-white/75">{{ eventId }}</p>
          <CopyButton class="shrink-0" :value="eventId" />
        </div>
      </div>
      <p v-if="relayNote" class="text-xs leading-relaxed text-amber-200/70">{{ relayNote }}</p>
    </div>

    <div class="mt-6 flex gap-3">
      <button type="button" class="glass-button flex-1 rounded-xl px-4 py-3 text-sm font-medium" @click="$emit('again')">Keep editing</button>
      <button type="button" class="glass-button glass-button-warning flex-1 rounded-xl px-4 py-3 text-sm font-semibold" @click="$emit('done')">Done</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import CopyButton from '@/components/CopyButton.vue'
import NostrIdentityOrb from '@/components/NostrIdentityOrb.vue'

const props = withDefaults(defineProps<{
  identityName: string
  eventId?: string
  accepted?: number
  attempted?: number
  relayNote?: string
}>(), { eventId: '', accepted: 0, attempted: 0, relayNote: '' })

defineEmits<{ again: []; done: [] }>()

const relaySummary = computed(() => {
  if (!props.attempted) return 'Saved locally; no relay publication was attempted.'
  if (props.accepted === props.attempted) return `Published to all ${props.attempted} configured relays.`
  return `Published to ${props.accepted}/${props.attempted} configured relays.`
})
</script>
