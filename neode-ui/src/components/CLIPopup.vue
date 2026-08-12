<template>
  <Teleport to="body">
    <Transition name="cli-popup">
      <div
        v-if="cliStore.isOpen"
        class="fixed inset-0 z-[2500] flex items-center justify-center p-4"
        @click.self="cliStore.close()"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div
          ref="panelRef"
          class="glass-card w-full max-w-2xl relative z-10 overflow-hidden flex flex-col"
          :style="panelStyle"
          @mousedown="onPanelMouseDown"
        >
          <!-- Header: terminal icon + title + app switcher -->
          <div class="flex items-center gap-3 px-4 py-3 border-b border-white/10">
            <div
              ref="dragHandleRef"
              class="flex items-center justify-center w-8 h-8 rounded cursor-grab hover:bg-white/10 transition-colors shrink-0"
              :class="{ 'cursor-grabbing': isDragging }"
              title="Drag to move"
            >
              <svg class="w-4 h-4 text-white/50" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 6h2v2H8V6zm0 5h2v2H8v-2zm0 5h2v2H8v-2zm5-10h2v2h-2V6zm0 5h2v2h-2v-2zm0 5h2v2h-2v-2z" />
              </svg>
            </div>
            <div class="flex items-center gap-3 flex-1 min-w-0">
              <svg class="w-5 h-5 text-white/60 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
              <span class="text-white font-medium">CLI Access</span>
            </div>
            <kbd class="hidden sm:inline-flex px-2 py-1 text-xs text-white/50 bg-white/10 rounded">Esc</kbd>
          </div>

          <!-- Content: mock CLI in dev, SSH instructions in production -->
          <div class="flex-1 overflow-hidden flex flex-col min-h-0">
            <!-- Mock CLI interface (dev mode only) -->
            <div
              v-if="isDev"
              class="flex-1 flex flex-col min-h-0 p-4 bg-black/80 rounded-b-lg font-mono text-sm"
            >
              <div ref="outputRef" class="flex-1 overflow-y-auto text-green-400/90 whitespace-pre-wrap break-words mb-2 min-h-0">{{ mockOutput }}</div>
              <div class="flex items-center gap-2 shrink-0">
                <span class="text-amber-400">archipelago@node</span>
                <span class="text-white/60">~</span>
                <span class="text-white/40">$</span>
                <input
                  ref="cliInputRef"
                  v-model="mockCommand"
                  type="text"
                  class="flex-1 bg-transparent text-white outline-none border-none"
                  placeholder=" "
                  @keydown.enter="runMockCommand"
                />
              </div>
            </div>

            <!-- SSH instructions (production only) -->
            <div v-else class="flex-1 overflow-y-auto p-4 space-y-4">
              <p class="text-white/80 text-sm">
                Connect to this node via SSH to access the command line. Use the same host as this web interface.
              </p>

              <div class="space-y-3">
                <div class="p-3 rounded-lg bg-white/5 font-mono text-sm">
                  <div class="text-white/50 text-xs uppercase tracking-wider mb-2">SSH Command</div>
                  <div class="flex items-center gap-2 flex-wrap">
                    <code class="text-green-400 break-all">{{ sshCommand }}</code>
                    <button
                      type="button"
                      class="shrink-0 px-2 py-1 rounded bg-white/10 text-white/80 hover:bg-white/20 hover:text-white text-xs transition-colors"
                      @click="copyCommand"
                    >
                      {{ copied ? 'Copied!' : 'Copy' }}
                    </button>
                  </div>
                </div>

                <div class="p-3 rounded-lg bg-white/5 text-sm space-y-1">
                  <div class="text-white/50 text-xs uppercase tracking-wider mb-2">Connection Details</div>
                  <div class="flex flex-col gap-1.5 text-white/80">
                    <div class="flex justify-between gap-4">
                      <span class="text-white/50">Host</span>
                      <span class="font-mono text-green-400">{{ host }}</span>
                    </div>
                    <div class="flex justify-between gap-4">
                      <span class="text-white/50">User</span>
                      <span class="font-mono">archipelago</span>
                    </div>
                    <div class="flex justify-between gap-4">
                      <span class="text-white/50">Password</span>
                      <span class="font-mono">archipelago</span>
                    </div>
                  </div>
                </div>

                <p class="text-white/50 text-xs">
                  From the terminal menu you can install to disk, configure Bitcoin, Lightning, view logs, and more.
                </p>
                <p class="text-white/40 text-xs">
                  Tip: Press <kbd class="px-1.5 py-0.5 rounded bg-white/10 font-mono text-[10px]">F</kbd> to open this anytime.
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { useCLIStore } from '@/stores/cli'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
const cliStore = useCLIStore()
const panelRef = ref<HTMLElement | null>(null)
const dragHandleRef = ref<HTMLElement | null>(null)
const outputRef = ref<HTMLElement | null>(null)
const cliInputRef = ref<HTMLInputElement | null>(null)
const copied = ref(false)

const mockCommand = ref('')
const mockOutput = ref(`  ╔═══════════════════════════════════════════════════════════╗
  ║   🏝️  ARCHIPELAGO BITCOIN NODE OS                         ║
  ║   Your sovereign Bitcoin infrastructure                   ║
  ╚═══════════════════════════════════════════════════════════╝

  System Status:
  ─────────────────────────────────────────────────────────────
  Mode:        🟢 Installed
  Podman:      🟢 Installed
  Bitcoin:     🟢 Running (blocks: syncing)
  Lightning:   🟡 Stopped

  Main Menu:
  ─────────────────────────────────────────────────────────────
  r) Refresh                - Update IP/status
  w) Open Web UI            - Launch graphical interface
  1) Install to Disk        - Permanently install Archipelago
  2) Setup Bitcoin Core     - Configure Bitcoin full node
  3) Setup Lightning (LND)  - Configure Lightning Network
  4) Setup BTCPay Server    - Bitcoin payment processor
  5) View Logs              - Monitor running services
  6) Network Settings       - Configure networking
  7) System Info            - View system information
  q) Quit

`)

const isDragging = ref(false)
const dragStart = ref<{ x: number; y: number; panelX: number; panelY: number } | null>(null)

const SAVED_POSITION_KEY = 'archipelago-cli-position'
const savedPosition = ref<{ x: number; y: number } | null>(null)

const isDev = import.meta.env.DEV
const host = computed(() => window.location.hostname)
const sshCommand = computed(() => `ssh archipelago@${host.value}`)

const panelStyle = computed(() => {
  const pos = savedPosition.value
  if (!pos) return {}
  return {
    transform: `translate(${pos.x}px, ${pos.y}px)`,
    margin: 0,
  }
})

function loadSavedPosition() {
  try {
    const raw = localStorage.getItem(SAVED_POSITION_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      savedPosition.value = { x: parsed.x ?? 0, y: parsed.y ?? 0 }
    } else {
      savedPosition.value = null
    }
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Failed to load saved CLI position', e)
    savedPosition.value = null
  }
}

function savePosition(x: number, y: number) {
  savedPosition.value = { x, y }
  try {
    localStorage.setItem(SAVED_POSITION_KEY, JSON.stringify({ x, y }))
  } catch (e) {
    if (import.meta.env.DEV) console.warn('Failed to save CLI position', e)
  }
}

function runMockCommand() {
  const cmd = mockCommand.value.trim()
  if (!cmd) return
  mockOutput.value += `\n  archipelago@node ~ $ ${cmd}\n`
  const lower = cmd.toLowerCase()
  if (lower === 'r' || lower === 'refresh') {
    mockOutput.value += `  Status refreshed.\n`
  } else if (lower === 'w' || lower.startsWith('web')) {
    mockOutput.value += `  Opening Web UI... (press C to return to CLI)\n`
  } else if (lower === 'q' || lower === 'quit' || lower === 'exit') {
    mockOutput.value += `  Goodbye! 🏝️\n`
    cliStore.close()
  } else if (lower === 'help' || lower === '?') {
    mockOutput.value += `  Type r, w, 1-7, or q. Press C to switch to Web UI.\n`
  } else {
    mockOutput.value += `  Unknown command. Type 'help' or 'r' for menu.\n`
  }
  mockCommand.value = ''
  nextTick(() => {
    outputRef.value?.scrollTo({ top: outputRef.value.scrollHeight, behavior: 'smooth' })
  })
}

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(sshCommand.value)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch {
    // Fallback for older browsers
    const textarea = document.createElement('textarea')
    textarea.value = sshCommand.value
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
  }
}

function onPanelMouseDown(e: MouseEvent) {
  if (!dragHandleRef.value?.contains(e.target as Node)) return
  isDragging.value = true
  const rect = panelRef.value?.getBoundingClientRect()
  if (!rect) return
  const currentX = savedPosition.value?.x ?? 0
  const currentY = savedPosition.value?.y ?? 0
  dragStart.value = { x: e.clientX, y: e.clientY, panelX: currentX, panelY: currentY }
}

function onMouseMove(e: MouseEvent) {
  if (!dragStart.value) return
  const dx = e.clientX - dragStart.value.x
  const dy = e.clientY - dragStart.value.y
  savePosition(dragStart.value.panelX + dx, dragStart.value.panelY + dy)
}

function onMouseUp() {
  isDragging.value = false
  dragStart.value = null
}

useModalKeyboard(panelRef, computed(() => cliStore.isOpen), () => cliStore.close())

watch(
  () => cliStore.isOpen,
  (open) => {
    if (open) {
      loadSavedPosition()
      if (isDev) {
        nextTick(() => cliInputRef.value?.focus())
      }
    }
  }
)

onMounted(() => {
  loadSavedPosition()
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
})

onBeforeUnmount(() => {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
})
</script>

<style scoped>
.cli-popup-enter-active,
.cli-popup-leave-active {
  transition: opacity 0.2s ease;
}
.cli-popup-enter-from,
.cli-popup-leave-to {
  opacity: 0;
}
</style>
