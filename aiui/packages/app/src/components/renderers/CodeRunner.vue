<template>
  <div class="code-runner rounded-xl bg-white/5 border border-white/10 overflow-hidden">
    <!-- Toolbar -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-white/5">
      <span class="text-xs text-white/30 uppercase tracking-wider">{{ language }}</span>
      <div class="flex-1" />
      <button
        class="text-xs px-2.5 py-1 rounded bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
        @click="runCode"
      >
        Run
      </button>
      <button
        v-if="consoleOutput.length > 0"
        class="text-xs px-2 py-1 rounded bg-white/5 text-white/40 hover:text-white/60 hover:bg-white/10 transition-colors"
        @click="clearOutput"
      >
        Clear
      </button>
    </div>

    <!-- Code display -->
    <pre class="px-3 py-2 text-xs text-white/70 overflow-x-auto max-h-48 bg-black/20"><code>{{ code }}</code></pre>

    <!-- Output iframe (hidden, used for execution) -->
    <iframe
      v-if="showIframe"
      ref="iframeRef"
      :srcdoc="srcdoc"
      sandbox="allow-scripts"
      class="w-full border-t border-white/5"
      :class="isHtml ? 'h-48' : 'h-0 invisible'"
      title="Code output"
    />

    <!-- Console output -->
    <div
      v-if="consoleOutput.length > 0"
      class="border-t border-white/5 bg-black/30 px-3 py-2 max-h-32 overflow-y-auto"
    >
      <p class="text-xs text-white/20 uppercase tracking-wider mb-1">Console</p>
      <div
        v-for="(entry, i) in consoleOutput"
        :key="i"
        class="text-xs font-mono leading-relaxed"
        :class="entry.type === 'error' ? 'text-red-400/80' : 'text-white/60'"
      >
        {{ entry.text }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps<{
  code: string
  language: string
}>()

interface ConsoleEntry {
  type: 'log' | 'error'
  text: string
}

const consoleOutput = ref<ConsoleEntry[]>([])
const showIframe = ref(false)
const iframeRef = ref<HTMLIFrameElement | null>(null)

const isHtml = computed(() => props.language === 'html')

const srcdoc = computed(() => buildSrcdoc(props.language, props.code))

function buildSrcdoc(lang: string, code: string): string {
  // Build script tags via concatenation to avoid parser issues
  const sOpen = String.fromCharCode(60) + 'script>'
  const sClose = String.fromCharCode(60) + '/script>'
  const capture = sOpen
    + 'const _post=(t,a)=>parent.postMessage({type:"code-runner-console",level:t,text:a.map(x=>typeof x==="object"?JSON.stringify(x):String(x)).join(" ")},"*");'
    + 'console.log=(...a)=>_post("log",a);'
    + 'console.error=(...a)=>_post("error",a);'
    + 'window.onerror=(m)=>_post("error",[m]);'
    + sClose

  if (lang === 'html') {
    return '<!DOCTYPE html><html><head><style>body{background:#0a0a0a;color:#e0e0e0;font-family:system-ui;margin:8px}</style>' + capture + '</head><body>' + code + '</body></html>'
  }
  if (lang === 'javascript' || lang === 'js') {
    return '<!DOCTYPE html><html><head>' + capture + '</head><body>' + sOpen + code + sClose + '</body></html>'
  }
  if (lang === 'css') {
    return '<!DOCTYPE html><html><head><style>' + code + '</style></head><body style="background:#0a0a0a"><div style="padding:16px;color:#e0e0e0;font-family:system-ui">CSS Preview</div></body></html>'
  }
  return ''
}

function runCode() {
  consoleOutput.value = []
  showIframe.value = false
  // Force re-mount iframe
  requestAnimationFrame(() => {
    showIframe.value = true
  })
}

function clearOutput() {
  consoleOutput.value = []
}

function handleMessage(e: MessageEvent) {
  if (e.data?.type === 'code-runner-console') {
    consoleOutput.value.push({
      type: e.data.level === 'error' ? 'error' : 'log',
      text: e.data.text,
    })
  }
}

onMounted(() => {
  window.addEventListener('message', handleMessage)
})

onBeforeUnmount(() => {
  window.removeEventListener('message', handleMessage)
})
</script>
