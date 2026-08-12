<template>
  <!-- Mobile-only DID copy/rotate card. Lives BELOW the view tabs in
       Federation.vue (not in the header) and is hidden by the parent on the
       Network Map tab, where vertical space belongs to the map. -->
  <div v-if="selfDid" class="md:hidden glass-card px-4 py-3 mb-6 flex items-center gap-3">
    <div class="min-w-0 flex-1">
      <p class="text-[10px] text-white/40 mb-0.5">{{ serverName }}</p>
      <p class="text-xs text-white/80 font-mono truncate cursor-pointer" :title="selfDid" @click="handleCopy">{{ didCopied ? 'Copied!' : shortDidDisplay }}</p>
    </div>
    <button @click="handleCopy" class="glass-button px-2.5 py-1 rounded text-[10px]">{{ didCopied ? 'Copied!' : 'Copy' }}</button>
    <button @click="$emit('rotate')" class="glass-button px-2.5 py-1 rounded text-[10px] text-orange-300">Rotate</button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { shortDid } from './utils'
import { safeClipboardWrite } from '../web5/utils'

const props = defineProps<{
  selfDid: string
  serverName: string
}>()

defineEmits<{
  rotate: []
}>()

const didCopied = ref(false)
const shortDidDisplay = computed(() => shortDid(props.selfDid))

function handleCopy() {
  if (props.selfDid) {
    safeClipboardWrite(props.selfDid)
    didCopied.value = true
    setTimeout(() => { didCopied.value = false }, 2000)
  }
}
</script>
