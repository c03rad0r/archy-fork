<template>
  <div class="article-reader h-full flex">
    <!-- TOC Sidebar (desktop only) -->
    <aside
      v-if="headings.length > 1"
      class="hidden lg:flex flex-col w-56 shrink-0 border-r border-white/5 overflow-y-auto scrollbar-hide py-4 px-3"
    >
      <p class="text-xs uppercase tracking-wider text-white/30 mb-2 px-2">Contents</p>
      <button
        v-for="(h, i) in headings"
        :key="i"
        class="text-left text-xs leading-relaxed py-1 px-2 rounded transition-colors truncate"
        :class="[
          activeHeadingIdx === i ? 'text-accent bg-accent/10' : 'text-white/50 hover:text-white/70 hover:bg-white/5',
          h.level === 3 ? 'pl-5' : ''
        ]"
        @click="scrollToHeading(h.id)"
      >
        {{ h.text }}
      </button>
    </aside>

    <!-- Main content -->
    <div ref="contentRef" class="flex-1 overflow-y-auto scrollbar-hide">
      <!-- Header bar -->
      <div class="sticky top-0 z-10 flex items-center gap-2 px-4 py-2 bg-black/60 backdrop-blur-md border-b border-white/5">
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          title="Back"
          @click="$emit('back')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <span class="flex-1 text-xs text-white/40 truncate">{{ readingTime }} min read</span>

        <!-- TOC toggle (mobile) -->
        <button
          v-if="headings.length > 1"
          class="lg:hidden min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          title="Table of contents"
          @click="showMobileToc = !showMobileToc"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h7" />
          </svg>
        </button>

        <!-- Font size -->
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          title="Decrease font size"
          :disabled="fontSizeIdx <= 0"
          @click="fontSizeIdx = Math.max(0, fontSizeIdx - 1)"
        >
          <span class="text-xs font-bold">A-</span>
        </button>
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          title="Increase font size"
          :disabled="fontSizeIdx >= fontSizes.length - 1"
          @click="fontSizeIdx = Math.min(fontSizes.length - 1, fontSizeIdx + 1)"
        >
          <span class="text-xs font-bold">A+</span>
        </button>

        <!-- Print -->
        <button
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
          title="Print"
          @click="printArticle"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 17h2a2 2 0 002-2v-4a2 2 0 00-2-2H5a2 2 0 00-2 2v4a2 2 0 002 2h2m2 4h6a2 2 0 002-2v-4a2 2 0 00-2-2H9a2 2 0 00-2 2v4a2 2 0 002 2zm8-12V5a2 2 0 00-2-2H9a2 2 0 00-2 2v4h10z" />
          </svg>
        </button>
      </div>

      <!-- Mobile TOC dropdown -->
      <div
        v-if="showMobileToc && headings.length > 1"
        class="lg:hidden bg-black/40 backdrop-blur-md border-b border-white/5 px-4 py-2 space-y-0.5 animate-fade-up-fast"
      >
        <button
          v-for="(h, i) in headings"
          :key="i"
          class="block w-full text-left text-xs py-1 px-2 rounded transition-colors truncate"
          :class="[
            activeHeadingIdx === i ? 'text-accent bg-accent/10' : 'text-white/50 hover:text-white/70',
            h.level === 3 ? 'pl-5' : ''
          ]"
          @click="scrollToHeading(h.id); showMobileToc = false"
        >
          {{ h.text }}
        </button>
      </div>

      <!-- Article body -->
      <article
        ref="articleRef"
        class="article-body px-4 md:px-8 py-6 max-w-prose mx-auto leading-relaxed text-white/90"
        :style="{ fontSize: fontSizes[fontSizeIdx] + 'px' }"
      >
        <h1 v-if="title" class="text-xl font-bold text-white/96 mb-4">{{ title }}</h1>
        <div
          class="article-content [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:text-white/96 [&_h2]:mt-8 [&_h2]:mb-3 [&_h3]:text-base [&_h3]:font-medium [&_h3]:text-white/90 [&_h3]:mt-6 [&_h3]:mb-2 [&_p]:mb-4 [&_ul]:list-disc [&_ul]:ml-5 [&_ul]:mb-4 [&_ol]:list-decimal [&_ol]:ml-5 [&_ol]:mb-4 [&_li]:mb-1 [&_a]:text-accent [&_a]:underline [&_a]:underline-offset-2 [&_blockquote]:border-l-2 [&_blockquote]:border-accent/30 [&_blockquote]:pl-4 [&_blockquote]:italic [&_blockquote]:text-white/70 [&_blockquote]:my-4 [&_code]:bg-white/10 [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-[0.9em] [&_pre]:bg-white/5 [&_pre]:rounded-lg [&_pre]:p-4 [&_pre]:overflow-x-auto [&_pre]:my-4 [&_img]:rounded-lg [&_img]:max-w-full [&_img]:my-4 [&_hr]:border-white/10 [&_hr]:my-6"
          v-html="renderedHtml"
        />
      </article>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import MarkdownIt from 'markdown-it'

const props = defineProps<{
  content: string
  title?: string
}>()

defineEmits<{ back: [] }>()

// Font sizes
const fontSizes = [13, 15, 17, 19, 21]
const savedIdx = localStorage.getItem('aiui-article-font-size')
const fontSizeIdx = ref(savedIdx ? parseInt(savedIdx, 10) : 1)

watch(fontSizeIdx, (v) => {
  localStorage.setItem('aiui-article-font-size', String(v))
})

const showMobileToc = ref(false)

// markdown-it instance
const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
})

// Add IDs to headings for TOC anchoring
md.renderer.rules.heading_open = (tokens, idx, options, _env, self) => {
  const token = tokens[idx]
  const level = parseInt(token.tag.slice(1), 10)
  if (level === 2 || level === 3) {
    const nextToken = tokens[idx + 1]
    const text = nextToken?.children?.reduce((acc, t) => acc + (t.content || ''), '') || ''
    const id = text.toLowerCase().replace(/[^\w]+/g, '-').replace(/(^-|-$)/g, '')
    token.attrSet('id', id)
  }
  return self.renderToken(tokens, idx, options)
}

// Open links in new tab
const defaultLinkOpen = md.renderer.rules.link_open || function (tokens, idx, options, _env, self) {
  return self.renderToken(tokens, idx, options)
}
md.renderer.rules.link_open = function (tokens, idx, options, env, self) {
  tokens[idx].attrSet('target', '_blank')
  tokens[idx].attrSet('rel', 'noopener noreferrer')
  return defaultLinkOpen(tokens, idx, options, env, self)
}

const renderedHtml = computed(() => md.render(props.content))

// Extract headings for TOC
interface Heading {
  text: string
  id: string
  level: number
}

const headings = computed<Heading[]>(() => {
  const result: Heading[] = []
  const re = /^(#{2,3})\s+(.+)$/gm
  let m: RegExpExecArray | null
  while ((m = re.exec(props.content)) !== null) {
    const text = m[2].trim()
    const id = text.toLowerCase().replace(/[^\w]+/g, '-').replace(/(^-|-$)/g, '')
    result.push({ text, id, level: m[1].length })
  }
  return result
})

// Reading time (~200 words/min)
const readingTime = computed(() => {
  const words = props.content.split(/\s+/).length
  return Math.max(1, Math.ceil(words / 200))
})

// Active heading tracking via Intersection Observer
const contentRef = ref<HTMLElement | null>(null)
const articleRef = ref<HTMLElement | null>(null)
const activeHeadingIdx = ref(0)
let observer: IntersectionObserver | null = null

function setupObserver() {
  if (!contentRef.value) return
  observer?.disconnect()
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          const id = (entry.target as HTMLElement).id
          const idx = headings.value.findIndex((h) => h.id === id)
          if (idx >= 0) activeHeadingIdx.value = idx
        }
      }
    },
    { root: contentRef.value, rootMargin: '-20% 0px -60% 0px', threshold: 0 }
  )
  const headingEls = articleRef.value?.querySelectorAll('h2[id], h3[id]')
  headingEls?.forEach((el) => observer!.observe(el))
}

onMounted(() => {
  setTimeout(setupObserver, 100)
})

watch(() => props.content, () => {
  setTimeout(setupObserver, 100)
})

onBeforeUnmount(() => {
  observer?.disconnect()
})

function scrollToHeading(id: string) {
  const el = articleRef.value?.querySelector(`#${CSS.escape(id)}`)
  el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function printArticle() {
  const printWindow = window.open('', '_blank')
  if (!printWindow) return
  printWindow.document.write(`<!DOCTYPE html>
<html><head><title>${props.title || 'Article'}</title>
<style>
  body { font-family: Georgia, serif; max-width: 700px; margin: 2em auto; padding: 0 1em; line-height: 1.7; color: #222; }
  h1 { font-size: 1.8em; margin-bottom: 0.5em; }
  h2 { font-size: 1.4em; margin-top: 1.5em; }
  h3 { font-size: 1.2em; margin-top: 1.2em; }
  code { background: #f0f0f0; padding: 2px 5px; border-radius: 3px; }
  pre { background: #f5f5f5; padding: 1em; overflow-x: auto; border-radius: 5px; }
  blockquote { border-left: 3px solid #ccc; padding-left: 1em; color: #555; }
  img { max-width: 100%; }
  a { color: #0066cc; }
</style></head><body>
${props.title ? `<h1>${props.title}</h1>` : ''}
${renderedHtml.value}
</body></html>`)
  printWindow.document.close()
  printWindow.print()
}
</script>
