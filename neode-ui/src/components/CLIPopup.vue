<template>
  <Teleport to="body">
    <Transition name="cli-popup">
      <div v-if="cliStore.isOpen" class="fixed inset-0 z-[2500] pointer-events-none p-4">
        <section ref="panelRef" class="terminal-panel relative z-10 w-full max-w-5xl overflow-hidden pointer-events-auto" :class="{ minimized }" :style="panelStyle" aria-label="Archipelago terminal">
          <header class="terminal-header" @mousedown="startDrag">
            <div class="terminal-title"><span class="terminal-dot"></span><span>Archipelago terminal</span><span class="terminal-host">{{ activeSession?.name || 'No session' }}</span></div>
            <div class="terminal-actions">
              <select v-model="selectedId" class="terminal-select" aria-label="Terminal session" @change="connectSelected">
                <option value="">Select session</option>
                <option v-for="session in sessions" :key="session.id" :value="session.id">{{ session.name }} · {{ session.state }}</option>
              </select>
              <button class="terminal-button" type="button" @click="createSession">New</button>
              <button class="terminal-button" type="button" aria-label="Refresh terminal" title="Refresh terminal" @click="refreshTerminal">Refresh</button>
              <button class="terminal-button" type="button" :aria-label="minimized ? 'Restore terminal' : 'Minimize terminal'" @click="minimized = !minimized">{{ minimized ? 'Restore' : 'Minimize' }}</button>
              <button class="terminal-button" type="button" @click="toggleFullscreen">{{ fullscreen ? 'Windowed' : 'Fullscreen' }}</button>
              <button class="terminal-close" type="button" aria-label="Close terminal" @click="closeTerminal">Close</button>
            </div>
          </header>
          <div v-if="!minimized" class="terminal-body">
            <div v-if="error" class="terminal-empty terminal-error"><strong>Terminal unavailable</strong><span>{{ error }}</span><button class="terminal-button" type="button" @click="loadSessions">Retry</button></div>
            <div v-else-if="!sessions.length" class="terminal-empty"><strong>Create your first developer session</strong><span>Sessions stay running when this window closes and can be resumed later.</span><button class="terminal-button terminal-primary" type="button" @click="createSession">Start session</button></div>
            <div v-else-if="!selectedId" class="terminal-empty"><strong>Choose a session</strong><span>Resume an existing workspace or start a new one.</span></div>
            <pre v-else ref="outputRef" class="terminal-output" tabindex="0" @click="focusInput">{{ output }}</pre>
          </div>
          <footer v-if="selectedId && !minimized" class="terminal-input-bar">
            <span class="terminal-prompt">›</span><input ref="inputRef" class="terminal-input" aria-label="Terminal input" autocomplete="off" autocapitalize="off" spellcheck="false" @keydown="onKeydown" @blur="keepFocus" />
            <span class="terminal-status" :class="{ connected: connected }">{{ connected ? 'Connected' : 'Reconnecting…' }}</span>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
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
const output = ref('')
const error = ref('')
const connected = ref(false)
const outputRef = ref<HTMLElement | null>(null)
const inputRef = ref<HTMLInputElement | null>(null)
let socket: WebSocket | null = null

const activeSession = computed(() => sessions.value.find(session => session.id === selectedId.value))
const panelStyle = computed(() => ({ transform: `translate(${position.value.x}px, ${position.value.y}px)` }))
const wsUrl = (id: string) => `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws/terminal?session=${encodeURIComponent(id)}`

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
function refreshTerminal() { if (selectedId.value) connectSelected(); else loadSessions() }
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
  disconnect(); output.value = ''
  if (!selectedId.value) return
  socket = new WebSocket(wsUrl(selectedId.value))
  socket.onopen = () => { connected.value = true; focusInput() }
  socket.onclose = () => { connected.value = false }
  socket.onerror = () => { connected.value = false }
  socket.onmessage = event => {
    try { const message = JSON.parse(event.data); if (message.type === 'output') { output.value = message.data; nextTick(() => { if (outputRef.value) outputRef.value.scrollTop = outputRef.value.scrollHeight }) } } catch { /* ignore malformed frames */ }
  }
}

function send(data: string) { if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'input', data })) }
function onKeydown(event: KeyboardEvent) {
  if (event.ctrlKey && event.key.toLowerCase() === 'c') { event.preventDefault(); send('\u0003'); return }
  if (event.ctrlKey && event.key.toLowerCase() === 'd') { event.preventDefault(); send('\u0004'); return }
  const special: Record<string, string> = { Enter: '\n', Backspace: '\u007f', Tab: '\t', ArrowUp: '\u001b[A', ArrowDown: '\u001b[B', ArrowRight: '\u001b[C', ArrowLeft: '\u001b[D' }
  const specialKey = special[event.key]
  if (specialKey !== undefined) { event.preventDefault(); send(specialKey); return }
  if (event.key.length === 1 && !event.metaKey && !event.altKey) { event.preventDefault(); send(event.key) }
}
function focusInput() { nextTick(() => inputRef.value?.focus()) }
function keepFocus() { if (cliStore.isOpen) setTimeout(focusInput, 0) }
watch(() => cliStore.isOpen, open => { if (open) loadSessions(); else disconnect() })
onMounted(() => { window.addEventListener('mousemove', onDrag); window.addEventListener('mouseup', stopDrag) })
onBeforeUnmount(() => { window.removeEventListener('mousemove', onDrag); window.removeEventListener('mouseup', stopDrag) })
onBeforeUnmount(disconnect)
</script>

<style scoped>
.terminal-panel { display: flex; flex-direction: column; width: min(960px, calc(100vw - 2rem)); height: min(680px, calc(100vh - 2rem)); min-width: 320px; min-height: 220px; max-width: calc(100vw - 2rem); max-height: calc(100vh - 2rem); resize: both; border: 1px solid rgba(247,147,26,.28); border-radius: 18px; background: #0b0e12; box-shadow: 0 28px 90px rgba(0,0,0,.65); color: #f6f7f9; }
.terminal-panel.minimized { width: auto; height: auto; min-width: 320px; resize: none; }
.terminal-panel:fullscreen { width: 100vw; height: 100vh; max-width: none; max-height: none; border-radius: 0; }
.terminal-header, .terminal-input-bar { display: flex; align-items: center; gap: 12px; padding: 13px 16px; background: rgba(255,255,255,.045); }
.terminal-header { justify-content: space-between; border-bottom: 1px solid rgba(255,255,255,.1); cursor: grab; user-select: none; }
.terminal-header:active { cursor: grabbing; }
.terminal-title, .terminal-actions { display: flex; align-items: center; gap: 10px; min-width: 0; }
.terminal-title { font-weight: 650; } .terminal-host { color: rgba(255,255,255,.45); font-size: 12px; font-weight: 400; }
.terminal-dot { width: 9px; height: 9px; border-radius: 50%; background: #f7931a; box-shadow: 0 0 12px #f7931a; }
.terminal-body { flex: 1 1 auto; min-height: 0; height: 0; overflow: hidden; background: #050608; }
.terminal-output { box-sizing: border-box; height: 100%; max-height: 100%; margin: 0; overflow-y: auto; overflow-x: hidden; padding: 18px; color: #d7f7d3; font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; overflow-anchor: auto; outline: none; }
.terminal-input-bar { border-top: 1px solid rgba(255,255,255,.1); } .terminal-prompt { color: #f7931a; font-size: 20px; } .terminal-input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: white; font: 13px ui-monospace, monospace; }
.terminal-status { color: #e8a7a7; font-size: 11px; } .terminal-status.connected { color: #92d6a0; }
.terminal-select, .terminal-button, .terminal-close { min-height: 36px; border-radius: 9px; border: 1px solid rgba(255,255,255,.15); background: rgba(255,255,255,.07); color: white; padding: 0 11px; font-size: 12px; }
.terminal-button:hover, .terminal-select:hover { background: rgba(255,255,255,.13); } .terminal-primary { background: #f7931a; border-color: #f7931a; color: #17100a; font-weight: 700; } .terminal-close { color: rgba(255,255,255,.65); }
.terminal-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; height: 100%; min-height: 300px; padding: 32px; text-align: center; color: rgba(255,255,255,.55); } .terminal-empty strong { color: white; font-size: 16px; } .terminal-error strong { color: #ffaaa6; }
@media (max-width: 640px) { .terminal-panel { width: calc(100vw - 1rem); height: calc(100vh - 1rem); max-width: calc(100vw - 1rem); max-height: calc(100vh - 1rem); } .terminal-header { align-items: flex-start; flex-direction: column; } .terminal-actions { width: 100%; flex-wrap: wrap; } .terminal-select { flex: 1; min-width: 0; } .terminal-status { display: none; } }
</style>
