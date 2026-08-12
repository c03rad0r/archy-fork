<template>
  <div class="btc-face-wrap">
    <pre class="btc-face-canvas" v-html="frameHtml"></pre>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'

const frameHtml = ref('')

const W = 74
const H = 38
const CX = W / 2
const CY = H / 2
const RX = 30
const RY = 16

// Bitcoin symbol bitmap (12 wide, 11 tall)
const SYM = [
  '     # #    ',
  '  ########  ',
  ' ###     ## ',
  ' ###    ### ',
  ' ###     ## ',
  ' ########## ',
  ' ###     ## ',
  ' ###    ### ',
  ' ###     ## ',
  '  ########  ',
  '     # #    ',
]
const SYM_W = SYM[0]!.length
const SYM_OX = -Math.floor(SYM_W / 2)
const SYM_OY = -Math.floor(SYM.length / 2) + 1

function isInSym(fx: number, fy: number): boolean {
  const bx = Math.round(fx) - SYM_OX
  const by = Math.round(fy) - SYM_OY
  if (by < 0 || by >= SYM.length || bx < 0 || bx >= SYM_W) return false
  return SYM[by]![bx] === '#'
}

// Eyes
const EYE_Y = -7, EYE_H = 3, EYE_L = -9, EYE_R = 8

function isEye(fx: number, fy: number): boolean {
  const ry = Math.round(fy), rx = Math.round(fx)
  if (rx >= EYE_L - 2 && rx <= EYE_L + 2 && ry >= EYE_Y && ry <= EYE_Y + EYE_H - 1) return true
  if (rx >= EYE_R - 2 && rx <= EYE_R + 2 && ry >= EYE_Y && ry <= EYE_Y + EYE_H - 1) return true
  return false
}

function isPupil(fx: number, fy: number, lookX: number): boolean {
  const ry = Math.round(fy), rx = Math.round(fx)
  const p = Math.round(lookX * 0.8)
  if (rx >= EYE_L + p && rx <= EYE_L + p + 1 && ry >= EYE_Y && ry <= EYE_Y + 1) return true
  if (rx >= EYE_R + p && rx <= EYE_R + p + 1 && ry >= EYE_Y && ry <= EYE_Y + 1) return true
  return false
}

// Mouth
function isMouth(fx: number, fy: number): boolean {
  const ry = Math.round(fy), rx = Math.round(fx)
  if (ry === 9 && rx >= -5 && rx <= 5) return Math.abs(rx) >= 2
  if (ry === 8 && (rx === -5 || rx === 5)) return true
  return false
}

function mouthChar(fx: number): string {
  const rx = Math.round(fx)
  return (rx === -5 || rx === 5) ? "'" : '~'
}

const EDGE = '@#%*+=:. '

function render(f: number): string {
  const bob = Math.sin(f * 0.055) * 1.2
  const breathe = Math.sin(f * 0.028) * 0.4
  const lookX = Math.sin(f * 0.02)
  const blink = (f % 160 >= 152 && f % 160 <= 157) || (f % 480 >= 300 && f % 480 <= 305)

  const lines: string[] = []
  for (let y = 0; y < H; y++) {
    let line = ''
    for (let x = 0; x < W; x++) {
      const fy = y - CY + bob, fx = x - CX
      const rx = RX + breathe * 2, ry = RY + breathe
      const dx = fx / rx, dy = fy / ry
      const dist = Math.sqrt(dx * dx + dy * dy)

      if (dist > 1.15) {
        line += ' '
      } else if (dist > 1.0) {
        const sh = Math.sin(f * 0.08 + Math.atan2(dy, dx) * 3) * 0.5 + 0.5
        const ci = Math.floor(((dist - 1.0) / 0.15) * 6 + sh * 2)
        const c = EDGE[Math.min(ci, EDGE.length - 1)]
        line += c === ' ' ? ' ' : `<span class="af-g">${c}</span>`
      } else if (dist > 0.9) {
        const sh = Math.sin(f * 0.1 + Math.atan2(dy, dx) * 4) * 0.5 + 0.5
        let ci = Math.floor((1 - (dist - 0.9) / 0.1) * 3 + sh * 2)
        ci = Math.max(0, Math.min(ci, 4))
        line += `<span class="af-b">${'@#%*+'[ci]}</span>`
      } else if (dist > 0.85) {
        const sh = Math.sin(f * 0.1 + Math.atan2(dy, dx) * 4 + 1) * 0.5 + 0.5
        line += `<span class="af-b">${'#%'[Math.floor(sh * 2)]}</span>`
      } else if (isEye(fx, fy)) {
        if (blink) line += '<span class="af-b">-</span>'
        else if (isPupil(fx, fy, lookX)) line += '<span class="af-p">@</span>'
        else line += ' '
      } else if (isInSym(fx, fy)) {
        const gl = Math.sin(f * 0.06 + fx * 0.3) * 0.3
        line += `<span class="${gl > 0.1 ? 'af-m' : 'af-b'}">$</span>`
      } else if (isMouth(fx, fy)) {
        line += `<span class="af-b">${mouthChar(fx)}</span>`
      } else {
        const w = Math.sin(f * 0.04 + x * 0.12 + y * 0.08)
        const d = (dist / 0.85) * 2 + w * 0.6
        line += d < 1.2 ? '=' : d < 1.6 ? '+' : d < 2.0 ? ':' : d < 2.3 ? '-' : '.'
      }
    }
    lines.push(line)
  }
  return lines.join('\n')
}

// Animation loop — 24fps (lighter than 30 for embedded use)
let frame = 0, lastTime = -1, running = false, rafId: number | null = null
const FT = 1000 / 24

function tick(ts: number) {
  if (lastTime === -1) lastTime = ts
  let dt = ts - lastTime
  let dirty = false
  while (dt >= FT) { frame++; dt -= FT; lastTime += FT; dirty = true }
  if (dirty) frameHtml.value = render(frame)
  if (running) rafId = requestAnimationFrame(tick)
}

function start() {
  if (!running) { running = true; lastTime = -1; rafId = requestAnimationFrame(tick) }
}

function stop() {
  running = false
  if (rafId) { cancelAnimationFrame(rafId); rafId = null }
}

function onVisChange() {
  if (document.hidden) stop(); else start()
}

onMounted(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    frameHtml.value = render(0)
  } else {
    document.addEventListener('visibilitychange', onVisChange)
    start()
  }
})

onBeforeUnmount(() => {
  stop()
  document.removeEventListener('visibilitychange', onVisChange)
})
</script>

<style scoped>
.btc-face-wrap {
  overflow: hidden;
}

.btc-face-canvas {
  margin: 0;
  padding: 0;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 7px;
  line-height: 1.15;
  white-space: pre;
  color: #6a6a6e;
  letter-spacing: 0.3px;
  user-select: none;
}

.btc-face-canvas :deep(.af-b) { color: #F7931A; }
.btc-face-canvas :deep(.af-g) { color: #c98a20; }
.btc-face-canvas :deep(.af-p) { color: #ffffff; }
.btc-face-canvas :deep(.af-m) { color: #e8a43a; }
.btc-face-canvas :deep(.af-d) { color: #52524e; }
</style>
