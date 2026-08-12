<template>
  <Transition name="boot-fade">
    <div v-if="visible" class="boot-screen">
      <!-- Particle starfield -->
      <canvas ref="canvasRef" class="boot-stars" />

      <!-- Two-column layout: terminal left, orb right -->
      <div class="boot-layout">
        <!-- Left: Terminal log -->
        <div class="boot-left">
          <div class="boot-terminal" ref="terminalRef">
            <p v-for="(line, i) in logLines" :key="i" class="boot-log-line" :class="line.type">
              <span class="boot-log-ts">{{ line.prefix }}</span>
              <span>{{ line.text }}</span>
            </p>
            <span class="boot-cursor">_</span>
          </div>
          <div class="boot-progress-wrap">
            <svg class="boot-arc" viewBox="0 0 200 12" preserveAspectRatio="none">
              <rect x="0" y="4" width="200" height="4" rx="2" fill="rgba(255,255,255,0.06)" />
              <rect x="0" y="4" :width="progress * 2" height="4" rx="2" fill="url(#boot-grad)" />
              <defs>
                <linearGradient id="boot-grad" x1="0" y1="0" x2="200" y2="0" gradientUnits="userSpaceOnUse">
                  <stop offset="0" stop-color="rgba(255,255,255,0.5)" />
                  <stop offset="1" stop-color="rgba(255,255,255,0.9)" />
                </linearGradient>
              </defs>
            </svg>
            <span class="boot-pct">{{ Math.round(progress) }}%</span>
          </div>
        </div>

        <!-- Right: The orb -->
        <div class="boot-right">
          <div class="boot-orb">
            <!-- Viz ring segments -->
            <div class="boot-viz-ring">
              <div
                v-for="(_, i) in 48"
                :key="i"
                class="boot-viz-seg"
                :style="{ '--si': i, '--sd': `${(i / 48) * 360}deg` }"
                :class="{ 'boot-seg-lit': i < litBars }"
              />
            </div>

            <!-- Center: gradient-bordered frame with cycling icons -->
            <div class="boot-center-icon">
              <div class="boot-icon-frame boot-gradient-border">
                <Transition name="icon-morph" mode="out-in">
                  <div :key="currentIcon" class="boot-pixel-wrap" :class="{ 'boot-glitch': glitching }">
                    <img :src="iconSources[currentIcon]" class="boot-icon-img" />
                  </div>
                </Transition>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ ready: [] }>()

const canvasRef = ref<HTMLCanvasElement | null>(null)
const terminalRef = ref<HTMLElement | null>(null)

const currentIcon = ref(0)
const progress = ref(0)
const litBars = ref(0)
const glitching = ref(false)

// Boot screen icons — from /assets/icon/ directory
const iconSources = [
  '/assets/icon/bitcoin.svg',
  '/assets/icon/cloud-done.svg',
]

interface LogLine { prefix: string; text: string; type: string }
const logLines = ref<LogLine[]>([])

const bootMessages = [
  { delay: 500, prefix: 'sys', text: 'Archipelago v0.1.0', type: 'info' },
  { delay: 1500, prefix: 'sec', text: 'Loading ed25519 keys...', type: 'info' },
  { delay: 3000, prefix: ' ok', text: 'Cryptographic keys loaded', type: 'success' },
  { delay: 4500, prefix: 'net', text: 'Binding to port 5678', type: 'info' },
  { delay: 5500, prefix: ' ok', text: 'Nginx proxy detected', type: 'success' },
  { delay: 7000, prefix: ' id', text: 'Initializing identity store...', type: 'info' },
  { delay: 8500, prefix: ' ok', text: 'DID resolver online', type: 'success' },
  { delay: 10000, prefix: 'btc', text: 'Connecting to Bitcoin node...', type: 'info' },
  { delay: 12000, prefix: 'lnd', text: 'Lightning daemon syncing', type: 'info' },
  { delay: 14000, prefix: ' ok', text: 'LND chain synced', type: 'success' },
  { delay: 16000, prefix: 'pod', text: 'Scanning containers...', type: 'info' },
  { delay: 17500, prefix: ' ok', text: '12 containers discovered', type: 'success' },
  { delay: 19000, prefix: 'sec', text: 'AppArmor profiles verified', type: 'success' },
  { delay: 20500, prefix: 'dwn', text: 'DWN node connected', type: 'success' },
  { delay: 22000, prefix: 'msh', text: 'Mesh radio initialized', type: 'success' },
  { delay: 23500, prefix: '***', text: 'ALL SYSTEMS OPERATIONAL', type: 'ready' },
]

// Starfield
let animFrame = 0
const stars: { x: number; y: number; z: number }[] = []

function initStars(c: HTMLCanvasElement) {
  for (let i = 0; i < 180; i++) {
    stars.push({ x: (Math.random() - 0.5) * c.width * 3, y: (Math.random() - 0.5) * c.height * 3, z: Math.random() * 1500 + 500 })
  }
}

function drawStars(c: HTMLCanvasElement, ctx: CanvasRenderingContext2D) {
  ctx.fillStyle = '#0a0a0a'
  ctx.fillRect(0, 0, c.width, c.height)
  const speed = 0.6 + (progress.value / 100) * 2.5
  const cx = c.width / 2, cy = c.height / 2
  for (const s of stars) {
    s.z -= speed
    if (s.z <= 0) { s.z = 1500; s.x = (Math.random() - 0.5) * c.width * 3; s.y = (Math.random() - 0.5) * c.height * 3 }
    const sx = (s.x / s.z) * 300 + cx, sy = (s.y / s.z) * 300 + cy
    if (sx < 0 || sx > c.width || sy < 0 || sy > c.height) continue
    const size = Math.max(0.5, (1 - s.z / 1500) * 2)
    const alpha = Math.min(1, (1 - s.z / 1500) * 1.2)
    ctx.beginPath(); ctx.arc(sx, sy, size, 0, Math.PI * 2)
    ctx.fillStyle = `rgba(255,255,255,${alpha * 0.7})`; ctx.fill()
  }
  animFrame = requestAnimationFrame(() => drawStars(c, ctx))
}

function triggerGlitch() { glitching.value = true; setTimeout(() => { glitching.value = false }, 200) }

// Health check
async function checkHealth(): Promise<boolean> {
  try {
    const ac = new AbortController()
    const t = setTimeout(() => ac.abort(), 3000)
    const res = await fetch('/rpc/v1', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'server.echo', params: { message: 'boot' } }),
      signal: ac.signal,
    })
    clearTimeout(t)
    return res.status !== 502 && res.status !== 503
  } catch { return false }
}

let iconInterval: ReturnType<typeof setInterval> | null = null
let pollInterval: ReturnType<typeof setInterval> | null = null
let logTimeouts: ReturnType<typeof setTimeout>[] = []

function startPolling() {
  iconInterval = setInterval(() => {
    currentIcon.value = (currentIcon.value + 1) % iconSources.length
    triggerGlitch()
  }, 2500)

  // Feed boot log messages
  const lastMsgDelay = bootMessages[bootMessages.length - 1]!.delay
  for (const msg of bootMessages) {
    logTimeouts.push(setTimeout(() => {
      logLines.value.push({ prefix: msg.prefix, text: msg.text, type: msg.type })
      if (logLines.value.length > 8) logLines.value.shift()
      const idx = bootMessages.indexOf(msg)
      progress.value = Math.min(95, ((idx + 1) / bootMessages.length) * 100)
      litBars.value = Math.round((progress.value / 100) * 48)
      nextTick(() => { if (terminalRef.value) terminalRef.value.scrollTop = terminalRef.value.scrollHeight })
    }, msg.delay))
  }

  // After the last message, poll for server readiness then immediately transition
  let finished = false
  logTimeouts.push(setTimeout(() => {
    const finishBoot = () => {
      if (finished) return
      finished = true
      stopPolling()
      progress.value = 100
      litBars.value = 48
      if (import.meta.env.DEV) console.log('[Boot] finishBoot — emitting ready in 800ms')
      setTimeout(() => {
        if (import.meta.env.DEV) console.log('[Boot] emitting ready now')
        emit('ready')
      }, 800)
    }

    checkHealth().then(r => {
      if (import.meta.env.DEV) console.log('[Boot] health check result:', r)
      if (r) { finishBoot(); return }
      pollInterval = setInterval(async () => {
        const healthy = await checkHealth()
        if (import.meta.env.DEV) console.log('[Boot] poll health:', healthy)
        if (healthy) finishBoot()
      }, 2000)
    })
  }, lastMsgDelay + 1500))

  // Reset mock boot timer on fresh page load
  fetch('/rpc/v1', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 0, method: 'server.echo', params: { message: 'boot-reset' } }),
  }).catch(() => {})
}

function stopPolling() {
  if (iconInterval) { clearInterval(iconInterval); iconInterval = null }
  if (pollInterval) { clearInterval(pollInterval); pollInterval = null }
  for (const t of logTimeouts) clearTimeout(t)
  logTimeouts = []
}

function initCanvas() {
  const c = canvasRef.value
  if (!c) return
  c.width = window.innerWidth; c.height = window.innerHeight
  const ctx = c.getContext('2d')
  if (ctx) { initStars(c); drawStars(c, ctx) }
}

let started = false
function startIfNeeded() {
  if (started) return
  started = true
  startPolling()
  nextTick(initCanvas)
}
watch(() => props.visible, v => { if (v) startIfNeeded() })
onMounted(() => { if (props.visible) startIfNeeded() })
onBeforeUnmount(() => { stopPolling(); cancelAnimationFrame(animFrame) })
</script>

<style scoped>
.boot-screen {
  position: fixed; inset: 0; z-index: 9000;
  display: flex; align-items: center; justify-content: center;
  cursor: default; overflow: hidden;
}
.boot-stars { position: absolute; inset: 0; width: 100%; height: 100%; }

/* Two-column layout */
.boot-layout {
  position: relative; z-index: 1;
  display: flex; align-items: center; gap: 3rem;
  max-width: 900px; width: 90%; padding: 0 1rem;
}

/* Left column: terminal */
.boot-left {
  flex: 1; min-width: 0; max-width: 400px;
}

.boot-terminal {
  max-height: 200px; overflow: hidden;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 11px; line-height: 1.8;
  mask-image: linear-gradient(to bottom, transparent 0%, black 15%, black 100%);
  -webkit-mask-image: linear-gradient(to bottom, transparent 0%, black 15%, black 100%);
}
.boot-log-line { white-space: nowrap; overflow: hidden; animation: log-in 0.3s ease both; }
.boot-log-line.info { color: rgba(255,255,255,0.35); }
.boot-log-line.success { color: rgba(255,255,255,0.7); }
.boot-log-line.ready { color: white; font-weight: 600; text-shadow: 0 0 10px rgba(255,255,255,0.4); }
.boot-log-ts { color: rgba(255,255,255,0.15); margin-right: 8px; font-weight: 500; }
.boot-log-line.success .boot-log-ts { color: rgba(255,255,255,0.35); }
.boot-log-line.ready .boot-log-ts { color: rgba(255,255,255,0.5); }
@keyframes log-in { from { opacity:0; transform:translateY(6px); } to { opacity:1; transform:translateY(0); } }

.boot-cursor { color: rgba(255,255,255,0.5); animation: blink 1s step-end infinite; font-family: monospace; font-size: 12px; }
@keyframes blink { 50% { opacity: 0; } }

.boot-progress-wrap { display: flex; align-items: center; gap: 10px; margin-top: 12px; }
.boot-arc { flex: 1; height: 12px; }
.boot-pct { font-family: 'SF Mono', monospace; font-size: 10px; color: rgba(255,255,255,0.3); min-width: 28px; text-align: right; }

/* Right column: orb */
.boot-right {
  flex-shrink: 0; display: flex; flex-direction: column; align-items: center; gap: 1.5rem;
}

.boot-orb {
  position: relative; width: 220px; height: 220px;
}
@media (min-width: 640px) { .boot-orb { width: 280px; height: 280px; } }
@media (min-width: 768px) { .boot-orb { width: 320px; height: 320px; } }

/* Viz ring */
.boot-viz-ring { position: absolute; inset: 0; --vr: 100px; }
@media (min-width: 640px) { .boot-viz-ring { --vr: 130px; } }
@media (min-width: 768px) { .boot-viz-ring { --vr: 150px; } }

.boot-viz-seg {
  position: absolute; left: 50%; top: 50%;
  width: 3px; height: 18px; margin-left: -1.5px; margin-top: -9px;
  border-radius: 1.5px; transform-origin: center center;
  transform: rotate(var(--sd)) translateY(calc(-1 * var(--vr)));
  background: rgba(255,255,255,0.05);
  transition: background 0.4s ease, height 0.4s ease, box-shadow 0.4s ease;
}
.boot-seg-lit {
  background: linear-gradient(to bottom, rgba(255,255,255,0.7), rgba(255,255,255,0.2));
  box-shadow: 0 0 6px rgba(255,255,255,0.15);
  height: 22px; margin-top: -11px;
}

/* Center icon */
.boot-center-icon {
  position: absolute; left: 50%; top: 50%; transform: translate(-50%,-50%); z-index: 10;
  filter: drop-shadow(0 0 30px rgba(255,255,255,0.1));
}
.boot-icon-frame {
  width: 120px; height: 120px;
  display: flex; align-items: center; justify-content: center;
}
@media (min-width: 640px) { .boot-icon-frame { width: 160px; height: 160px; } }
@media (min-width: 768px) { .boot-icon-frame { width: 200px; height: 200px; } }

/* Gradient border — circular, matches logo-gradient-border style */
.boot-gradient-border {
  position: relative;
  border-radius: 9999px;
  padding: 3px;
  background: linear-gradient(135deg, rgba(255,255,255,0.6) 0%, rgba(0,0,0,0.8) 100%);
  box-shadow: 0 8px 24px rgba(0,0,0,0.5);
}
.boot-gradient-border::after {
  content: '';
  position: absolute;
  inset: 3px;
  border-radius: 9999px;
  background: #000;
  z-index: 0;
}

.boot-pixel-wrap {
  width: 100%; height: 100%;
  display: flex; align-items: center; justify-content: center;
  position: relative; z-index: 1;
}

.boot-icon-img {
  width: 55%; height: 55%; object-fit: contain;
  filter: brightness(0) invert(1) drop-shadow(0 0 8px rgba(255,255,255,0.15));
}

/* Glitch */
.boot-glitch { animation: glitch 0.2s steps(3) both; }
@keyframes glitch {
  0% { transform: translate(0); filter: none; }
  25% { transform: translate(2px,-1px); filter: hue-rotate(90deg); }
  50% { transform: translate(-2px,1px); filter: hue-rotate(-90deg) brightness(1.4); }
  75% { transform: translate(1px,2px); filter: hue-rotate(45deg); }
  100% { transform: translate(0); filter: none; }
}

/* Icon morph */
.icon-morph-enter-active { transition: opacity 0.3s ease, transform 0.3s ease, filter 0.3s ease; }
.icon-morph-leave-active { transition: opacity 0.2s ease, transform 0.2s ease, filter 0.2s ease; }
.icon-morph-enter-from { opacity:0; transform: scale(0.5) rotate(-10deg); filter: blur(4px); }
.icon-morph-leave-to { opacity:0; transform: scale(1.4) rotate(10deg); filter: blur(4px); }

/* Boot screen fade out */
.boot-fade-leave-active { transition: opacity 1.2s ease; }
.boot-fade-leave-to { opacity: 0; }

/* Mobile: stack vertically */
@media (max-width: 767px) {
  .boot-layout { flex-direction: column-reverse; gap: 2rem; }
  .boot-left { max-width: 100%; }
  .boot-orb { width: 200px; height: 200px; }
}
</style>
