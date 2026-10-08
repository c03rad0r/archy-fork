<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { websitePreview } from '@/services/publishing'
const props = defineProps<{
  publication: { html: string; public_archive?: string | null }
  archive?: { sha256: string; size: number } | null
  address?: string | null
  busy?: boolean
}>()
const emit = defineEmits<{ change: [enabled: boolean] }>()
const approved = ref(false)
watch(() => [props.publication, props.archive], () => { approved.value = false })
const fileAddress = computed(() => props.address && props.publication.public_archive
  ? `${props.address.replace(/\/$/, '')}/${props.publication.public_archive}` : null)
</script>

<template>
  <details class="rounded-xl border border-white/10 p-4">
    <summary class="cursor-pointer text-sm font-medium text-white/80">Share the archived website file</summary>
    <div class="space-y-4 pt-4">
      <p class="text-sm text-white/60">Let visitors download this exact website snapshot by its Blossom content hash. Other files and the Blossom app stay private. No upload or Nostr announcement is made.</p>
      <template v-if="publication.public_archive">
        <p class="text-sm">This snapshot is shared on this connection.</p>
        <p v-if="fileAddress" class="font-mono text-xs break-all select-all">{{ fileAddress }}</p>
        <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy" @click="emit('change', false)">Stop sharing this file</button>
      </template>
      <template v-else-if="archive">
        <p class="text-sm text-white/60">The archived file must match the published preview below. Store and publish the same version before sharing.</p>
        <p class="font-mono text-xs break-all">Archived SHA-256: {{ archive.sha256 }} · {{ archive.size }} bytes</p>
        <iframe :srcdoc="websitePreview(publication.html)" sandbox="" referrerpolicy="no-referrer" title="Archived file publication preview" class="w-full h-64 rounded-xl bg-white" />
        <label class="flex items-start gap-3 text-sm"><input v-model="approved" type="checkbox" class="mt-1" /><span>I approve public downloads of this exact snapshot on this connection. Copies may remain after I stop sharing.</span></label>
        <button class="glass-button glass-button-sm rounded-lg px-5 py-2 text-sm font-medium" :disabled="busy || !approved" @click="emit('change', true)">Share this archived file</button>
      </template>
      <p v-else class="text-sm text-white/60">In “Create your website”, save a signed copy in local Blossom first.</p>
      <p class="text-xs text-white/50">Publishing an update turns file sharing off until you review that version again.</p>
    </div>
  </details>
</template>
