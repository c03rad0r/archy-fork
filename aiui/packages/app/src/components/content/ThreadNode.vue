<template>
  <div :style="{ paddingLeft: `${Math.min(depth, 4) * 16}px` }">
    <div class="rounded-lg bg-white/[0.03] border border-white/5 p-2.5 mb-1">
      <div class="flex items-center gap-1.5 mb-1">
        <div class="w-5 h-5 rounded-full shrink-0 flex items-center justify-center text-xs font-bold bg-purple-500/20 text-purple-400">
          {{ node.note.authorName?.charAt(0)?.toUpperCase() ?? '?' }}
        </div>
        <span class="text-xs font-semibold text-white/70">{{ node.note.authorName ?? 'anon' }}</span>
        <span class="text-xs ml-auto text-white/20">{{ formatTime(node.note.created_at) }}</span>
      </div>
      <p class="text-xs text-white/60 leading-relaxed whitespace-pre-wrap">{{ node.note.content }}</p>
      <button
        class="text-xs text-white/25 hover:text-accent/60 mt-1 transition-colors"
        @click="$emit('reply', node.note)"
      >
        Reply
      </button>
    </div>

    <!-- Children (recursive) -->
    <ThreadNode
      v-for="child in node.children"
      :key="child.note.id"
      :node="child"
      :depth="depth + 1"
      @reply="(note: NostrNote) => $emit('reply', note)"
    />
  </div>
</template>

<script setup lang="ts">
import type { NostrNote } from '@/composables/useNostr'

interface ThreadTreeNode {
  note: NostrNote
  children: ThreadTreeNode[]
}

defineProps<{
  node: ThreadTreeNode
  depth: number
}>()

defineEmits<{ reply: [note: NostrNote] }>()

function formatTime(ts: number): string {
  const d = new Date(ts * 1000)
  return d.toLocaleTimeString('en', { hour: '2-digit', minute: '2-digit' })
}
</script>
