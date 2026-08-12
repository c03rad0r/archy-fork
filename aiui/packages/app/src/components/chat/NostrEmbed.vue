<template>
  <div
    class="rounded-xl p-3 transition-all duration-150"
    :class="isDark
      ? 'bg-purple-500/10 border border-purple-500/20'
      : 'bg-purple-50 border border-purple-200'"
  >
    <!-- Loading -->
    <div v-if="loading" class="flex items-center gap-2">
      <div
        class="w-6 h-6 rounded-full animate-pulse"
        :class="isDark ? 'bg-purple-500/20' : 'bg-purple-200'"
      />
      <div class="flex-1 space-y-1">
        <div
          class="h-3 w-24 rounded animate-pulse"
          :class="isDark ? 'bg-white/10' : 'bg-gray-200'"
        />
        <div
          class="h-2 w-40 rounded animate-pulse"
          :class="isDark ? 'bg-white/5' : 'bg-gray-100'"
        />
      </div>
    </div>

    <!-- Error -->
    <div v-else-if="error" class="flex items-center gap-2">
      <span class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
        {{ isProfile ? 'Profile' : 'Note' }} not found
      </span>
      <span
        class="text-xs font-mono truncate"
        :class="isDark ? 'text-purple-400/40' : 'text-purple-400'"
      >
        {{ truncatedId }}
      </span>
    </div>

    <!-- Note content -->
    <div v-else-if="note">
      <div class="flex items-center gap-2 mb-1.5">
        <div
          class="w-6 h-6 rounded-full shrink-0 flex items-center justify-center text-xs font-bold"
          :class="isDark ? 'bg-purple-500/20 text-purple-400' : 'bg-purple-100 text-purple-600'"
        >
          {{ note.authorName?.charAt(0)?.toUpperCase() ?? '?' }}
        </div>
        <span
          class="text-xs font-semibold truncate"
          :class="isDark ? 'text-white/70' : 'text-gray-700'"
        >
          {{ note.authorName ?? 'anon' }}
        </span>
        <span
          class="text-xs ml-auto shrink-0"
          :class="isDark ? 'text-white/20' : 'text-gray-300'"
        >
          {{ formatTime(note.created_at) }}
        </span>
      </div>
      <p
        class="text-xs leading-relaxed line-clamp-4"
        :class="isDark ? 'text-white/60' : 'text-gray-600'"
      >
        {{ note.content }}
      </p>
      <div class="flex items-center gap-2 mt-1.5">
        <span
          class="text-xs font-mono"
          :class="isDark ? 'text-purple-400/40' : 'text-purple-400/60'"
        >
          nostr
        </span>
      </div>
    </div>

    <!-- Profile card (npub) -->
    <div v-else-if="isProfile">
      <div class="flex items-center gap-2">
        <div
          class="w-8 h-8 rounded-full shrink-0 flex items-center justify-center text-xs font-bold"
          :class="isDark ? 'bg-purple-500/20 text-purple-400' : 'bg-purple-100 text-purple-600'"
        >
          {{ truncatedId.charAt(0).toUpperCase() }}
        </div>
        <div>
          <span
            class="text-xs font-mono block"
            :class="isDark ? 'text-white/60' : 'text-gray-600'"
          >
            {{ truncatedId }}
          </span>
          <span
            class="text-xs"
            :class="isDark ? 'text-purple-400/40' : 'text-purple-400/60'"
          >
            nostr profile
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useNostr, type NostrNote } from '@/composables/useNostr'
import { decodeNIP19 } from '@/utils/bech32'

const props = defineProps<{
  uri: string
}>()

const { isDark } = useTheme()
const { connect, fetchNote: fetchFromRelay } = useNostr()

const note = ref<NostrNote | null>(null)
const loading = ref(false)
const error = ref(false)

const decoded = computed(() => {
  const raw = props.uri.replace(/^nostr:/, '')
  return decodeNIP19(raw)
})

const isProfile = computed(() => decoded.value?.type === 'npub' || decoded.value?.type === 'nprofile')

const truncatedId = computed(() => {
  const hex = decoded.value?.hex ?? ''
  if (hex.length <= 12) return hex
  return hex.slice(0, 8) + '...' + hex.slice(-4)
})

function formatTime(ts: number): string {
  const diff = Math.floor(Date.now() / 1000 - ts)
  if (diff < 60) return 'now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h`
  return `${Math.floor(diff / 86400)}d`
}

onMounted(async () => {
  if (!decoded.value) {
    error.value = true
    return
  }

  // For profiles, just show the card with no fetch needed
  if (isProfile.value) return

  // For notes/events, fetch from relay
  const hexId = decoded.value.hex
  if (!hexId) {
    error.value = true
    return
  }

  loading.value = true
  connect()

  // Small delay to allow relay connections to establish
  await new Promise(r => setTimeout(r, 500))

  const result = await fetchFromRelay(hexId)
  loading.value = false
  if (result) {
    note.value = result
  } else {
    error.value = true
  }
})
</script>
