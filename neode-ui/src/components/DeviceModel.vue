<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { createDeviceScene } from '@/graphics/deviceScene'
import { deviceSpecs } from '@/graphics/deviceModels'

const props = withDefaults(defineProps<{
  model: string
  label: string
  fallback?: string
  fillViewport?: boolean
  glassContainer?: boolean
  controls?: boolean
  /** Development-only gallery controls. Never rendered in production builds. */
  devBrowse?: boolean
}>(), { fallback: '', fillViewport: false, glassContainer: false, controls: true, devBrowse: false })
const host = ref<HTMLElement>()
const frame = ref<HTMLElement>()
const ready = ref(false)
const failed = ref(false)
const paused = ref(false)
const reducedMotion = ref(false)
const currentModel = ref(props.model)
const devModelIds = Object.keys(deviceSpecs)
const devBrowseEnabled = import.meta.env.DEV && props.devBrowse
const dragging = ref(false)
const remainingHeight = ref('360px')
let view: ReturnType<typeof createDeviceScene> | undefined
let observer: IntersectionObserver | undefined
let resizeObserver: ResizeObserver | undefined
let motion: MediaQueryList | undefined
let visible = false
let mounted = false
let generation = 0
let raf = 0
let previous = 0
let lastDraw = 0
let pointer: number | undefined
let lastX = 0
let lastY = 0

function devModelLabel(id: string) { return deviceSpecs[id]?.name ?? id }
function browseModel(offset: number) {
  if (!devBrowseEnabled || !view) return
  const index = devModelIds.indexOf(currentModel.value)
  const next = devModelIds[(index + offset + devModelIds.length) % devModelIds.length]!
  currentModel.value = next
  view.setModel(next)
  view.render()
}

function stop() { cancelAnimationFrame(raf); raf = 0; previous = 0; lastDraw = 0 }
function canSpin() { return view && visible && !document.hidden && !paused.value && !motion?.matches && !dragging.value }
function animate(now: number) {
  raf = 0
  if (!canSpin()) { previous = 0; return }
  // Cap to 30fps; complete a turn in about a minute, independent of refresh rate.
  if (now - lastDraw >= 32) {
    view!.rotation.y += previous ? Math.min((now - previous) / 1000, .1) * .105 : 0
    previous = now; lastDraw = now; view!.render()
  }
  raf = requestAnimationFrame(animate)
}
function sync() { reducedMotion.value = !!motion?.matches; stop(); if (canSpin()) raf = requestAnimationFrame(animate) }
function layout() {
  if (props.fillViewport && frame.value) {
    // Use the layout position, not its changing scrolled screen position.
    let scroll = 0
    for (let el = frame.value.parentElement; el; el = el.parentElement) scroll += el.scrollTop
    const top = frame.value.getBoundingClientRect().top + scroll
    // Keep a visible page gutter below the showcase; the frame sits inside a
    // glass card whose bottom margin must remain visible in the viewport.
    remainingHeight.value = `${Math.max(260, window.innerHeight - top - 88)}px`
  }
  view?.resize()
}
function disposeScene() {
  stop(); ready.value = false
  view?.canvas.removeEventListener('webglcontextlost', contextLost)
  view?.dispose(); view = undefined
}
function contextLost(event: Event) {
  event.preventDefault(); failed.value = true; disposeScene()
}
async function init() {
  if (!mounted || !visible || view || failed.value) return
  const token = ++generation
  try {
    const { createDeviceScene } = await import('@/graphics/deviceScene')
    if (!mounted || token !== generation || !host.value) return
    view = createDeviceScene(host.value, props.model)
    view.canvas.addEventListener('webglcontextlost', contextLost)
    ready.value = true; layout(); sync()
  } catch {
    if (token === generation) { failed.value = true; disposeScene() }
  }
}
function down(event: PointerEvent) {
  if (!view || (event.pointerType === 'mouse' && event.button !== 0) || pointer !== undefined) return
  pointer = event.pointerId; lastX = event.clientX; lastY = event.clientY
  dragging.value = true; stop(); host.value?.setPointerCapture(pointer)
}
function move(event: PointerEvent) {
  if (pointer !== event.pointerId || !view) return
  view.rotation.y += (event.clientX - lastX) * .012
  // Touch preserves vertical page scrolling; mouse can also tilt the model.
  if (event.pointerType === 'mouse') view.rotation.x = Math.max(-.65, Math.min(.65, view.rotation.x + (event.clientY - lastY) * .008))
  lastX = event.clientX; lastY = event.clientY; view.render()
}
function up() { pointer = undefined; dragging.value = false; sync() }
function key(event: KeyboardEvent) {
  if (!view || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return
  event.preventDefault(); paused.value = true; stop()
  if (event.key === 'Home') view.rotation.set(props.model === 'beryl-ax' ? .27 : -.08, -.4, 0)
  else if (event.key === 'End') view.rotation.set(props.model === 'beryl-ax' ? .27 : -.08, Math.PI - .4, 0)
  else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') view.rotation.y += event.key === 'ArrowLeft' ? -.2 : .2
  else view.rotation.x = Math.max(-.65, Math.min(.65, view.rotation.x + (event.key === 'ArrowUp' ? -.12 : .12)))
  view.render()
}
watch(paused, sync)
watch(() => props.model, () => {
  generation++; up()
  currentModel.value = props.model
  if (view) {
    try { view.setModel(props.model) } catch { failed.value = true; disposeScene() }
  } else { failed.value = false; void init() }
})
onMounted(() => {
  mounted = true
  if (!window.matchMedia || !window.IntersectionObserver || !window.ResizeObserver) {
    failed.value = true
    return
  }
  motion = window.matchMedia('(prefers-reduced-motion: reduce)')
  motion.addEventListener('change', sync)
  document.addEventListener('visibilitychange', sync)
  window.addEventListener('resize', layout)
  resizeObserver = new ResizeObserver(layout)
  if (host.value) resizeObserver.observe(host.value)
  if (frame.value?.parentElement) resizeObserver.observe(frame.value.parentElement)
  observer = new IntersectionObserver(entries => {
    visible = entries.some(entry => entry.isIntersecting)
    if (visible) void init()
    sync()
  })
  if (frame.value) observer.observe(frame.value)
  layout()
})
onBeforeUnmount(() => {
  mounted = false; generation++; observer?.disconnect(); resizeObserver?.disconnect()
  motion?.removeEventListener('change', sync)
  document.removeEventListener('visibilitychange', sync); window.removeEventListener('resize', layout)
  disposeScene()
})
</script>

<template>
  <figure ref="frame" class="device-model" :class="{ 'device-model--viewport': fillViewport, 'device-model--glass': glassContainer }"
    :style="{ '--device-height': remainingHeight }" :data-model="model" :data-ready="ready">
    <div ref="host" class="device-model__stage" :class="{ 'is-dragging': dragging }"
      :tabindex="ready ? 0 : -1" role="group" :aria-label="`${label}. Drag or use arrow keys to rotate. Home shows the front; End shows the back.`"
      @pointerdown="down" @pointermove="move" @pointerup="up" @pointercancel="up" @lostpointercapture="up" @keydown="key">
      <img v-if="!ready && fallback" :src="fallback" :alt="label" class="device-model__fallback" />
      <div v-else-if="!ready" class="device-model__placeholder" role="img" :aria-label="label">
        <svg viewBox="0 0 200 120" aria-hidden="true"><rect x="31" y="59" width="138" height="46" rx="15"/><path d="M38 65V17Q38 9 45 9T52 17V60M148 60V17Q148 9 155 9T162 17V65M92 87h16"/></svg>
      </div>
    </div>
    <figcaption v-if="controls || devBrowseEnabled" class="device-model__caption">
      <span>{{ failed ? label : (devBrowseEnabled ? devModelLabel(currentModel) : 'Drag to explore') }}</span>
      <template v-if="devBrowseEnabled">
        <button type="button" aria-label="Previous 3D model" @click="browseModel(-1)">Back</button>
        <span class="device-model__counter">{{ devModelIds.indexOf(currentModel) + 1 }} / {{ devModelIds.length }}</span>
        <button type="button" aria-label="Next 3D model" @click="browseModel(1)">Next</button>
      </template>
      <button v-if="ready && !reducedMotion" type="button" :aria-label="paused ? 'Resume automatic rotation' : 'Pause automatic rotation'"
        :aria-pressed="paused" @click="paused = !paused">{{ paused ? 'Rotate' : 'Pause' }}</button>
    </figcaption>
  </figure>
</template>

<style scoped>
.device-model { position: relative; width: 100%; height: 240px; margin: 0; isolation: isolate; }
.device-model--glass { padding: 16px 18px 12px; border: 1px solid rgba(255,255,255,.13); border-radius: 14px; background: linear-gradient(135deg, rgba(255,255,255,.075), rgba(255,255,255,.025)); box-shadow: inset 0 1px 0 rgba(255,255,255,.045), 0 18px 55px rgba(0,0,0,.14); }
.device-model--glass::after { content: ''; position: absolute; inset: 0; border-radius: inherit; pointer-events: none; background: linear-gradient(120deg, rgba(255,255,255,.1), transparent 34%, transparent 72%, rgba(255,255,255,.035)); opacity: .45; }
.device-model::before { content: ''; position: absolute; inset: 14% 9%; background: radial-gradient(ellipse, rgba(110,167,180,.09), transparent 68%); pointer-events: none; z-index: -1; }
.device-model__stage { position: absolute; inset: 0 0 28px; cursor: grab; touch-action: pan-y; border-radius: 16px; }
.device-model__stage.is-dragging { cursor: grabbing; }
.device-model__stage:focus-visible { outline: 1px solid rgba(255,255,255,.5); outline-offset: 2px; }
.device-model__stage :deep(canvas) { display: block; width: 100%; height: 100%; }
.device-model__fallback { width: 100%; height: 100%; object-fit: contain; padding: 22px; }
.device-model__placeholder { height: 100%; display: grid; place-items: center; }
.device-model__placeholder svg { width: min(65%, 280px); fill: rgba(63,126,146,.2); stroke: #619ba9; stroke-width: 2; }
.device-model__caption { position: absolute; inset: auto 0 0; display: flex; justify-content: center; align-items: center; gap: 14px; color: rgba(255,255,255,.4); font-size: 11px; }
.device-model__caption button { padding: 4px 9px; color: rgba(255,255,255,.65); border: 1px solid rgba(255,255,255,.12); border-radius: 20px; background: transparent; cursor: pointer; }
.device-model__counter { color: rgba(255,255,255,.3); font-variant-numeric: tabular-nums; }
.device-model--viewport { height: 290px; }
@media (min-width: 1024px) { .device-model--viewport { height: var(--device-height); min-height: 260px; } }
</style>
