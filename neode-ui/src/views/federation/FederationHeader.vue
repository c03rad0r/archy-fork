<template>
  <div class="mb-6">
    <BackButton label="Web5" @click="router.push('/dashboard/web5')" />
    <div class="flex items-start justify-between gap-4">
      <div>
        <h1 class="text-3xl font-bold text-white mb-2">Federation & Peers</h1>
        <p class="text-white/70">Connect, sync, and share with trusted nodes</p>
      </div>
      <!-- Your Node DID — top right card (desktop) -->
      <div v-if="selfDid" class="hidden md:block shrink-0">
        <div class="glass-card px-4 py-3 flex items-center gap-3">
          <div class="min-w-0">
            <p class="text-[10px] text-white/40 mb-0.5">{{ serverName }}</p>
            <p class="text-xs text-white/80 font-mono cursor-pointer" :title="selfDid" @click="handleCopy">{{ didCopied ? 'Copied!' : shortDidDisplay }}</p>
          </div>
          <button @click="handleCopy" class="glass-button px-2.5 py-1 rounded text-[10px]">{{ didCopied ? 'Copied!' : 'Copy' }}</button>
          <button @click="$emit('rotate')" class="glass-button px-2.5 py-1 rounded text-[10px] text-orange-300">Rotate</button>
        </div>
      </div>
    </div>
    <!-- Mobile DID card moved to DidCardMobile.vue, rendered by
         Federation.vue below the view tabs (hidden on the map tab). -->
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import BackButton from '@/components/BackButton.vue'
import { shortDid } from './utils'
import { safeClipboardWrite } from '../web5/utils'

const props = defineProps<{
  selfDid: string
  serverName: string
}>()

defineEmits<{
  rotate: []
}>()

const router = useRouter()
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
