<template>
  <button
    type="button"
    class="copy-btn"
    :class="[sizeClass, copied ? 'copy-btn-copied' : '']"
    :aria-label="copied ? 'Copied' : label"
    @click.stop="copy"
  >
    <!-- Icon swaps to a tick on success; both are the same box so the button
         never changes width mid-feedback (the jump was half the reason the
         old ad-hoc buttons felt broken). -->
    <svg v-if="!copied" class="copy-btn-icon" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-2M8 5a2 2 0 002 2h4a2 2 0 002-2M8 5a2 2 0 012-2h4a2 2 0 012 2m0 0h2a2 2 0 012 2v3" />
    </svg>
    <svg v-else class="copy-btn-icon" fill="none" stroke="currentColor" stroke-width="2.5" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
    </svg>
    <span v-if="!iconOnly" class="copy-btn-text">{{ copied ? copiedLabel : label }}</span>
  </button>
</template>

<script setup lang="ts">
import { ref, computed, onUnmounted } from 'vue'

// One copy button for the whole app. Before this, 25 call sites each rolled
// their own markup and feedback — some flipped a label, some did nothing at
// all, and the widths jumped. Every copy affordance should look and behave
// identically, so this owns both the visual and the timing.
const props = withDefaults(defineProps<{
  /** Text to place on the clipboard. */
  value: string
  /** Button label in the idle state. */
  label?: string
  /** Label shown while the success state is held. */
  copiedLabel?: string
  /** Icon with no text — for tight rows next to a truncated value. */
  iconOnly?: boolean
  size?: 'sm' | 'md'
}>(), {
  label: 'Copy',
  copiedLabel: 'Copied',
  iconOnly: false,
  size: 'sm',
})

const emit = defineEmits<{ copied: [] }>()

/** How long the success state is held. Long enough to register, short enough
 *  that a second copy doesn't feel blocked. */
const FEEDBACK_MS = 1600

const copied = ref(false)
let timer: ReturnType<typeof setTimeout> | null = null

const sizeClass = computed(() => (props.size === 'md' ? 'copy-btn-md' : 'copy-btn-sm'))

async function copy() {
  try {
    await navigator.clipboard.writeText(props.value)
  } catch {
    // Clipboard can reject (insecure origin, denied permission). Fall back to
    // the legacy path so the button still works over plain http on a LAN IP,
    // which is how plenty of nodes are reached.
    try {
      const ta = document.createElement('textarea')
      ta.value = props.value
      ta.setAttribute('readonly', '')
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      document.body.removeChild(ta)
    } catch {
      return // genuinely could not copy — don't claim success
    }
  }
  copied.value = true
  emit('copied')
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => {
    copied.value = false
    timer = null
  }, FEEDBACK_MS)
}

onUnmounted(() => {
  if (timer) clearTimeout(timer)
})
</script>

<style scoped>
.copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  border-radius: 0.5rem;
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.75);
  transition: background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  white-space: nowrap;
}
.copy-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}
.copy-btn-sm {
  padding: 0.35rem 0.6rem;
  font-size: 0.75rem;
}
.copy-btn-md {
  padding: 0.5rem 0.85rem;
  font-size: 0.875rem;
}
.copy-btn-icon {
  width: 0.95rem;
  height: 0.95rem;
  flex-shrink: 0;
}
.copy-btn-md .copy-btn-icon {
  width: 1.05rem;
  height: 1.05rem;
}
/* Success: emerald, matching the paid/settled language used elsewhere in the wallet. */
.copy-btn-copied {
  background: rgba(16, 185, 129, 0.16);
  border-color: rgba(16, 185, 129, 0.4);
  color: rgb(110, 231, 183);
}
.copy-btn-copied:hover {
  background: rgba(16, 185, 129, 0.22);
  color: rgb(110, 231, 183);
}
@media (prefers-reduced-motion: reduce) {
  .copy-btn {
    transition: none;
  }
}
</style>
