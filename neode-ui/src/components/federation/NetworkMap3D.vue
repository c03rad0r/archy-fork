<template>
  <div
    ref="containerRef"
    class="node-map-stage"
    role="img"
    :aria-label="`Federation map: ${peerCount} peer${peerCount === 1 ? '' : 's'}`"
  >
    <svg ref="svgRef" class="node-map-svg"></svg>

    <!-- Legend + count overlay (HTML, not SVG, so it stays crisp and glass-styled) -->
    <div class="node-map-legend" aria-hidden="true">
      <span class="node-map-legend-item"><span class="node-map-dot" :style="{ background: motionTokens.color.trusted }"></span>Trusted</span>
      <span class="node-map-legend-item"><span class="node-map-dot" :style="{ background: motionTokens.color.observer }"></span>Observer</span>
      <span class="node-map-legend-item"><span class="node-map-dot" :style="{ background: motionTokens.color.untrusted }"></span>Untrusted</span>
      <span v-if="requests?.length" class="node-map-legend-item"><span class="node-map-dot" :style="{ background: motionTokens.color.pending }"></span>Request</span>
    </div>
    <!-- 2D/3D projection toggle — tweens the camera between the flat radial
         layout and the depth view -->
    <div class="node-map-mode-toggle" role="group" aria-label="Map projection">
      <button
        v-for="m in (['2d', '3d'] as const)"
        :key="m"
        class="node-map-mode-btn"
        :class="{ 'node-map-mode-btn-active': viewMode === m }"
        :aria-pressed="viewMode === m"
        @click="setMode(m)"
      >{{ m.toUpperCase() }}</button>
    </div>

    <!-- Peer request popover: black glass, centred over the scene -->
    <Transition name="nm-pop">
      <div v-if="activeRequest" class="node-map-popover" role="dialog" aria-label="Peer request" @click.stop>
        <button class="node-map-popover-close" aria-label="Dismiss" @click="activeRequest = null">✕</button>
        <span class="node-map-popover-badge">Peer request</span>
        <p class="node-map-popover-title">{{ activeRequest.label }}</p>
        <p class="node-map-popover-sub">wants to peer with your node</p>
        <p v-if="activeRequest.message" class="node-map-popover-msg">“{{ activeRequest.message }}”</p>
        <div class="node-map-popover-actions">
          <button class="node-map-popover-btn nm-reject" @click="decideRequest('reject')">Reject</button>
          <button class="node-map-popover-btn nm-accept" @click="decideRequest('approve')">Accept</button>
        </div>
      </div>
    </Transition>

    <div v-if="peerCount === 0 && (requests?.length ?? 0) === 0" class="node-map-empty">
      <p class="node-map-empty-title">No peers yet</p>
      <p class="node-map-empty-sub">Invite a peer or discover nodes to grow your federation</p>
    </div>
    <p class="node-map-hint" aria-hidden="true">Drag to orbit · tap a node for details</p>
  </div>
</template>

<script setup lang="ts">
// Federation network map — a keyed, diff-reconciled GSAP scene.
//
// ARCHITECTURE (the answer to "the intro is flaky and revisits are janky"):
//
// 1. The scene is KEYED by identity (peer DID / request id). Data changes
//    reconcile against the live scene — update in place, animate arrivals in,
//    animate departures out, tween slot changes — instead of wiping the SVG.
//    A wipe-and-rebuild while the intro timeline was mid-flight used to
//    orphan every tween target, which is exactly why parts of the intro
//    vanished whenever the 5s poll (or the async self-DID fetch) landed
//    during it.
// 2. The intro is GATED on readiness: the container must have a real size
//    (the first ResizeObserver callback, i.e. post-layout) and the self node
//    must exist (it arrives async), with a 1.2s fallback so a failed DID
//    fetch can't hold the scene hostage. Graph updates that land during the
//    intro are queued and applied when it completes.
// 3. Layout is DETERMINISTIC per node: peers sort by DID for slot assignment
//    and jitter/phase derive from a DID hash, so backend response order
//    can't shuffle the map between polls.
// 4. The full cinematic intro plays once per browser session; revisits get a
//    shorter, lighter entrance (they remount mid route-transition, where the
//    long dolly reads as jank).
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from 'vue'
import { gsap, motionTokens, prefersReducedMotion } from '@/utils/motion'

export interface MapNode {
  did: string
  label: string
  trust_level: 'trusted' | 'observer' | 'untrusted'
  online: boolean
  app_count: number
  is_self: boolean
}

export interface MapLink {
  source: string
  target: string
}

/** Inbound peer request awaiting a decision — rendered as a blinking yellow
 *  globe hovering outside the peer orbits (not yet part of the federation). */
export interface MapRequest {
  id: string
  label: string
  message: string | null
}

const props = defineProps<{
  nodes: MapNode[]
  links: MapLink[]
  requests?: MapRequest[]
}>()

const emit = defineEmits<{
  (e: 'select', did: string): void
  (e: 'approve', id: string): void
  (e: 'reject', id: string): void
}>()

const containerRef = ref<HTMLDivElement>()
const svgRef = ref<SVGSVGElement>()

const peerCount = computed(() => props.nodes.filter(n => !n.is_self).length)

const SVG_NS = 'http://www.w3.org/2000/svg'
const FONT = "'Avenir Next', system-ui, sans-serif"

/* ------------------------------------------------------------------ *
 * Scene state — mutated by GSAP tweens and read by the per-frame
 * render pass. Nothing here is Vue-reactive on purpose: the ticker
 * repaints at 60fps and must not churn the reactivity graph.
 * ------------------------------------------------------------------ */
const cam = {
  rotY: 0,
  tilt: -0.5, // downward tilt; tweened between the 2D and 3D projections
  persp: 3.2, // perspective strength; large = near-orthographic (flat 2D)
  zoom: 1,
  spin: 0, // rad/s — only ever non-zero from drag inertia; the scene does not
           // idle-orbit (nodes hold position so the map stays readable)
}

/** The two projections the toggle tweens between: '2d' is the original flat
 *  radial map (top-down, orthographic); '3d' is the depth view. Resolved via
 *  modeParams() — the 3D tilt steepens on portrait containers so the orbit
 *  reads as a tall ellipse instead of a squashed horizontal band. */
type MapMode = '2d' | '3d'
function modeParams(mode: MapMode): { tilt: number; persp: number } {
  if (mode === '2d') return { tilt: -1.45, persp: 18 }
  return { tilt: height > width ? -0.95 : -0.5, persp: 3.2 }
}

/** One dot on a node's point-cloud sphere, in unit-sphere local coords. */
interface GlobeDot {
  el: SVGCircleElement
  x: number
  y: number
  z: number
}

interface PeerVis {
  node: MapNode
  /** Tweenable orbital slot (reconciliation glides nodes to new slots). */
  angle: number
  ringRadius: number
  yJitter: number
  bobPhase: number
  /** Arrival progress 0→1: drives fly-in offset, scale, opacity. */
  p: number
  dying: boolean
  el: SVGGElement
  label: SVGTextElement
  linkEl: SVGPathElement | null
  dots: GlobeDot[]
  radiusPx: number
  dotBaseOpacity: number
  depth: number
}

interface RequestVis {
  req: MapRequest
  angle: number
  ringRadius: number
  yJitter: number
  bobPhase: number
  p: number
  blinking: boolean
  /** Reject-pop scale multiplier. */
  deathScale: number
  /** Set once the user decided — departure then skips the generic fade. */
  decided: boolean
  dying: boolean
  el: SVGGElement
  outline: SVGCircleElement
  halo: SVGCircleElement
  coreGlow: SVGCircleElement
  label: SVGTextElement
  linkEl: SVGPathElement
  dots: GlobeDot[]
  radiusPx: number
  depth: number
}

interface SelfVis {
  did: string
  el: SVGGElement
  label: SVGTextElement
  pulseEl: SVGCircleElement
  dots: GlobeDot[]
  radiusPx: number
}

interface RingVis {
  el: SVGPathElement
  radius: number
  kind: 'peer' | 'request'
}

/** Render arrays include dying members (they animate out in place). Maps hold
 *  only live members and drive reconciliation. */
let peers: PeerVis[] = []
let requestVis: RequestVis[] = []
const peerMap = new Map<string, PeerVis>()
const reqMap = new Map<string, RequestVis>()
let selfVis: SelfVis | null = null
const selfState = { p: 0 }
let ringPaths: RingVis[] = []
let nodesLayer: SVGGElement | null = null
let linksLayer: SVGGElement | null = null
let ringsLayer: SVGGElement | null = null

let width = 0
let height = 0
/** Scene scale + vertical centring — tweened alongside cam.tilt/persp during
 *  a 2D/3D toggle so the layout re-fits as it folds. unitX/unitY scale the
 *  two screen axes independently (capped anisotropy) so the orbit fills the
 *  container's aspect ratio instead of being bound by the narrow axis. */
const fit = { unitX: 1, unitY: 1, centerY: 0 }
let elapsed = 0
let frame = 0
let modeTween: gsap.core.Tween[] = []
let modeInitialized = false

const viewMode = ref<MapMode>('3d')
let resizeObserver: ResizeObserver | null = null
let intro: gsap.core.Timeline | null = null
let spinTween: gsap.core.Tween[] = []
let tickerAttached = false
let staticMode = false

/** Lifecycle gates (see file header). */
let built = false
let introPlayed = false
let introFallback: ReturnType<typeof setTimeout> | null = null
let pendingGraphUpdate = false

/* ------------------------------ helpers ---------------------------- */

function trustColor(n: MapNode): string {
  switch (n.trust_level) {
    case 'trusted': return motionTokens.color.trusted
    case 'observer': return motionTokens.color.observer
    case 'untrusted': return motionTokens.color.untrusted
    default: return motionTokens.color.neutral
  }
}

function nodeRadius(n: MapNode): number {
  return n.is_self ? 11 : Math.max(6, Math.min(9.5, 5.5 + n.app_count * 0.4))
}

/** Ring layout: first 8 peers on the inner orbit, next 14 on a wider one,
 *  rest beyond. `countInRing` is how many peers actually landed on that ring
 *  (given `total` peers), so angle spacing is always even. */
const RING_CAPACITIES = [8, 14, 22]
function ringFor(i: number, total: number): { ring: number; indexInRing: number; countInRing: number } {
  let start = 0
  for (let r = 0; r < RING_CAPACITIES.length; r++) {
    const cap = RING_CAPACITIES[r] ?? 8
    if (i < start + cap) {
      return { ring: r, indexInRing: i - start, countInRing: Math.min(cap, total - start) }
    }
    start += cap
  }
  return { ring: RING_CAPACITIES.length, indexInRing: i - start, countInRing: Math.max(total - start, 1) }
}

function peerSlot(i: number, total: number): { angle: number; ringRadius: number; ring: number } {
  const { ring, indexInRing, countInRing } = ringFor(i, total)
  return {
    angle: (indexInRing / countInRing) * Math.PI * 2 + ring * 0.5,
    ringRadius: 1 + ring * 0.65,
    ring,
  }
}

function maxPeerRingRadius(total: number): number {
  return total ? 1 + ringFor(total - 1, total).ring * 0.65 : 1
}

/** The request "waiting room" orbit sits clearly OUTSIDE the peer rings —
 *  requests are not part of the federation until accepted. */
function requestRingRadius(peerTotal: number): number {
  return maxPeerRingRadius(peerTotal) + 1.0
}

/** Deterministic [0,1) from a string — jitter/phases keyed by identity, so a
 *  node keeps its personality across polls, sessions, and reorderings. */
function hashStr(s: string): number {
  let h = 2166136261
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619)
  return (h >>> 0) / 4294967296
}

/** Equivalent target angle nearest to `current`, so slot-change tweens take
 *  the short way around the ring. */
function nearestAngle(current: number, target: number): number {
  const TWO = Math.PI * 2
  let delta = (target - current) % TWO
  if (delta > Math.PI) delta -= TWO
  if (delta < -Math.PI) delta += TWO
  return current + delta
}

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string> = {}): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVG_NS, tag)
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v)
  return e
}

/** Project a world-space point through the camera. Returns screen coords + depth scale. */
function project(x: number, y: number, z: number) {
  const cr = Math.cos(cam.rotY), sr = Math.sin(cam.rotY)
  const x1 = x * cr + z * sr
  const z1 = -x * sr + z * cr
  const ct = Math.cos(cam.tilt), st = Math.sin(cam.tilt)
  const y2 = y * ct - z1 * st
  const z2 = y * st + z1 * ct
  const s = cam.persp / (cam.persp + z2)
  return {
    x: width / 2 + x1 * fit.unitX * s * cam.zoom,
    y: fit.centerY + y2 * fit.unitY * s * cam.zoom,
    s,
    z: z2,
  }
}

/* ---------------------------- globe dots ---------------------------- */

/** Build a point-cloud sphere: dots on a fibonacci-sphere surface, appended
 *  to `parent` in the node's local px coordinate space. Positions/opacity are
 *  written per frame by renderGlobe(). */
function makeGlobe(parent: SVGGElement, radiusPx: number, color: string, count: number): GlobeDot[] {
  const dots: GlobeDot[] = []
  const golden = Math.PI * (3 - Math.sqrt(5))
  for (let k = 0; k < count; k++) {
    const y = 1 - (2 * (k + 0.5)) / count
    const rr = Math.sqrt(Math.max(0, 1 - y * y))
    const phi = k * golden
    const dot = el('circle', {
      r: (radiusPx * (0.06 + hashStr(String(k)) * 0.035)).toFixed(2),
      fill: color,
    })
    parent.appendChild(dot)
    dots.push({ el: dot, x: Math.cos(phi) * rr, y, z: Math.sin(phi) * rr })
  }
  return dots
}

/** One frame of a globe: spin the point cloud around its local Y axis, apply
 *  the camera tilt so every sphere shares the scene's horizon, and shade dots
 *  by depth (front bright, limb dim) so it reads as a solid sphere of points. */
function renderGlobe(dots: GlobeDot[], radiusPx: number, spin: number, baseOpacity: number) {
  const ct = Math.cos(cam.tilt), st = Math.sin(cam.tilt)
  const ca = Math.cos(spin), sa = Math.sin(spin)
  for (const d of dots) {
    const x1 = d.x * ca + d.z * sa
    const z1 = -d.x * sa + d.z * ca
    const y2 = d.y * ct - z1 * st
    const z2 = d.y * st + z1 * ct
    const t = (1 - z2) / 2 // z2 ∈ [-1,1]; front (−1) → 1
    d.el.setAttribute('cx', (x1 * radiusPx).toFixed(2))
    d.el.setAttribute('cy', (y2 * radiusPx).toFixed(2))
    d.el.setAttribute('opacity', (baseOpacity * (0.12 + 0.88 * t * t)).toFixed(3))
  }
}

/* --------------------------- scene assembly ------------------------- */

function ensureLayers() {
  if (nodesLayer) return
  const svg = svgRef.value!
  ringsLayer = el('g')
  linksLayer = el('g')
  nodesLayer = el('g')
  svg.append(ringsLayer, linksLayer, nodesLayer)
}

function createSelf(node: MapNode) {
  // Self: a BLACK point-cloud globe — dark dots over a soft light backing
  // disc so it reads against the dark glass, with the brand-orange sonar
  // pulse marking "you".
  const g = el('g', { cursor: 'pointer' }) as SVGGElement
  const r = nodeRadius(node)
  const halo = el('circle', { r: String(r * 2.2), fill: '#ffffff', opacity: '0.07' })
  const pulse = el('circle', { r: String(r), fill: 'none', stroke: motionTokens.color.accent, 'stroke-width': '1.5', opacity: '0.6' })
  const backing = el('circle', { r: String(r * 1.04), fill: '#ffffff', opacity: '0.16' })
  const rim = el('circle', { r: String(r * 1.04), fill: 'none', stroke: '#ffffff', 'stroke-width': '1', 'stroke-opacity': '0.45' })
  g.append(halo, pulse, backing, rim)
  const dots = makeGlobe(g, r, '#000000', 56)
  const label = el('text', {
    dy: String(r + 18), 'text-anchor': 'middle', class: 'nm-label',
    fill: motionTokens.color.textPrimary, 'font-size': '12px', 'font-weight': '600', 'font-family': FONT,
  })
  label.textContent = node.label || 'This node'
  const title = el('title')
  title.textContent = `${node.did}\nThis node`
  g.append(label, title)
  g.addEventListener('click', () => emit('select', node.did))
  nodesLayer!.appendChild(g)
  selfVis = { did: node.did, el: g, label, pulseEl: pulse, dots, radiusPx: r }

  if (!staticMode) {
    gsap.fromTo(pulse,
      { attr: { r }, opacity: 0.55 },
      { attr: { r: r * 2.6 }, opacity: 0, duration: 2.4, repeat: -1, ease: 'sine.out', repeatDelay: 0.6 })
  }
}

/** Per-peer static attrs derived from node data — split out so online/trust
 *  changes can restyle in place. */
function stylePeerDom(vis: PeerVis) {
  const { node, el: g } = vis
  const color = trustColor(node)
  const children = g.children
  const halo = children[0] as SVGCircleElement
  const outline = children[1] as SVGCircleElement
  const core = children[2] as SVGCircleElement
  halo.setAttribute('fill', color)
  halo.setAttribute('opacity', node.online ? '0.12' : '0.04')
  outline.setAttribute('stroke', color)
  outline.setAttribute('stroke-opacity', node.online ? '0.35' : '0.3')
  if (node.online) outline.removeAttribute('stroke-dasharray')
  else outline.setAttribute('stroke-dasharray', '3 3')
  core.setAttribute('fill', color)
  core.setAttribute('opacity', node.online ? '0.22' : '0.08')
  for (const d of vis.dots) d.el.setAttribute('fill', color)
  vis.dotBaseOpacity = node.online ? 0.95 : 0.35
  if (vis.linkEl) {
    vis.linkEl.setAttribute('stroke', node.online ? color : motionTokens.color.neutral)
    vis.linkEl.setAttribute('stroke-opacity', node.online ? '0.35' : '0.15')
    if (node.online) vis.linkEl.removeAttribute('stroke-dasharray')
    else vis.linkEl.setAttribute('stroke-dasharray', '5 4')
  }
}

function createPeer(node: MapNode, slot: { angle: number; ringRadius: number }, hasLink: boolean): PeerVis {
  const r = nodeRadius(node)
  const color = trustColor(node)
  const g = el('g', { cursor: 'pointer' }) as SVGGElement
  g.append(
    el('circle', { r: String(r * 1.9) }), // halo — styled below
    el('circle', { r: String(r), fill: 'none', stroke: color, 'stroke-width': '1' }), // outline
    el('circle', { r: String(r * 0.4) }), // core glow
  )
  const dots = makeGlobe(g, r, color, Math.max(22, Math.round(r * 3.4)))
  const label = el('text', {
    dy: String(r + 15), 'text-anchor': 'middle', class: 'nm-label',
    fill: motionTokens.color.textSecondary, 'font-size': '11px', 'font-family': FONT,
  })
  label.textContent = node.label
  const title = el('title')
  title.textContent = `${node.did}\nApps: ${node.app_count}\n${node.online ? 'Online' : 'Offline'}`
  g.append(label, title)
  g.addEventListener('click', () => emit('select', node.did))
  nodesLayer!.appendChild(g)

  let linkEl: SVGPathElement | null = null
  if (hasLink) {
    linkEl = el('path', { fill: 'none', 'stroke-width': '1.5' })
    linksLayer!.appendChild(linkEl)
  }

  const vis: PeerVis = {
    node,
    angle: slot.angle,
    ringRadius: slot.ringRadius,
    yJitter: (hashStr(node.did) - 0.5) * 0.22,
    bobPhase: hashStr(node.did + 'p') * Math.PI * 2,
    p: 0,
    dying: false,
    el: g,
    label,
    linkEl,
    dots,
    radiusPx: r,
    dotBaseOpacity: 0.95,
    depth: 0,
  }
  stylePeerDom(vis)
  peers.push(vis)
  peerMap.set(node.did, vis)
  return vis
}

function removeVisDom(elm: SVGGElement, linkEl: SVGPathElement | null) {
  elm.remove()
  linkEl?.remove()
}

function killPeer(vis: PeerVis) {
  peerMap.delete(vis.node.did)
  vis.dying = true
  gsap.killTweensOf(vis)
  const finish = () => {
    removeVisDom(vis.el, vis.linkEl)
    peers = peers.filter(p => p !== vis)
  }
  if (staticMode) finish()
  else gsap.to(vis, { p: 0, duration: 0.35, ease: 'power2.in', onComplete: finish })
}

function createRequest(req: MapRequest, slot: { angle: number; ringRadius: number }): RequestVis {
  const r = 7.5
  const color = motionTokens.color.pending
  const g = el('g', { cursor: 'pointer' }) as SVGGElement
  const halo = el('circle', { r: String(r * 2), fill: color, opacity: '0.14' })
  const outline = el('circle', {
    r: String(r), fill: 'none', stroke: color,
    'stroke-width': '1', 'stroke-opacity': '0.5', 'stroke-dasharray': '3 3',
  })
  const coreGlow = el('circle', { r: String(r * 0.4), fill: color, opacity: '0.25' })
  g.append(halo, outline, coreGlow)
  const dots = makeGlobe(g, r, color, 24)
  const label = el('text', {
    dy: String(r + 15), 'text-anchor': 'middle', class: 'nm-label',
    fill: motionTokens.color.textSecondary, 'font-size': '11px', 'font-family': FONT,
  })
  label.textContent = req.label
  const title = el('title')
  title.textContent = `Peer request from ${req.label}`
  g.append(label, title)
  // stopPropagation: the same click must not bubble to the container's
  // popover-dismiss handler and immediately close what it just opened
  g.addEventListener('click', (e) => { e.stopPropagation(); activeRequest.value = req })
  nodesLayer!.appendChild(g)
  const linkEl = el('path', {
    fill: 'none', stroke: color,
    'stroke-width': '1.2', 'stroke-opacity': '0.3', 'stroke-dasharray': '2 5',
  })
  linksLayer!.appendChild(linkEl)

  const vis: RequestVis = {
    req,
    angle: slot.angle,
    // Loose hover around the waiting-room orbit, not a tidy ring — these
    // nodes are outside the federation until accepted.
    ringRadius: slot.ringRadius + (hashStr(req.id + 'r') - 0.5) * 0.24,
    yJitter: (hashStr(req.id) - 0.5) * 0.2,
    bobPhase: hashStr(req.id + 'p') * Math.PI * 2,
    p: 0,
    blinking: true,
    deathScale: 1,
    decided: false,
    dying: false,
    el: g,
    outline,
    halo,
    coreGlow,
    label,
    linkEl,
    dots,
    radiusPx: r,
    depth: 0,
  }
  requestVis.push(vis)
  reqMap.set(req.id, vis)
  return vis
}

function killRequest(vis: RequestVis) {
  reqMap.delete(vis.req.id)
  vis.dying = true
  if (activeRequest.value?.id === vis.req.id) activeRequest.value = null
  gsap.killTweensOf(vis)
  const finish = () => {
    removeVisDom(vis.el, vis.linkEl)
    requestVis = requestVis.filter(r => r !== vis)
  }
  // Decided requests already played their bespoke exit (pop / join morph) —
  // just clear them fast once the backend confirms.
  if (staticMode || vis.decided) gsap.to(vis, { p: 0, duration: 0.15, onComplete: finish })
  else gsap.to(vis, { p: 0, duration: 0.3, ease: 'power2.in', onComplete: finish })
}

/** Reconcile the orbit guide rings with the desired set (crossfade). */
function syncRings(desired: { radius: number; kind: 'peer' | 'request' }[]) {
  const key = (radius: number, kind: string) => `${kind}:${radius.toFixed(2)}`
  const want = new Map(desired.map(d => [key(d.radius, d.kind), d]))
  const have = new Map(ringPaths.map(r => [key(r.radius, r.kind), r]))

  for (const [k, spec] of want) {
    if (have.has(k)) continue
    const p = spec.kind === 'peer'
      ? el('path', { fill: 'none', opacity: '0', stroke: motionTokens.color.lineFaint, 'stroke-width': '1' })
      : el('path', { fill: 'none', opacity: '0', stroke: motionTokens.color.pending, 'stroke-opacity': '0.12', 'stroke-width': '1', 'stroke-dasharray': '4 5' })
    ringsLayer!.appendChild(p)
    const vis: RingVis = { el: p, radius: spec.radius, kind: spec.kind }
    ringPaths.push(vis)
    if (introPlayed) {
      if (staticMode) p.setAttribute('opacity', '1')
      else gsap.to(p, { opacity: 1, duration: 0.6, ease: motionTokens.ease.inOut })
    }
  }
  for (const [k, ring] of have) {
    if (want.has(k)) continue
    ringPaths = ringPaths.filter(r => r !== ring)
    if (staticMode) ring.el.remove()
    else gsap.to(ring.el, { opacity: 0, duration: 0.4, onComplete: () => ring.el.remove() })
  }
}

/** THE reconciler: diff props against the live scene and animate the deltas.
 *  Never wipes the SVG — in-flight animations keep their targets. */
function applyGraph() {
  ensureLayers()

  const peerNodes = props.nodes
    .filter(n => !n.is_self)
    .sort((a, b) => (a.did < b.did ? -1 : a.did > b.did ? 1 : 0))
  const selfNode = props.nodes.find(n => n.is_self)

  // --- self ---
  if (selfNode && !selfVis) {
    createSelf(selfNode)
    if (introPlayed && selfState.p < 1) {
      if (staticMode) selfState.p = 1
      else gsap.to(selfState, { p: 1, duration: 0.55, ease: motionTokens.ease.arrive })
    }
  } else if (selfNode && selfVis && selfVis.label.textContent !== (selfNode.label || 'This node')) {
    selfVis.label.textContent = selfNode.label || 'This node'
  }

  // --- peers ---
  const seenDids = new Set<string>()
  peerNodes.forEach((node, i) => {
    seenDids.add(node.did)
    const slot = peerSlot(i, peerNodes.length)
    const hasLink = props.links.some(l => l.target === node.did || l.source === node.did)
    const existing = peerMap.get(node.did)

    if (!existing) {
      const vis = createPeer(node, slot, hasLink)
      if (introPlayed) {
        if (staticMode) vis.p = 1
        else gsap.to(vis, { p: 1, duration: 0.6, ease: motionTokens.ease.arrive })
      }
      return
    }

    // Structural change (trust colour, size, link gained/lost) → rebuild this
    // one node's DOM in place, preserving its arrival progress and slot.
    const structural = existing.node.trust_level !== node.trust_level
      || nodeRadius(node) !== existing.radiusPx
      || (existing.linkEl !== null) !== hasLink
    if (structural) {
      const keepP = existing.p
      const keepAngle = existing.angle
      const keepRing = existing.ringRadius
      gsap.killTweensOf(existing)
      removeVisDom(existing.el, existing.linkEl)
      peers = peers.filter(p => p !== existing)
      peerMap.delete(node.did)
      const vis = createPeer(node, slot, hasLink)
      vis.p = keepP
      vis.angle = keepAngle
      vis.ringRadius = keepRing
    } else {
      // Cheap in-place updates
      existing.node = node
      if (existing.label.textContent !== node.label) existing.label.textContent = node.label
      const title = existing.el.querySelector('title')
      if (title) title.textContent = `${node.did}\nApps: ${node.app_count}\n${node.online ? 'Online' : 'Offline'}`
      stylePeerDom(existing)
    }

    // Slot drift (another peer joined/left) → glide the short way around
    const vis = peerMap.get(node.did)!
    if (Math.abs(vis.ringRadius - slot.ringRadius) > 1e-3
      || Math.abs(nearestAngle(vis.angle, slot.angle) - vis.angle) > 1e-3) {
      if (staticMode) {
        vis.angle = slot.angle
        vis.ringRadius = slot.ringRadius
      } else {
        gsap.to(vis, {
          angle: nearestAngle(vis.angle, slot.angle),
          ringRadius: slot.ringRadius,
          duration: 0.7,
          ease: motionTokens.ease.inOut,
          overwrite: 'auto',
        })
      }
    }
  })
  for (const [did, vis] of peerMap) {
    if (!seenDids.has(did)) killPeer(vis)
  }

  // --- requests (waiting-room orbit, outside the peers) ---
  const reqs = [...(props.requests ?? [])].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
  const reqRing = requestRingRadius(peerNodes.length)
  const seenReqs = new Set<string>()
  reqs.forEach((req, i) => {
    seenReqs.add(req.id)
    const slot = { angle: (i / reqs.length) * Math.PI * 2 + 0.35, ringRadius: reqRing }
    const existing = reqMap.get(req.id)
    if (!existing) {
      const vis = createRequest(req, slot)
      if (introPlayed) {
        if (staticMode) vis.p = 1
        else gsap.to(vis, { p: 1, duration: 0.6, ease: motionTokens.ease.arrive })
      }
      return
    }
    existing.req = req
    if (existing.label.textContent !== req.label) existing.label.textContent = req.label
    if (!existing.decided) {
      const targetRing = reqRing + (hashStr(req.id + 'r') - 0.5) * 0.24
      if (Math.abs(existing.ringRadius - targetRing) > 1e-3
        || Math.abs(nearestAngle(existing.angle, slot.angle) - existing.angle) > 1e-3) {
        if (staticMode) {
          existing.angle = slot.angle
          existing.ringRadius = targetRing
        } else {
          gsap.to(existing, {
            angle: nearestAngle(existing.angle, slot.angle),
            ringRadius: targetRing,
            duration: 0.7,
            ease: motionTokens.ease.inOut,
            overwrite: 'auto',
          })
        }
      }
    }
  })
  for (const [id, vis] of reqMap) {
    if (!seenReqs.has(id)) killRequest(vis)
  }

  // --- orbit guide rings ---
  const desiredRings: { radius: number; kind: 'peer' | 'request' }[] = []
  const usedPeerRings = new Set<number>()
  peerNodes.forEach((_, i) => usedPeerRings.add(peerSlot(i, peerNodes.length).ring))
  for (const r of usedPeerRings) desiredRings.push({ radius: 1 + r * 0.65, kind: 'peer' })
  if (reqs.length) desiredRings.push({ radius: reqRing, kind: 'request' })
  syncRings(desiredRings)

  // --- re-fit to the (possibly new) outermost orbit ---
  refit()
}

/* ------------------------------ render ------------------------------ */

/** One frame: project every element through the camera and write attrs. */
function render() {
  if (!width || !height) return
  frame++
  // Point clouds update at half rate — the spin is slow, and this halves the
  // per-frame attribute writes (the dominant cost on mobile/companion).
  const updateDots = staticMode || frame % 2 === 0

  // Orbit guide rings
  for (const ring of ringPaths) {
    const steps = 72
    let d = ''
    for (let s = 0; s <= steps; s++) {
      const a = (s / steps) * Math.PI * 2
      const pt = project(Math.cos(a) * ring.radius, 0, Math.sin(a) * ring.radius)
      d += (s === 0 ? 'M' : 'L') + pt.x.toFixed(1) + ' ' + pt.y.toFixed(1)
    }
    ring.el.setAttribute('d', d)
  }

  // Self node at origin
  if (selfVis) {
    const pt = project(0, 0, 0)
    const s = pt.s * cam.zoom * selfState.p
    selfVis.el.setAttribute('transform', `translate(${pt.x},${pt.y}) scale(${Math.max(s, 0.001)})`)
    selfVis.el.setAttribute('opacity', String(selfState.p))
    if (updateDots) renderGlobe(selfVis.dots, selfVis.radiusPx, cam.rotY + elapsed * 0.22, 0.95)
  }

  const origin = project(0, 0, 0)

  for (const peer of peers) {
    // Fly-in: distance multiplier eases 1.9 → 1 as p goes 0 → 1
    const dist = peer.ringRadius * (1.9 - 0.9 * peer.p)
    // Idle motion is a gentle side-to-side sway along the ring (plus a whisper
    // of vertical drift) — nodes hold their position instead of orbiting, so
    // the map stays readable at a glance.
    const sway = staticMode ? 0 : Math.sin(elapsed * 0.55 + peer.bobPhase) * 0.04
    const bob = staticMode ? 0 : Math.sin(elapsed * 0.85 + peer.bobPhase * 1.7) * 0.02
    const a = peer.angle + sway
    const x = Math.cos(a) * dist
    const z = Math.sin(a) * dist
    const y = peer.yJitter + bob
    const pt = project(x, y, z)
    peer.depth = pt.z
    if (updateDots) {
      renderGlobe(peer.dots, peer.radiusPx, cam.rotY + elapsed * 0.18 + peer.bobPhase, peer.dotBaseOpacity)
    }

    const depthDim = 0.55 + 0.45 * Math.min(1, Math.max(0, (pt.s - 0.7) / 0.6))
    const s = pt.s * cam.zoom * (0.4 + 0.6 * peer.p)
    peer.el.setAttribute('transform', `translate(${pt.x},${pt.y}) scale(${Math.max(s, 0.001)})`)
    peer.el.setAttribute('opacity', String(peer.p * depthDim))
    peer.label.setAttribute('opacity', String(pt.s > 0.85 ? 1 : Math.max(0, (pt.s - 0.55) / 0.3)))

    if (peer.linkEl) {
      // Curved link: sag toward the plane midpoint for an orbital feel
      const mid = project(x * 0.5, y * 0.5 - 0.12, z * 0.5)
      peer.linkEl.setAttribute('d', `M${origin.x.toFixed(1)} ${origin.y.toFixed(1)} Q${mid.x.toFixed(1)} ${mid.y.toFixed(1)} ${pt.x.toFixed(1)} ${pt.y.toFixed(1)}`)
      peer.linkEl.setAttribute('opacity', String(peer.p))
    }
  }

  // Pending requests: same projection as peers, plus the attention blink
  for (const rv of requestVis) {
    const dist = rv.ringRadius * (1.9 - 0.9 * rv.p)
    const sway = staticMode ? 0 : Math.sin(elapsed * 0.55 + rv.bobPhase) * 0.04
    const a = rv.angle + sway
    const x = Math.cos(a) * dist
    const z = Math.sin(a) * dist
    const y = rv.yJitter
    const pt = project(x, y, z)
    rv.depth = pt.z
    if (updateDots) renderGlobe(rv.dots, rv.radiusPx, cam.rotY + elapsed * 0.18 + rv.bobPhase, 0.95)
    const blink = rv.blinking && !staticMode ? 0.55 + 0.45 * Math.sin(elapsed * 3.2 + rv.bobPhase) : 1
    const s = pt.s * cam.zoom * (0.4 + 0.6 * rv.p) * rv.deathScale
    rv.el.setAttribute('transform', `translate(${pt.x},${pt.y}) scale(${Math.max(s, 0.001)})`)
    rv.el.setAttribute('opacity', String(rv.p * blink))
    const mid = project(x * 0.5, y * 0.5 - 0.12, z * 0.5)
    rv.linkEl.setAttribute('d', `M${origin.x.toFixed(1)} ${origin.y.toFixed(1)} Q${mid.x.toFixed(1)} ${mid.y.toFixed(1)} ${pt.x.toFixed(1)} ${pt.y.toFixed(1)}`)
    rv.linkEl.setAttribute('opacity', String(rv.p * Math.min(blink + 0.2, 1)))
  }

  // Painter's order: farthest first so near nodes overlap far ones. The self
  // node sits at z=0 (mid-plane) and takes part in the same sort. Only touch
  // the DOM when the order actually changed.
  if (nodesLayer && (peers.length > 0 || requestVis.length > 0)) {
    const drawables: { el: SVGGElement; depth: number }[] = peers.map(p => ({ el: p.el, depth: p.depth }))
    for (const rv of requestVis) drawables.push({ el: rv.el, depth: rv.depth })
    if (selfVis) drawables.push({ el: selfVis.el, depth: 0 })
    drawables.sort((a, b) => b.depth - a.depth)
    let dirty = false
    const children = nodesLayer.children
    for (let i = 0; i < drawables.length; i++) {
      if (children[i] !== drawables[i]?.el) { dirty = true; break }
    }
    if (dirty) for (const d of drawables) nodesLayer.appendChild(d.el)
  }
}

/** Kiosk TVs run the kiosk image's conservative rasterizer (single raster
 *  thread, GPU raster off — the June choppy-audio tuning), so a 60fps
 *  SVG-attribute animation at 4K froze the browser outright (framework-pt,
 *  2026-08-12). On kiosks the ticker renders every OTHER frame (~30fps at a
 *  60Hz panel) with the skipped frame's delta carried over so spin speed is
 *  unchanged — at couch distance the half rate is invisible, the paint cost
 *  halves. */
const kioskLowPower =
  typeof document !== 'undefined' && document.documentElement.classList.contains('kiosk-mode')
let lowPowerPhase = 0
let carriedDeltaMS = 0

function tick(_time: number, deltaMS: number) {
  if (kioskLowPower) {
    carriedDeltaMS += deltaMS
    if ((lowPowerPhase++ & 1) === 1) return
    deltaMS = carriedDeltaMS
    carriedDeltaMS = 0
  }
  elapsed += deltaMS / 1000
  if (!dragging) cam.rotY += cam.spin * (deltaMS / 1000)
  render()
}

function attachTicker() {
  // Kiosks run staticMode for PLACEMENT (instant, intro-free) but still get
  // the calm orbit — half-rate via the kioskLowPower frame-skip in tick().
  if (tickerAttached || (staticMode && !kioskLowPower)) return
  gsap.ticker.add(tick)
  tickerAttached = true
}

function detachTicker() {
  if (!tickerAttached) return
  gsap.ticker.remove(tick)
  tickerAttached = false
}

/* ----------------------------- intro ------------------------------ */

/** Session flag: the long cinematic intro plays once per browser session;
 *  revisits (component remounts on every route entry) get a shorter, lighter
 *  entrance that doesn't fight the route transition. */
function sessionIntroSeen(): boolean {
  try { return sessionStorage.getItem('nm-intro-seen') === '1' } catch { return false }
}
function markSessionIntroSeen() {
  try { sessionStorage.setItem('nm-intro-seen', '1') } catch { /* private mode */ }
}

function startIntroIfReady(force = false) {
  if (introPlayed || !built) return
  // Wait for the self node (async DID fetch) so the intro includes the centre
  // and its links — unless the fallback timer forces the show to go on.
  if (!selfVis && !force) return
  introPlayed = true
  if (introFallback) { clearTimeout(introFallback); introFallback = null }
  playIntro()
}

function playIntro() {
  intro?.kill()
  if (staticMode) {
    // Reduced motion: no dolly, no stagger — everything lands in place.
    cam.rotY = 0
    cam.zoom = 1
    selfState.p = 1
    for (const p of peers) p.p = 1
    for (const rv of requestVis) rv.p = 1
    for (const r of ringPaths) r.el.setAttribute('opacity', '1')
    render()
    flushPendingGraph()
    return
  }

  const short = sessionIntroSeen()
  markSessionIntroSeen()
  const T = short
    ? { rotY: -0.35, zoom: 0.94, delay: 0.05, dolly: 0.7, self: 0.4, rings: 0.5, node: 0.5, stagger: 0.035, nodesAt: 0.15, reqsAt: 0.3 }
    : { rotY: -1.1, zoom: 0.82, delay: 0.2, dolly: motionTokens.duration.cinematic, self: 0.55, rings: 0.9, node: 0.7, stagger: 0.07, nodesAt: 0.45, reqsAt: 0.7 }

  cam.rotY = T.rotY
  cam.zoom = T.zoom
  selfState.p = 0
  for (const p of peers) p.p = 0
  for (const rv of requestVis) rv.p = 0

  intro = gsap.timeline({ delay: T.delay, onComplete: flushPendingGraph })
  intro
    .to(cam, { rotY: 0, zoom: 1, duration: T.dolly, ease: motionTokens.ease.out }, 0)
    .to(selfState, { p: 1, duration: T.self, ease: motionTokens.ease.arrive }, T.nodesAt * 0.4)
  if (ringPaths.length) {
    intro.to(ringPaths.map(r => r.el), { opacity: 1, duration: T.rings, ease: motionTokens.ease.inOut, stagger: 0.1 }, 0.25)
  }
  if (peers.length) {
    intro.to(peers, {
      p: 1, duration: T.node, ease: motionTokens.ease.arrive,
      stagger: { each: T.stagger, from: 'random' },
    }, T.nodesAt)
  }
  if (requestVis.length) {
    intro.to(requestVis, { p: 1, duration: T.node, ease: motionTokens.ease.arrive, stagger: 0.08 }, T.reqsAt)
  }
}

/** Graph updates that land mid-intro are deferred here and applied once. */
function flushPendingGraph() {
  if (!pendingGraphUpdate) return
  pendingGraphUpdate = false
  applyGraph()
}

/* ----------------------- request popover ------------------------- */

const activeRequest = ref<MapRequest | null>(null)

function decideRequest(decision: 'approve' | 'reject') {
  const req = activeRequest.value
  if (!req) return
  activeRequest.value = null
  const rv = reqMap.get(req.id)
  if (rv && !staticMode) {
    rv.blinking = false
    rv.decided = true
    gsap.killTweensOf(rv)
    if (decision === 'reject') {
      // Pop out of existence: a quick swell, then collapse — the dotted
      // link dies with the node.
      gsap.timeline()
        .to(rv, { deathScale: 1.22, duration: 0.14, ease: 'power2.out' })
        .to(rv, { deathScale: 0.001, p: 0.0001, duration: 0.32, ease: 'back.in(2.4)' })
    } else {
      // Join: green burst ring, the point cloud and link morph to the
      // trusted colour, and the globe glides in from the waiting-room orbit
      // onto the peer rings.
      const trusted = motionTokens.color.trusted
      const burst = el('circle', {
        r: String(rv.radiusPx), fill: 'none',
        stroke: trusted, 'stroke-width': '2', opacity: '0.8',
      })
      rv.el.appendChild(burst)
      gsap.to(burst, { attr: { r: rv.radiusPx * 3.2 }, opacity: 0, duration: 0.9, ease: 'sine.out' })
      rv.outline.removeAttribute('stroke-dasharray')
      rv.linkEl.removeAttribute('stroke-dasharray')
      gsap.to(rv.dots.map(d => d.el), { attr: { fill: trusted }, duration: 0.6, ease: motionTokens.ease.inOut })
      gsap.to([rv.halo, rv.coreGlow], { attr: { fill: trusted }, duration: 0.6, ease: motionTokens.ease.inOut })
      gsap.to([rv.outline, rv.linkEl], { attr: { stroke: trusted }, duration: 0.6, ease: motionTokens.ease.inOut })
      gsap.to(rv, { ringRadius: 1, duration: 0.9, ease: motionTokens.ease.inOut, delay: 0.15 })
    }
  } else if (rv) {
    rv.decided = true
  }
  if (decision === 'approve') emit('approve', req.id)
  else emit('reject', req.id)
}

/** Tapping empty map space dismisses the popover (request-node clicks
 *  stopPropagation, and the popover itself uses @click.stop). */
function onStageClick() {
  if (activeRequest.value) activeRequest.value = null
}

/* --------------------------- interaction --------------------------- */

let dragging = false
let lastX = 0
let dragVel = 0
let dragDistance = 0

function onPointerDown(e: PointerEvent) {
  if (staticMode) return
  // Reset BEFORE the toggle early-return: a stale post-drag distance would
  // otherwise make onClickCapture swallow toggle taps forever ("stuck" toggle)
  dragDistance = 0
  // Never hijack the projection toggle's taps
  if ((e.target as Element | null)?.closest?.('.node-map-mode-toggle')) return
  dragging = true
  lastX = e.clientX
  dragVel = 0
  for (const t of spinTween) t.kill()
  spinTween = []
  // Deliberately NO setPointerCapture: capture retargets pointerup to the
  // container, which suppresses the browser's click synthesis on child
  // elements — it silently killed node taps and the 2D/3D toggle. Drag
  // continuity outside the container comes from window-level move/up
  // listeners instead.
}

function onPointerMove(e: PointerEvent) {
  if (!dragging) return
  const dx = e.clientX - lastX
  lastX = e.clientX
  dragDistance += Math.abs(dx)
  cam.rotY += dx * 0.006
  dragVel = dx * 0.006 * 60 // approx rad/s
}

/** Swallow the click that follows a real drag so releasing an orbit fling
 *  over a node doesn't open its detail modal. */
function onClickCapture(e: MouseEvent) {
  if (dragDistance > 6) {
    e.stopPropagation()
    e.preventDefault()
    // One-shot: only the click synthesized from THIS drag is swallowed
    dragDistance = 0
  }
}

function onPointerUp(_e: PointerEvent) {
  if (!dragging) return
  dragging = false
  // Inertia: carry the fling velocity, then settle to a full stop — the
  // scene never idle-orbits on its own.
  cam.spin = Math.max(-3, Math.min(3, dragVel))
  spinTween = [gsap.to(cam, { spin: 0, duration: 1.4, ease: 'power2.out' })]
}

/* --------------------------- fit & modes --------------------------- */

/** Auto-fit for a given projection: sample the outermost orbit through the
 *  camera math (rotation-invariant — a ring about the Y axis projects
 *  identically at any rotY) to get the scene's true projected bounds, then
 *  scale to fill the container and centre vertically between the overlays.
 *  Pure with respect to tilt/persp so a mode toggle can compute its target
 *  fit and tween towards it. */
function computeFit(tilt: number, perspVal: number): { unitX: number; unitY: number; centerY: number } {
  let maxRing = 1
  for (const r of ringPaths) maxRing = Math.max(maxRing, r.radius)
  // Requests hover loosely up to ~0.12 beyond their guide ring
  if (ringPaths.some(r => r.kind === 'request')) maxRing += 0.15
  const ct = Math.cos(tilt), st = Math.sin(tilt)
  let maxAbsX = 0
  let yMin = Infinity
  let yMax = -Infinity
  for (let i = 0; i < 72; i++) {
    const a = (i / 72) * Math.PI * 2
    const x = Math.cos(a) * maxRing
    const z = Math.sin(a) * maxRing
    for (const y of [-0.2, 0.2]) { // covers yJitter + sway/bob amplitude
      const y2 = y * ct - z * st
      const z2 = y * st + z * ct
      const s = perspVal / (perspVal + z2)
      maxAbsX = Math.max(maxAbsX, Math.abs(x * s))
      yMin = Math.min(yMin, y2 * s)
      yMax = Math.max(yMax, y2 * s)
    }
  }
  // Compact containers (phones/companion) trade breathing room for scene
  // size — the map must fill the screen to stay readable.
  const compact = width < 480 || height < 480
  const marginX = compact ? 22 : 46 // node radius + label half-width clearance
  const marginTop = compact ? 42 : 54 // legend / toggle chips
  const marginBottom = compact ? 58 : 66 // hint + bottom-centre toggle (mobile)
  const bandH = Math.max(80, height - marginTop - marginBottom)
  const rawX = (width / 2 - marginX) / maxAbsX
  const rawY = bandH / Math.max(yMax - yMin, 0.001)
  // Anisotropic fill: each axis takes its own bound, capped at 1.75× the
  // other so the orbit stretches to the container's aspect ratio without
  // degenerating. This is what lets a portrait phone use its full height in
  // both 2D and 3D instead of shrinking to the narrow width.
  const ANISO = 1.75
  const unitX = Math.max(20, Math.min(rawX, rawY * ANISO))
  const unitY = Math.max(20, Math.min(rawY, rawX * ANISO))
  // Place the projected ellipse's midpoint at the centre of the available band
  return { unitX, unitY, centerY: marginTop + bandH / 2 - ((yMax + yMin) / 2) * unitY }
}

/** Re-fit after a graph change (e.g. the outermost orbit appeared/vanished).
 *  Tweens once the scene is live so the reframe glides instead of jumping. */
function refit() {
  if (!modeInitialized) return
  const m = modeParams(viewMode.value)
  const target = computeFit(m.tilt, m.persp)
  if (Math.abs(target.unitX - fit.unitX) < 0.5
    && Math.abs(target.unitY - fit.unitY) < 0.5
    && Math.abs(target.centerY - fit.centerY) < 0.5) return
  gsap.killTweensOf(fit)
  if (introPlayed && !staticMode) {
    gsap.to(fit, { ...target, duration: 0.6, ease: motionTokens.ease.inOut })
  } else {
    Object.assign(fit, target)
  }
}

/** Snap (or tween, via setMode) the camera + fit to the given mode. */
function applyMode(mode: MapMode, animate: boolean) {
  const target = modeParams(mode)
  const targetFit = computeFit(target.tilt, target.persp)
  for (const t of modeTween) t.kill()
  modeTween = []
  if (animate && !staticMode) {
    const opts = { duration: 0.9, ease: motionTokens.ease.inOut }
    modeTween.push(
      gsap.to(cam, { tilt: target.tilt, persp: target.persp, ...opts }),
      gsap.to(fit, { ...targetFit, ...opts }),
    )
  } else {
    cam.tilt = target.tilt
    cam.persp = target.persp
    Object.assign(fit, targetFit)
    render()
  }
}

function setMode(mode: MapMode) {
  if (viewMode.value === mode) return
  viewMode.value = mode
  try { localStorage.setItem('federation-map-projection', mode) } catch { /* private mode */ }
  applyMode(mode, true)
}

/* ----------------------------- lifecycle --------------------------- */

function measure() {
  const c = containerRef.value
  if (!c) return
  width = c.clientWidth
  height = c.clientHeight
  svgRef.value?.setAttribute('viewBox', `0 0 ${width} ${height}`)
  // Zero-size means layout hasn't happened yet (mid route transition) —
  // building now would compute a garbage fit. The observer will call again.
  if (width === 0 || height === 0) return
  if (!built) {
    initialBuild()
    return
  }
  // Resize: snap to the current mode's fit (no tween — tracks the drag)
  applyMode(viewMode.value, false)
  // No ticker (reduced-motion) means nobody repaints after this resize —
  // without an explicit render the map keeps its stale (possibly zero-size)
  // projection: the "loads blank / wrong scale until refresh" bug.
  if (built && !tickerAttached) render()
}

function initialBuild() {
  // Kiosk TVs take the static PLACEMENT path (the GSAP entrance intro needs
  // healthy rAF delivery to reach opacity 1 — on a paint-starved kiosk it
  // stalls and the screen reads as BLANK until a lucky refresh,
  // framework-pt 2026-08-14) but keep the calm orbital motion: the ticker
  // re-attaches below at half rate. Fully-static-everything was tried and
  // rejected — it also killed the animation on screens that could carry it.
  staticMode = prefersReducedMotion() || kioskLowPower
  built = true
  applyGraph()

  // First measurement decides the default projection: saved preference wins,
  // otherwise portrait containers (phone/companion) AND kiosk TVs open in the
  // flat 2D view — the depth view stays one tap away and is remembered. The
  // kiosk default exists for the same reason as the half-rate ticker above.
  modeInitialized = true
  let saved: string | null = null
  try { saved = localStorage.getItem('federation-map-projection') } catch { /* private mode */ }
  viewMode.value =
    saved === '2d' || saved === '3d' ? saved : (kioskLowPower || height > width ? '2d' : '3d')
  applyMode(viewMode.value, false)

  if (staticMode) {
    introPlayed = true
    playIntro() // static branch: everything lands in place
    attachTicker() // no-op for reduced-motion; kiosks resume the calm orbit
    render() // paint the landed state now — nothing else repaints until a tick
    return
  }
  attachTicker()
  startIntroIfReady()
  if (!introPlayed) {
    introFallback = setTimeout(() => startIntroIfReady(true), 1200)
  }
}

const graphSignature = computed(() => JSON.stringify({
  nodes: props.nodes.map(n => [n.did, n.label, n.trust_level, n.online, n.app_count, n.is_self]),
  links: props.links.map(l => [l.source, l.target]),
  requests: (props.requests ?? []).map(r => [r.id, r.label]),
}))

watch(graphSignature, () => {
  if (!built) return // initial build reads the latest props when size arrives
  if (intro?.isActive()) {
    // Never reconcile under a running intro — that's how tween targets used
    // to get orphaned and parts of the intro went missing.
    pendingGraphUpdate = true
    return
  }
  applyGraph()
  startIntroIfReady() // covers the late-arriving self node
})

onMounted(() => {
  resizeObserver = new ResizeObserver(() => measure())
  if (containerRef.value) {
    resizeObserver.observe(containerRef.value)
    containerRef.value.addEventListener('pointerdown', onPointerDown)
    containerRef.value.addEventListener('click', onClickCapture, true)
    containerRef.value.addEventListener('click', onStageClick)
    // Window-level so a drag that leaves the container keeps tracking
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)
    window.addEventListener('pointercancel', onPointerUp)
  }
})

onUnmounted(() => {
  detachTicker()
  if (introFallback) { clearTimeout(introFallback); introFallback = null }
  intro?.kill()
  for (const t of spinTween) t.kill()
  for (const t of modeTween) t.kill()
  gsap.killTweensOf([cam, fit, selfState])
  for (const p of peers) gsap.killTweensOf(p)
  for (const rv of requestVis) gsap.killTweensOf(rv)
  if (selfVis) gsap.killTweensOf(selfVis.pulseEl)
  resizeObserver?.disconnect()
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', onPointerUp)
  window.removeEventListener('pointercancel', onPointerUp)
})

// KeepAlive-aware: pause the 60fps loop while the tab is cached, resume on return
onDeactivated(() => detachTicker())
onActivated(() => { if (built && !staticMode) attachTicker() })
</script>

<style scoped>
.node-map-stage {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 320px;
  background:
    radial-gradient(ellipse at 50% 42%, rgba(251, 146, 60, 0.05), transparent 55%),
    radial-gradient(ellipse at 50% 120%, rgba(255, 255, 255, 0.04), transparent 60%),
    rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  border-radius: 1rem;
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.22);
  overflow: hidden;
  cursor: grab;
  touch-action: pan-y;
  user-select: none;
  -webkit-user-select: none;
}
.node-map-stage:active {
  cursor: grabbing;
}
.node-map-svg {
  width: 100%;
  height: 100%;
  display: block;
}
.node-map-legend {
  position: absolute;
  top: 12px;
  left: 12px;
  display: flex;
  gap: 10px;
  padding: 6px 12px;
  border-radius: 9999px;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  pointer-events: none;
}
.node-map-legend-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 10px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.55);
}
.node-map-dot {
  width: 7px;
  height: 7px;
  border-radius: 9999px;
  display: inline-block;
}
.node-map-mode-toggle {
  position: absolute;
  top: 12px;
  right: 12px;
  display: flex;
  gap: 2px;
  padding: 3px;
  border-radius: 9999px;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}
.node-map-mode-btn {
  /* min-height !important: the global ≤767px 44px touch-target rule would
     deform this deliberately-compact control */
  min-height: 0 !important;
  padding: 3px 10px;
  border-radius: 9999px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: rgba(255, 255, 255, 0.5);
  transition: color 0.2s ease, background-color 0.2s ease;
}
.node-map-mode-btn:hover {
  color: rgba(255, 255, 255, 0.85);
}
.node-map-mode-btn-active {
  background: rgba(255, 255, 255, 0.14);
  color: rgba(255, 255, 255, 0.95);
}
.node-map-hint {
  position: absolute;
  bottom: 10px;
  left: 0;
  right: 0;
  text-align: center;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.35);
  pointer-events: none;
  margin: 0;
}
.node-map-popover {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: min(300px, calc(100% - 32px));
  padding: 18px 16px 14px;
  border-radius: 1rem;
  background: rgba(0, 0, 0, 0.78);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  border: 1px solid rgba(255, 255, 255, 0.14);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45), inset 0 1px 0 rgba(255, 255, 255, 0.16);
  text-align: center;
}
.node-map-popover-close {
  position: absolute;
  top: 6px;
  right: 8px;
  min-height: 0 !important;
  padding: 4px 8px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.4);
}
.node-map-popover-close:hover {
  color: rgba(255, 255, 255, 0.85);
}
.node-map-popover-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 9999px;
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #facc15;
  background: rgba(250, 204, 21, 0.12);
  margin-bottom: 8px;
}
.node-map-popover-title {
  font-size: 0.95rem;
  font-weight: 700;
  color: rgba(255, 255, 255, 0.95);
  margin: 0;
  overflow-wrap: anywhere;
}
.node-map-popover-sub {
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.5);
  margin: 2px 0 0;
}
.node-map-popover-msg {
  font-size: 0.8rem;
  font-style: italic;
  color: rgba(255, 255, 255, 0.75);
  margin: 10px 0 0;
  overflow-wrap: anywhere;
}
.node-map-popover-actions {
  display: flex;
  gap: 8px;
  margin-top: 14px;
}
.node-map-popover-btn {
  flex: 1;
  padding: 9px 0;
  border-radius: 0.6rem;
  font-size: 0.8rem;
  font-weight: 600;
  border: 1px solid rgba(255, 255, 255, 0.12);
  transition: background-color 0.2s ease, border-color 0.2s ease;
}
.node-map-popover-btn.nm-accept {
  color: #4ade80;
  background: rgba(74, 222, 128, 0.12);
}
.node-map-popover-btn.nm-accept:hover {
  background: rgba(74, 222, 128, 0.22);
  border-color: rgba(74, 222, 128, 0.4);
}
.node-map-popover-btn.nm-reject {
  color: #f87171;
  background: rgba(239, 68, 68, 0.10);
}
.node-map-popover-btn.nm-reject:hover {
  background: rgba(239, 68, 68, 0.2);
  border-color: rgba(239, 68, 68, 0.4);
}
/* Popover enter/leave: quick glass pop */
.nm-pop-enter-active,
.nm-pop-leave-active {
  transition: opacity 0.22s ease, transform 0.22s ease;
}
.nm-pop-enter-from,
.nm-pop-leave-to {
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.92);
}

.node-map-empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  padding-bottom: 18%;
  text-align: center;
  pointer-events: none;
}
.node-map-empty-title {
  font-size: 0.95rem;
  font-weight: 600;
  color: rgba(255, 255, 255, 0.85);
  margin: 0 0 4px;
}
.node-map-empty-sub {
  font-size: 0.8rem;
  color: rgba(255, 255, 255, 0.45);
  margin: 0;
  max-width: 260px;
}

@media (max-width: 767px) {
  /* Key centred on mobile, mirroring the bottom-centre toggle */
  .node-map-legend {
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: calc(100% - 16px);
    padding: 5px 10px;
    gap: 8px;
  }
  /* Toggle drops to bottom-centre on mobile — thumb reach, and it frees the
     top edge so the scene can climb higher */
  .node-map-mode-toggle {
    top: auto;
    right: auto;
    bottom: 10px;
    left: 50%;
    transform: translateX(-50%);
  }
  .node-map-hint {
    bottom: 44px;
    font-size: 10px;
  }
}
</style>

<!-- Unscoped on purpose: the SVG labels are created with createElementNS at
     runtime, so Vue's scoped-style data attributes never land on them. -->
<style>
@media (max-width: 767px) {
  .node-map-stage text.nm-label {
    font-size: 10px;
  }
}
</style>
