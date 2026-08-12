<template>
  <div
    class="p-3 md:p-4 relative"
    @dragover.prevent="onDragOver"
    @dragleave="onDragLeave"
    @drop.prevent="onDrop"
  >
    <SearchResults
      v-if="isSearchMode"
      :results="searchResults"
      :is-searching="isSearching"
      @select="handleSearchSelect"
    />

    <!-- Reply-to quote -->
    <div
      v-if="replyTo"
      class="mb-2 flex items-start gap-2 rounded-xl bg-white/5 border border-white/10 px-3 py-2 animate-fade-up-fast"
    >
      <div class="flex-1 min-w-0">
        <p class="text-xs text-accent/70 font-medium mb-0.5">Replying to</p>
        <p class="text-xs text-white/50 truncate">{{ replyTo.excerpt }}</p>
      </div>
      <button
        class="shrink-0 touch-target rounded-md hover:bg-white/10 transition-colors text-white/40 hover:text-white/70"
        aria-label="Cancel reply"
        @click="$emit('clearReply')"
      >
        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Image thumbnails -->
    <div v-if="images.length > 0" class="mb-2 flex gap-2 flex-wrap animate-fade-up-fast">
      <div
        v-for="(img, i) in images"
        :key="i"
        class="relative group w-16 h-16 rounded-lg overflow-hidden border border-white/10 bg-white/5"
      >
        <img
          :src="`data:${img.mediaType};base64,${img.data}`"
          :alt="`Attached image ${i + 1}`"
          class="w-full h-full object-cover"
        />
        <button
          class="absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity"
          aria-label="Remove image"
          @click="removeImage(i)"
        >
          <svg class="w-4 h-4 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <span v-if="images.length >= MAX_IMAGES" class="self-center text-xs text-white/30">
        Max {{ MAX_IMAGES }} images
      </span>
    </div>

    <!-- Prompt palette -->
    <PromptPalette
      ref="paletteRef"
      :query="paletteQuery"
      :is-open="isPaletteMode"
      @select="handlePaletteSelect"
      @close="closePalette"
    />

    <!-- Drag overlay -->
    <div
      v-if="isDragging"
      class="absolute inset-0 z-10 flex items-center justify-center rounded-2xl border-2 border-dashed border-accent/50 bg-accent/5 backdrop-blur-sm pointer-events-none"
    >
      <p class="text-sm text-accent/80 font-medium">Drop image here</p>
    </div>

    <div
      class="rounded-2xl px-4 py-3 flex items-center gap-2 transition-all duration-300"
      :class="[
        isCodeMode
          ? 'bg-accent/15 border border-accent/25 backdrop-blur-xl'
          : 'path-glass-bubble',
        focused ? (isCodeMode ? 'border-accent/40' : 'border-white/30') : '',
      ]"
    >
      <!-- Image attach button -->
      <button
        v-if="!streaming && images.length < MAX_IMAGES"
        class="shrink-0 min-w-[44px] min-h-[44px] -my-2 flex items-center justify-center rounded-lg text-white/40 hover:text-white/70 hover:bg-white/10 transition-all"
        aria-label="Attach image"
        @click="openFilePicker"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </button>

      <textarea
        ref="textareaRef"
        v-model="text"
        rows="1"
        :placeholder="placeholder"
        class="flex-1 resize-none bg-transparent text-base outline-none min-h-[24px] max-h-[120px] text-white/90 placeholder:text-white/25"
        @keydown="handleKeydown"
        @input="autoResize"
        @paste="onPaste"
        @focus="onInputFocus"
        @blur="focused = false"
      />
      <button
        v-if="streaming"
        class="shrink-0 path-glass-button path-glass-button-sm rounded-xl px-3 transition-all duration-200 hover:opacity-80 active:scale-95"
        aria-label="Stop generation"
        @click="$emit('stop')"
      >
        <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
      </button>
      <template v-else>
        <!-- Extract/contextualize button — shown after paste -->
        <button
          v-if="hasPasted && canSend"
          :disabled="!canSend"
          class="shrink-0 path-glass-button path-glass-button-sm rounded-xl px-3 transition-all duration-200 hover:opacity-80 active:scale-95 text-accent/80"
          aria-label="Extract content"
          title="Contextualize — extract media without sending to AI"
          @click="extract"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
          </svg>
        </button>
        <!-- Send button -->
        <button
          :disabled="!canSend"
          class="shrink-0 rounded-xl px-3 transition-all duration-200"
          :class="[
            isCodeMode ? 'bg-accent/80 text-white' : 'path-glass-button path-glass-button-sm',
            canSend
              ? 'hover:opacity-80 active:scale-95'
              : 'opacity-30 cursor-not-allowed',
          ]"
          :style="isCodeMode ? 'height: 32px' : ''"
          aria-label="Send message"
          @click="send"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5" />
          </svg>
        </button>
      </template>
    </div>

    <!-- Hidden file input -->
    <input
      ref="fileInputRef"
      type="file"
      accept="image/jpeg,image/png,image/gif,image/webp"
      multiple
      class="hidden"
      @change="onFileSelect"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, watch, onMounted, onBeforeUnmount } from 'vue'
import { archyBridge } from '../../services/archyBridge'
import { useFederatedSearch, type SearchResult } from '@/composables/useFederatedSearch'
import SearchResults from '@/components/ui/SearchResults.vue'
import PromptPalette from './PromptPalette.vue'
import type { ImageAttachment } from '@aiui/core/types/message'

const MAX_IMAGES = 4
const ACCEPTED_TYPES = ['image/jpeg', 'image/png', 'image/gif', 'image/webp']

const props = withDefaults(
  defineProps<{
    disabled?: boolean
    streaming?: boolean
    placeholder?: string
    activeTab?: string
    replyTo?: { messageId: string; excerpt: string } | null
  }>(),
  {
    disabled: false,
    streaming: false,
    placeholder: 'Message AIUI...',
    activeTab: '',
    replyTo: null,
  }
)

const isCodeMode = computed(() => props.activeTab === 'code')

const emit = defineEmits<{
  send: [text: string, images: ImageAttachment[]]
  extract: [text: string]
  stop: []
  clearReply: []
}>()

const text = ref('')
const focused = ref(false)
const hasPasted = ref(false)
const isDragging = ref(false)
const textareaRef = ref<HTMLTextAreaElement | null>(null)
const fileInputRef = ref<HTMLInputElement | null>(null)
const images = ref<ImageAttachment[]>([])

const paletteRef = ref<InstanceType<typeof PromptPalette> | null>(null)

// Archy's ⌘K search can hand us the text the operator typed there
// ("Talk to AIUI about it"). We prefill and focus rather than auto-sending:
// the operator gets to see and amend the question before it costs a model
// call. Anything already half-typed here wins — their own draft is not
// clobbered by a background handoff.
let releasePrefill: (() => void) | null = null

onMounted(() => {
  releasePrefill = archyBridge.onPrefill((incoming: string) => {
    if (text.value.trim()) return
    text.value = incoming
    nextTick(() => {
      autoResize()
      textareaRef.value?.focus()
      // Caret to the end so they can keep typing straight away.
      const el = textareaRef.value
      if (el) el.setSelectionRange(el.value.length, el.value.length)
    })
  })
})

onBeforeUnmount(() => {
  releasePrefill?.()
  releasePrefill = null
})

// Prompt palette: opens when text starts with "/" and has no space yet
const isPaletteMode = computed(() => {
  const t = text.value.trimStart()
  return t === '/' || (t.startsWith('/') && !t.includes(' '))
})

const paletteQuery = computed(() => {
  if (!isPaletteMode.value) return ''
  return text.value.trimStart().slice(1) // strip leading /
})

function handlePaletteSelect(templateText: string) {
  // Slash commands — send immediately (except /search which needs query input)
  if (templateText.startsWith('/') && !templateText.includes('{{')) {
    text.value = templateText
    if (templateText.trimEnd() === '/search') {
      // /search needs a query — set text and let user type
      nextTick(() => {
        autoResize()
        textareaRef.value?.focus()
      })
      return
    }
    nextTick(() => send())
    return
  }
  text.value = templateText
  nextTick(() => {
    autoResize()
    textareaRef.value?.focus()
  })
}

function closePalette() {
  // Add a space to exit palette mode
  if (text.value.trimStart() === '/') {
    text.value = ''
  }
}

function onInputFocus() {
  focused.value = true
  nextTick(() => {
    textareaRef.value?.scrollIntoView({ block: 'end', behavior: 'smooth' })
  })
}

function handleKeydown(e: KeyboardEvent) {
  if (isPaletteMode.value && !paletteRef.value?.hasSelectedTemplate) {
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      paletteRef.value?.navigateUp()
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      paletteRef.value?.navigateDown()
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      paletteRef.value?.selectHighlighted()
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      closePalette()
      return
    }
  }
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    send()
  }
}

const canSend = computed(() => (text.value.trim().length > 0 || images.value.length > 0) && !props.disabled)

function send() {
  if (!canSend.value) return
  emit('send', text.value.trim(), [...images.value])
  text.value = ''
  images.value = []
  hasPasted.value = false
  nextTick(autoResize)
}

function extract() {
  if (!canSend.value) return
  emit('extract', text.value.trim())
  text.value = ''
  hasPasted.value = false
  nextTick(autoResize)
}

function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items
  if (!items) {
    hasPasted.value = true
    return
  }

  let hasImage = false
  for (const item of items) {
    if (ACCEPTED_TYPES.includes(item.type)) {
      hasImage = true
      const file = item.getAsFile()
      if (file) addImageFile(file)
    }
  }

  if (!hasImage) {
    hasPasted.value = true
  }
}

function onDragOver(e: DragEvent) {
  if (e.dataTransfer?.types.includes('Files')) {
    isDragging.value = true
  }
}

function onDragLeave() {
  isDragging.value = false
}

function onDrop(e: DragEvent) {
  isDragging.value = false
  const files = e.dataTransfer?.files
  if (!files) return
  for (const file of files) {
    if (ACCEPTED_TYPES.includes(file.type)) {
      addImageFile(file)
    }
  }
}

function openFilePicker() {
  fileInputRef.value?.click()
}

function onFileSelect(e: Event) {
  const input = e.target as HTMLInputElement
  const files = input.files
  if (!files) return
  for (const file of files) {
    if (ACCEPTED_TYPES.includes(file.type)) {
      addImageFile(file)
    }
  }
  // Reset input so same file can be re-selected
  input.value = ''
}

function addImageFile(file: File) {
  if (images.value.length >= MAX_IMAGES) return

  const reader = new FileReader()
  reader.onload = () => {
    const result = reader.result as string
    // Strip the data:...;base64, prefix
    const base64 = result.split(',')[1]
    if (base64) {
      images.value.push({ data: base64, mediaType: file.type })
    }
  }
  reader.readAsDataURL(file)
}

function removeImage(index: number) {
  images.value.splice(index, 1)
}

function autoResize() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 120) + 'px'
}

// Federated search via /search command
const { results: searchResults, isSearching, search: doSearch, clear: clearSearch } = useFederatedSearch()

const isSearchMode = computed(() => text.value.trimStart().startsWith('/search '))

watch(text, (val) => {
  if (isSearchMode.value) {
    const searchQuery = val.trimStart().replace(/^\/search\s+/, '')
    doSearch(searchQuery)
  } else {
    clearSearch()
  }
})

function handleSearchSelect(result: SearchResult) {
  // Insert content reference tag based on type
  const tags: Record<string, string> = {
    film: `[[film:${result.id}]]`,
    song: `[[song:${result.id}]]`,
    podcast: `[[podcast:${result.id}]]`,
  }
  text.value = tags[result.type] ?? result.title
  clearSearch()
  nextTick(() => textareaRef.value?.focus())
}
</script>
