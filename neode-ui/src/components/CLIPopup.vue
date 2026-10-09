<template>
  <Teleport to="body">
    <Transition name="cli-popup">
      <div v-if="cliStore.isOpen" class="fixed inset-0 z-[2500] flex items-center justify-start p-4 pointer-events-none">
        <section ref="panelRef" class="glass-card terminal-panel relative z-10 w-full overflow-hidden pointer-events-auto md:ml-[300px]" :class="{ minimized }" :style="panelStyle" aria-label="Archipelago terminal">
          <header class="terminal-header sticky top-0 z-10 flex items-center gap-3 border-b border-white/10 px-4 py-3 bg-black/60 backdrop-blur-md md:bg-transparent md:backdrop-blur-none" @mousedown="startDrag">
            <div class="terminal-drag-handle" title="Drag to move">
              <svg class="w-4 h-4 text-white/50" fill="currentColor" viewBox="0 0 24 24"><path d="M8 6h2v2H8V6zm0 5h2v2H8v-2zm0 5h2v2H8v-2zm5-10h2v2h-2V6zm0 5h2v2h-2v-2zm0 5h2v2h-2v-2z" /></svg>
            </div>
            <div class="terminal-title"><svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" /></svg><span>CLI Access</span><span class="terminal-host">{{ activeSession?.name || 'No session' }}</span></div>
            <div class="terminal-actions">
              <select v-model="selectedId" class="terminal-select" aria-label="Terminal session" @change="connectSelected">
                <option value="">Select session</option>
                <option v-for="session in sessions" :key="session.id" :value="session.id">{{ session.name }} · {{ session.state }}</option>
              </select>
              <button class="terminal-icon-button" type="button" @click="createSession">New</button>
              <button class="terminal-icon-button" type="button" aria-label="Refresh" title="Refresh" @click="refreshTerminal"><svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg></button>
              <button class="terminal-icon-button" type="button" :aria-label="minimized ? 'Restore terminal' : 'Minimize terminal'" @click="minimized = !minimized"><svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="minimized ? 'M5 12h14' : 'M6 12h12'" /></svg></button>
              <button class="terminal-icon-button" type="button" :aria-label="fullscreen ? 'Windowed' : 'Fullscreen'" @click="toggleFullscreen"><svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="fullscreen ? 'M8 3v5H3m18 0h-5V3M3 16h5v5m8 0v-5h5' : 'M8 3H3v5m13-5h5v5M3 16v5h5m8 0h5v-5'" /></svg></button>
              <button class="terminal-icon-button" type="button" aria-label="Close" @click="closeTerminal"><svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg></button>
            </div>
          </header>
          <div v-show="!minimized" class="terminal-body">
            <div v-if="error" class="terminal-empty terminal-error"><strong>Terminal unavailable</strong><span>{{ error }}</span><button class="terminal-button" type="button" @click="loadSessions">Retry</button></div>
            <div v-else-if="!sessions.length" class="terminal-empty"><strong>Create your first developer session</strong><span>Sessions stay running when this window closes and can be resumed later.</span><button class="terminal-button terminal-primary" type="button" @click="createSession">Start session</button></div>
            <div v-else-if="!selectedId" class="terminal-empty"><strong>Choose a session</strong><span>Resume an existing workspace or start a new one.</span></div>
            <div v-else ref="terminalRef" class="terminal-output" tabindex="0" aria-label="Archipelago terminal session" @pointerdown.stop="focusInput" @click.stop="focusInput" @keydown.stop @keyup.stop @keypress.stop></div>
          </div>
          <footer v-if="selectedId && !minimized" class="terminal-input-bar">
            <span class="terminal-status" :class="{ connected: connected }">{{ connected ? 'Connected' : 'Reconnecting…' }}</span>
            <span class="terminal-hint">Click terminal to focus · drag the header to move · resize from the corner</span>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useCLIStore } from '@/stores/cli'

type Session = { id: string; name: string; workspace: string; state: string; updated_at: number }
const cliStore = useCLIStore()
const panelRef = ref<HTMLElement | null>(null)
const minimized = ref(false)
const fullscreen = ref(false)
const position = ref({ x: 0, y: 0 })
const dragging = ref<{ x: number; y: number; px: number; py: number } | null>(null)
const sessions = ref<Session[]>([])
const selectedId = ref('')
const error = ref('')
const connected = ref(false)
const terminalRef = ref<HTMLElement | null>(null)
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null
let resizeObserver: ResizeObserver | null = null
let socket: WebSocket | null = null
let renderedSession = ''
let connectionGeneration = 0

const activeSession = computed(() => sessions.value.find(session => session.id === selectedId.value))
const panelStyle = computed(() => ({ transform: `translate(${position.value.x}px, ${position.value.y}px)` }))
const wsUrl = (id: string) => `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws/terminal?session=${encodeURIComponent(id)}`
function compactSnapshot(data: string) {
  const lines = data.split(/\r?\n/)
  const visible = (line: string) => line.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]/g, '').trim().length > 0
  while (lines.length && !visible(lines[0] ?? '')) lines.shift()
  while (lines.length && !visible(lines[lines.length - 1] ?? '')) lines.pop()
  return lines.join('\n')
}

async function loadSessions() {
  error.value = ''
  try {
    const response = await fetch('/api/terminal/sessions', { credentials: 'include' })
    if (!response.ok) throw new Error(response.status === 401 ? 'Sign in to use the developer terminal.' : `Session service returned ${response.status}.`)
    sessions.value = await response.json()
    if (!selectedId.value && sessions.value[0]) selectedId.value = sessions.value[0].id
    if (selectedId.value) connectSelected()
  } catch (cause) { error.value = cause instanceof Error ? cause.message : 'Could not load terminal sessions.' }
}

async function createSession() {
  error.value = ''
  try {
    const response = await fetch('/api/terminal/sessions', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Archipelago work' }) })
    if (!response.ok) throw new Error('Could not create a developer session.')
    const session = await response.json()
    sessions.value = [session, ...sessions.value]
    selectedId.value = session.id
    connectSelected()
  } catch (cause) { error.value = cause instanceof Error ? cause.message : 'Could not create terminal session.' }
}

function closeTerminal() { disconnect(); cliStore.close() }
function refreshTerminal() { if (selectedId.value) { terminal?.reset(); connectSelected() } else loadSessions() }
function startDrag(event: MouseEvent) {
  if ((event.target as HTMLElement).closest('button, select, input')) return
  dragging.value = { x: event.clientX, y: event.clientY, px: position.value.x, py: position.value.y }
}
function onDrag(event: MouseEvent) {
  if (!dragging.value) return
  position.value = { x: dragging.value.px + event.clientX - dragging.value.x, y: dragging.value.py + event.clientY - dragging.value.y }
}
function stopDrag() { dragging.value = null }
async function toggleFullscreen() {
  if (!panelRef.value) return
  if (document.fullscreenElement === panelRef.value) { await document.exitFullscreen?.(); fullscreen.value = false; return }
  try { await panelRef.value.requestFullscreen?.(); fullscreen.value = true } catch { fullscreen.value = false }
}
function disconnect() { socket?.close(); socket = null; connected.value = false }
function connectSelected() {
  disconnect()
  if (!selectedId.value) return
  if (renderedSession !== selectedId.value) {
    terminal?.reset()
    renderedSession = selectedId.value
  }
  const generation = ++connectionGeneration
  const connection = new WebSocket(wsUrl(selectedId.value))
  socket = connection
  connection.onopen = () => { if (generation !== connectionGeneration) return; connected.value = true; resizeTerminal() }
  connection.onclose = () => { if (generation === connectionGeneration) connected.value = false }
  connection.onerror = () => { if (generation === connectionGeneration) connected.value = false }
  connection.onmessage = event => {
    if (generation !== connectionGeneration) return
    try {
      const message = JSON.parse(event.data)
      if (message.type === 'output') {
        // The daemon sends a complete visible-pane snapshot, not a byte
        // stream. Replace the pane so every prompt does not duplicate the
        // previous screen into xterm scrollback.
        terminal?.reset()
        terminal?.write(compactSnapshot(message.data), () => terminal?.scrollToBottom())
      }
    } catch { /* ignore malformed frames */ }
  }
}

function resizeTerminal() {
  fitAddon?.fit()
  if (socket?.readyState === WebSocket.OPEN && terminal) socket.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }))
}

function send(data: string) { if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'input', data })) }
function focusInput() { nextTick(() => terminal?.focus()) }
watch(() => cliStore.isOpen, open => { if (open) loadSessions(); else disconnect() })
watch(terminalRef, element => {
  if (!element || !terminal || element.querySelector('.xterm')) return
  terminal.open(element)
  fitAddon?.fit()
  resizeObserver?.observe(element)
})
onMounted(() => {
  window.addEventListener('mousemove', onDrag)
  window.addEventListener('mouseup', stopDrag)
  terminal = new Terminal({
    convertEol: true,
    cursorBlink: true,
    cursorInactiveStyle: 'none',
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
    fontSize: 13,
    lineHeight: 1.45,
    scrollback: 10000,
    theme: { background: 'transparent', foreground: '#f3f4f6', cursor: '#f7931a', selectionBackground: 'rgba(247,147,26,.3)' },
  })
  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.onData(data => send(data))
  resizeObserver = new ResizeObserver(() => resizeTerminal())
  if (terminalRef.value) {
    terminal.open(terminalRef.value)
    resizeObserver.observe(terminalRef.value)
  }
  nextTick(resizeTerminal)
})
function blurTerminalWhenOutside(event: PointerEvent) {
  if (panelRef.value && !panelRef.value.contains(event.target as Node)) terminal?.blur()
}
onMounted(() => window.addEventListener('pointerdown', blurTerminalWhenOutside, true))
onBeforeUnmount(() => {
  window.removeEventListener('mousemove', onDrag)
  window.removeEventListener('mouseup', stopDrag)
  window.removeEventListener('pointerdown', blurTerminalWhenOutside, true)
  resizeObserver?.disconnect()
  terminal?.dispose()
  disconnect()
})
</script>

<style scoped>
.terminal-panel { display: flex; flex-direction: column; width: min(960px, calc(100vw - 2rem)); height: min(680px, calc(100vh - 2rem)); min-width: 420px; min-height: 260px; max-width: calc(100vw - 2rem); max-height: calc(100vh - 2rem); resize: both; }
.terminal-panel.minimized { width: auto; height: auto; min-width: 320px; resize: none; }
.terminal-panel:fullscreen { width: 100vw; height: 100vh; max-width: none; max-height: none; border-radius: 0; }
.terminal-header, .terminal-input-bar { display: flex; align-items: center; gap: 12px; padding: 13px 16px; }
.terminal-header { cursor: grab; user-select: none; }
.terminal-header:active { cursor: grabbing; }
.terminal-title, .terminal-actions { display: flex; align-items: center; gap: 10px; min-width: 0; }
.terminal-title { flex: 1; font-weight: 500; } .terminal-host { color: rgba(255,255,255,.45); font-size: 12px; font-weight: 400; }
.terminal-drag-handle { display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: 6px; flex-shrink: 0; }
.terminal-drag-handle:hover { background: rgba(255,255,255,.1); }
.terminal-body { display: flex; flex: 1 1 0; min-height: 0; max-height: 100%; height: 0; overflow: hidden; }
.terminal-output { box-sizing: border-box; flex: 1 1 auto; min-height: 0; min-width: 0; height: 100%; max-height: 100%; margin: 0; overflow: hidden; padding: 14px 16px; color: rgba(255,255,255,.9); outline: none; }
.terminal-output :deep(.xterm) { height: 100%; width: 100%; }
.terminal-output :deep(.xterm-viewport) { background: transparent !important; }
.terminal-output:not(:focus-within) :deep(.xterm-cursor) { visibility: hidden !important; }
.terminal-output:focus-within :deep(.xterm-cursor) { visibility: visible !important; }
.terminal-input-bar { border-top: 1px solid rgba(255,255,255,.1); } .terminal-prompt { color: #f7931a; font-size: 20px; } .terminal-input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: white; font: 13px ui-monospace, monospace; }
.terminal-status { color: rgba(255,255,255,.5); font-size: 11px; } .terminal-status.connected { color: #92d6a0; }
.terminal-hint { margin-left: auto; color: rgba(255,255,255,.35); font-size: 11px; }
.terminal-select, .terminal-icon-button, .terminal-button { min-height: 36px; border-radius: 9px; border: 1px solid rgba(255,255,255,.12); background: rgba(255,255,255,.06); color: white; padding: 0 9px; font-size: 12px; }
.terminal-icon-button:hover, .terminal-button:hover, .terminal-select:hover { background: rgba(255,255,255,.13); }
.terminal-primary { background: #f7931a; border-color: #f7931a; color: #17100a; font-weight: 700; }
.terminal-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; height: 100%; min-height: 300px; padding: 32px; text-align: center; color: rgba(255,255,255,.55); } .terminal-empty strong { color: white; font-size: 16px; } .terminal-error strong { color: #ffaaa6; }
@media (max-width: 640px) { .terminal-panel { width: calc(100vw - 1rem); height: calc(100vh - 1rem); max-width: calc(100vw - 1rem); max-height: calc(100vh - 1rem); } .terminal-header { align-items: flex-start; flex-direction: column; } .terminal-actions { width: 100%; flex-wrap: wrap; } .terminal-select { flex: 1; min-width: 0; } .terminal-status { display: none; } }
</style>
