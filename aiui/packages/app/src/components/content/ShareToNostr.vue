<template>
  <div v-if="showCompose" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
    <div class="w-full max-w-md mx-4 rounded-2xl bg-[#0a0a0a] border border-white/10 p-5 space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-bold text-white/90">Share to Nostr</h3>
        <button
          class="p-2 min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/40 hover:text-white/70 hover:bg-white/5 transition-colors"
          @click="close"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <textarea
        v-model="noteContent"
        class="w-full h-32 px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors resize-none"
        placeholder="Add a note about this content..."
      />

      <div class="rounded-lg bg-white/[0.03] border border-white/5 p-3">
        <p class="text-xs text-white/25 mb-1">Preview</p>
        <p class="text-xs text-white/60 whitespace-pre-wrap">{{ previewText }}</p>
      </div>

      <div class="flex gap-2">
        <button
          class="flex-1 min-h-[44px] rounded-lg text-sm font-medium text-white/40 hover:text-white/70 hover:bg-white/5 transition-colors"
          @click="close"
        >
          Cancel
        </button>
        <button
          class="flex-1 min-h-[44px] rounded-lg text-sm font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
          :disabled="isPublishing || !isLoggedIn"
          @click="publish"
        >
          {{ isPublishing ? 'Publishing...' : 'Publish' }}
        </button>
      </div>

      <p v-if="!isLoggedIn" class="text-xs text-yellow-400/60 text-center">
        Sign in with Nostr to share
      </p>

      <p v-if="publishResult" class="text-xs text-center" :class="publishOk ? 'text-emerald-400/60' : 'text-red-400/60'">
        {{ publishResult }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useNostrIdentity } from '@/composables/useNostrIdentity'
import { useNostr } from '@/composables/useNostr'

const props = defineProps<{
  title: string
  type: string
  description?: string
  url?: string
}>()

const emit = defineEmits<{ close: [] }>()

const { isLoggedIn, signEvent } = useNostrIdentity()
const { publishEvent } = useNostr()

const showCompose = ref(true)
const noteContent = ref('')
const isPublishing = ref(false)
const publishResult = ref('')
const publishOk = ref(false)

const previewText = computed(() => {
  const parts: string[] = []
  if (noteContent.value.trim()) {
    parts.push(noteContent.value.trim())
    parts.push('')
  }
  parts.push(`${props.type.charAt(0).toUpperCase() + props.type.slice(1)}: ${props.title}`)
  if (props.description) parts.push(props.description)
  if (props.url) parts.push(props.url)
  return parts.join('\n')
})

function close() {
  showCompose.value = false
  emit('close')
}

async function publish() {
  isPublishing.value = true
  publishResult.value = ''

  const content = previewText.value

  const unsigned = {
    kind: 1,
    created_at: Math.floor(Date.now() / 1000),
    tags: [] as string[][],
    content,
  }

  const signed = await signEvent(unsigned)
  if (!signed) {
    isPublishing.value = false
    publishResult.value = 'Signing failed'
    publishOk.value = false
    return
  }

  const results = await publishEvent(signed)
  const successes = results.filter(r => r.success).length

  isPublishing.value = false
  if (successes > 0) {
    publishResult.value = `Published to ${successes}/${results.length} relays`
    publishOk.value = true
    setTimeout(close, 1500)
  } else {
    publishResult.value = 'Failed to publish'
    publishOk.value = false
  }
}
</script>
