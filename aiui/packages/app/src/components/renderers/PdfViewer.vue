<template>
  <div class="pdf-viewer h-full flex flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-2 px-4 py-2 bg-black/60 backdrop-blur-md border-b border-white/5 shrink-0">
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
        title="Back"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <span class="text-xs text-white/40 truncate flex-1">{{ title || 'PDF' }}</span>

      <!-- Page nav -->
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors disabled:opacity-30"
        :disabled="currentPage <= 1"
        title="Previous page"
        @click="goToPage(currentPage - 1)"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <span class="text-xs text-white/50 tabular-nums min-w-[60px] text-center">{{ currentPage }} / {{ totalPages }}</span>
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors disabled:opacity-30"
        :disabled="currentPage >= totalPages"
        title="Next page"
        @click="goToPage(currentPage + 1)"
      >
        <svg class="w-4 h-4 rotate-180" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <!-- Zoom -->
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors disabled:opacity-30"
        :disabled="scale <= 0.5"
        title="Zoom out"
        @click="scale = Math.max(0.5, scale - 0.25)"
      >
        <span class="text-xs font-bold">−</span>
      </button>
      <span class="text-xs text-white/40 tabular-nums w-10 text-center">{{ Math.round(scale * 100) }}%</span>
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors disabled:opacity-30"
        :disabled="scale >= 2"
        title="Zoom in"
        @click="scale = Math.min(2, scale + 0.25)"
      >
        <span class="text-xs font-bold">+</span>
      </button>
    </div>

    <!-- PDF canvas container -->
    <div ref="containerRef" class="flex-1 overflow-auto bg-black/20 p-4 flex justify-center">
      <div v-if="loading" class="flex items-center justify-center h-full">
        <div class="text-sm text-white/50">Loading PDF...</div>
      </div>
      <div v-else-if="error" class="flex items-center justify-center h-full">
        <div class="text-sm text-red-400/70">{{ error }}</div>
      </div>
      <canvas
        v-show="!loading && !error"
        ref="canvasRef"
        class="shadow-lg rounded"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, shallowRef } from 'vue'

const props = defineProps<{
  url: string
  title?: string
}>()

defineEmits<{ back: [] }>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
const containerRef = ref<HTMLElement | null>(null)
const currentPage = ref(1)
const totalPages = ref(0)
const scale = ref(1)
const loading = ref(true)
const error = ref('')

// Lazy-load pdfjs-dist
type PDFDocumentProxy = import('pdfjs-dist').PDFDocumentProxy
const pdfDoc = shallowRef<PDFDocumentProxy | null>(null)

async function loadPdf() {
  loading.value = true
  error.value = ''
  try {
    const pdfjsLib = await import('pdfjs-dist')
    // Set worker source
    pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
      'pdfjs-dist/build/pdf.worker.min.mjs',
      import.meta.url
    ).toString()

    const doc = await pdfjsLib.getDocument(props.url).promise
    pdfDoc.value = doc
    totalPages.value = doc.numPages
    currentPage.value = 1
    await renderPage()
  } catch (e) {
    error.value = `Failed to load PDF: ${e instanceof Error ? e.message : 'Unknown error'}`
  } finally {
    loading.value = false
  }
}

async function renderPage() {
  const doc = pdfDoc.value
  const canvas = canvasRef.value
  if (!doc || !canvas) return

  try {
    const page = await doc.getPage(currentPage.value)
    const viewport = page.getViewport({ scale: scale.value })

    canvas.height = viewport.height
    canvas.width = viewport.width

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    await page.render({
      canvasContext: ctx,
      canvas: canvas,
      viewport,
    }).promise
  } catch (e) {
    error.value = `Failed to render page: ${e instanceof Error ? e.message : 'Unknown error'}`
  }
}

function goToPage(page: number) {
  if (page < 1 || page > totalPages.value) return
  currentPage.value = page
}

watch(currentPage, () => renderPage())
watch(scale, () => renderPage())

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
    goToPage(currentPage.value - 1)
    e.preventDefault()
  } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
    goToPage(currentPage.value + 1)
    e.preventDefault()
  }
}

onMounted(() => {
  loadPdf()
  window.addEventListener('keydown', handleKeyDown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeyDown)
  pdfDoc.value?.destroy()
})
</script>
