<template>
  <!-- Hop-route visualization modal: click a message's transport pill to see
       how it traveled. Teleported to body (full-screen backdrop, project rule).
       Branded redesign: glowing endpoint medallions ringed with EQ segments
       (ScreensaverRing motif), accent-colored track with staggered relay
       markers and an animated packet traveling sender → recipient. Vertical
       stacked layout below 560px; prefers-reduced-motion disables all loops. -->
  <Teleport to="body">
    <div class="mesh-transport-modal-backdrop" @click.self="emit('close')">
      <div class="glass-card hopviz-panel" :style="panelStyle">
        <h3 class="hopviz-title">{{ transportLabel || 'Message' }} route</h3>
        <p class="hopviz-sub">{{ senderName }} → {{ recipientName }}</p>

        <div class="hopviz-chain">
          <div class="hopviz-endpoint">
            <div class="hopviz-medallion">
              <span v-for="i in RING_SEGMENTS" :key="i" class="hopviz-seg" :style="ringSegStyle(i)" />
              <span class="hopviz-glyph">🏝️</span>
            </div>
            <span class="hopviz-name">{{ senderName }}</span>
          </div>

          <div class="hopviz-path">
            <div class="hopviz-track" :class="{ 'hopviz-track-unknown': !msg.transport }">
              <div class="hopviz-track-line"></div>
              <span
                v-for="i in relayCount"
                :key="i"
                class="hopviz-relay"
                :style="relayStyle(i)"
                title="relay node"
              >
                <template v-if="msg.transport === 'tor'">🧅</template>
                <template v-else>
                  <i v-for="b in 3" :key="b" class="hopviz-relay-bar" :style="{ '--bar-i': b }" />
                </template>
              </span>
              <div class="hopviz-packet"></div>
            </div>
            <div class="hopviz-track-label">{{ routeLabel }}</div>
          </div>

          <div class="hopviz-endpoint">
            <div class="hopviz-medallion">
              <span v-for="i in RING_SEGMENTS" :key="i" class="hopviz-seg" :style="ringSegStyle(i)" />
              <span class="hopviz-glyph">🏝️</span>
            </div>
            <span class="hopviz-name">{{ recipientName }}</span>
          </div>
        </div>

        <div v-if="showSignal" class="hopviz-meta-row">
          <span class="hopviz-chip hopviz-chip-strong">{{ signalLabel }}</span>
          <span v-if="peer?.snr != null" class="hopviz-chip">SNR {{ peer.snr.toFixed(1) }} dB</span>
          <span v-if="peer?.rssi != null" class="hopviz-chip">RSSI {{ peer.rssi }} dBm</span>
          <p class="hopviz-note">Signal values are the current link readings for this peer, not a snapshot from this message.</p>
        </div>
        <div class="hopviz-meta-row">
          <span v-if="msg.encrypted" class="mesh-chat-e2e">E2E</span>
          <span class="hopviz-chip">{{ deliveryLabel }}</span>
          <span class="hopviz-chip">{{ timeLabel }}</span>
        </div>

        <button class="mesh-transport-cancel" @click="emit('close')">Close</button>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { MeshMessage, MeshPeer } from '@/stores/mesh'

const props = defineProps<{
  msg: MeshMessage
  peer: MeshPeer | null
  /** Pre-computed by Mesh.vue's transportLabel() — don't duplicate the logic. */
  transportLabel: string | null
  /** Pre-computed by Mesh.vue's signalQualityLabel(). */
  signalLabel: string
  /** Pre-computed by Mesh.vue's timeAgo(). */
  timeLabel: string
}>()

const emit = defineEmits<{ close: [] }>()

/** EQ segments around each endpoint medallion (compact ScreensaverRing echo). */
const RING_SEGMENTS = 14

/** Accent per transport — MUST match the chat transport pill colors in
 *  mesh-styles.css (.mesh-chat-transport.transport-*). */
const TRANSPORT_ACCENTS: Record<string, string> = {
  meshtastic: '#3eb489', // mint
  meshcore: '#fb923c', // orange
  reticulum: '#60a5fa', // blue
  lora: '#f59e0b', // amber
  fips: '#a78bfa', // violet
  tor: '#818cf8', // indigo
}

function hexToRgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

const accent = computed(() => TRANSPORT_ACCENTS[props.msg.transport ?? ''] ?? '#fb923c')

const panelStyle = computed(() => ({
  '--hop-accent': accent.value,
  '--hop-accent-soft': hexToRgba(accent.value, 0.35),
  '--hop-accent-faint': hexToRgba(accent.value, 0.14),
}))

const peerName = computed(() => props.peer?.advert_name || props.msg.peer_name || 'Peer')
const senderName = computed(() => (props.msg.direction === 'sent' ? 'You' : peerName.value))
const recipientName = computed(() => (props.msg.direction === 'sent' ? peerName.value : 'You'))

/** LoRa transports only know a hop COUNT (peer.hops; 0 or 0xff/null = direct). */
const hops = computed<number | null>(() => {
  const p = props.peer
  if (!p || p.hops == null || p.hops === 0xff) return null
  return p.hops
})

const showSignal = computed(() => props.msg.transport !== 'tor' && props.msg.transport !== 'fips')

const relayCount = computed(() => {
  const t = props.msg.transport
  if (t === 'tor') return 3 // fixed circuit shape, anonymous
  if (t === 'fips' || !t) return 0 // direct P2P / not recorded
  return hops.value ? Math.min(hops.value, 6) : 0
})

const routeLabel = computed(() => {
  const t = props.msg.transport
  if (t === 'tor') return '🧅 3 anonymous relays'
  if (t === 'fips') return '⚡ FIPS overlay · direct peer-to-peer'
  if (!t) return "🛰 transport wasn't recorded for this message"
  const h = hops.value
  return `📡 ${h === null || h === 0 ? 'direct radio link' : `${h} hop${h === 1 ? '' : 's'}`}`
})

const deliveryLabel = computed(() =>
  props.msg.delivered && props.msg.direction === 'sent'
    ? 'delivered ✓✓'
    : props.msg.direction === 'sent'
      ? 'sent'
      : 'received',
)

function ringSegStyle(i: number) {
  return {
    '--seg-deg': `${((i - 1) / RING_SEGMENTS) * 360}deg`,
    '--seg-i': String(i - 1),
  }
}

function relayStyle(i: number) {
  return {
    '--relay-pos': `${Math.round((i / (relayCount.value + 1)) * 100)}%`,
    '--relay-i': String(i - 1),
  }
}
</script>

<style scoped>
.hopviz-panel {
  width: min(560px, 94vw);
  max-height: 90vh;
  overflow-y: auto;
  padding: 26px 28px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.hopviz-title {
  margin: 0;
  font-family: 'Montserrat', sans-serif;
  font-weight: 700;
  font-size: 1.15rem;
  letter-spacing: 0.02em;
  color: var(--hop-accent);
  text-shadow: 0 0 18px var(--hop-accent-faint);
}
.hopviz-sub {
  margin: 0;
  color: rgba(255, 255, 255, 0.6);
  font-size: 0.85rem;
  overflow-wrap: anywhere;
}

/* ── Chain: sender medallion → track → recipient medallion ─────────────── */
.hopviz-chain {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 4px 10px;
}
/* Staggered route reveal: sender, then track+relays, then recipient. */
.hopviz-chain > * {
  opacity: 0;
  animation: hopviz-appear 0.4s ease forwards;
}
.hopviz-chain > *:nth-child(1) { animation-delay: 0.05s; }
.hopviz-chain > *:nth-child(2) { animation-delay: 0.35s; }
.hopviz-chain > *:nth-child(3) { animation-delay: 0.65s; }
@keyframes hopviz-appear {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: none; }
}

.hopviz-endpoint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  min-width: 84px;
}

/* Endpoint medallion: island glyph inside a compact EQ-segment ring
   (ScreensaverRing technique: rotate + translateY(-radius) + scaleY pulse). */
.hopviz-medallion {
  position: relative;
  width: 72px;
  height: 72px;
  --ring-radius: 30px;
}
.hopviz-glyph {
  position: absolute;
  inset: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1.6rem;
  border-radius: 50%;
  background: radial-gradient(circle, var(--hop-accent-faint) 0%, rgba(255, 255, 255, 0.03) 70%);
  border: 1px solid var(--hop-accent-faint);
  box-shadow: 0 0 26px var(--hop-accent-soft);
}
.hopviz-seg {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 3px;
  height: 11px;
  margin-left: -1.5px;
  margin-top: -5.5px;
  border-radius: 2px;
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.85), var(--hop-accent-soft));
  transform-origin: center center;
  transform: rotate(var(--seg-deg)) translateY(calc(-1 * var(--ring-radius)));
  animation: hopviz-seg-pulse 2.6s ease-in-out infinite;
  animation-delay: calc(var(--seg-i) * 0.12s);
}
@keyframes hopviz-seg-pulse {
  0%, 100% {
    opacity: 0.35;
    transform: rotate(var(--seg-deg)) translateY(calc(-1 * var(--ring-radius))) scaleY(0.5);
  }
  50% {
    opacity: 0.95;
    transform: rotate(var(--seg-deg)) translateY(calc(-1 * var(--ring-radius))) scaleY(1);
  }
}

.hopviz-name {
  font-size: 0.8rem;
  font-weight: 600;
  color: #fff;
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ── Path: accent track + relay markers + traveling packet + label ─────── */
.hopviz-path {
  flex: 1 1 auto;
  min-width: 140px;
  align-self: center;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.hopviz-track {
  position: relative;
  width: 100%;
  height: 44px;
}
.hopviz-track-line {
  position: absolute;
  left: 6px;
  right: 6px;
  top: 50%;
  height: 2px;
  transform: translateY(-50%);
  background: linear-gradient(to right, transparent, var(--hop-accent) 12%, var(--hop-accent) 88%, transparent);
  opacity: 0.75;
  box-shadow: 0 0 8px var(--hop-accent-soft);
  border-radius: 1px;
}
.hopviz-track-unknown .hopviz-track-line { opacity: 0.3; box-shadow: none; }

/* Relay markers sit ON the track: tiny EQ clusters (🧅 for Tor's circuit). */
.hopviz-relay {
  position: absolute;
  left: var(--relay-pos);
  top: 50%;
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 2px;
  font-size: 0.85rem;
  line-height: 1;
  filter: drop-shadow(0 0 6px var(--hop-accent-soft));
}
.hopviz-relay-bar {
  display: block;
  width: 3px;
  height: 12px;
  border-radius: 2px;
  background: linear-gradient(to bottom, #fff, var(--hop-accent));
  transform-origin: center;
  animation: hopviz-bar-pulse 1.8s ease-in-out infinite;
  animation-delay: calc((var(--relay-i, 0) * 0.3s) + (var(--bar-i, 0) * 0.15s));
}
.hopviz-relay-bar:nth-child(2) { height: 16px; }
@keyframes hopviz-bar-pulse {
  0%, 100% { transform: scaleY(0.45); opacity: 0.5; }
  50% { transform: scaleY(1); opacity: 1; }
}

/* The packet: a bright glowing dot traveling sender → recipient. */
.hopviz-packet {
  position: absolute;
  left: 0%;
  top: 50%;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  background: #fff;
  box-shadow:
    0 0 6px 1px #fff,
    0 0 14px 4px var(--hop-accent),
    0 0 26px 8px var(--hop-accent-soft);
  opacity: 0;
  animation: hopviz-packet-x 2.2s ease-in-out infinite;
  animation-delay: 0.9s;
}
@keyframes hopviz-packet-x {
  0% { left: 0%; opacity: 0; }
  10% { opacity: 1; }
  88% { opacity: 1; }
  100% { left: 100%; opacity: 0; }
}
@keyframes hopviz-packet-y {
  0% { top: 0%; opacity: 0; }
  10% { opacity: 1; }
  88% { opacity: 1; }
  100% { top: 100%; opacity: 0; }
}

.hopviz-track-label {
  text-align: center;
  font-size: 0.8rem;
  color: rgba(255, 255, 255, 0.75);
  overflow-wrap: anywhere;
}

/* ── Metadata footer: glass chips ──────────────────────────────────────── */
.hopviz-meta-row {
  display: flex;
  align-items: center;
  gap: 8px;
  justify-content: center;
  flex-wrap: wrap;
}
.hopviz-chip {
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.7);
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  padding: 3px 10px;
}
.hopviz-chip-strong {
  color: #fff;
  border-color: var(--hop-accent-soft);
  background: var(--hop-accent-faint);
}
.hopviz-note {
  flex-basis: 100%;
  text-align: center;
  font-size: 0.68rem;
  color: rgba(255, 255, 255, 0.35);
  margin: 2px 0 0;
}

/* ── Mobile: vertical chain, sender top → recipient bottom ─────────────── */
@media (max-width: 560px) {
  .hopviz-panel { width: 94vw; padding: 22px 18px; }
  .hopviz-chain { flex-direction: column; gap: 8px; padding: 14px 0 8px; }
  .hopviz-medallion { width: 56px; height: 56px; --ring-radius: 23px; }
  .hopviz-glyph { inset: 9px; font-size: 1.3rem; }
  .hopviz-endpoint { min-width: 0; }
  .hopviz-name { max-width: 80vw; }
  .hopviz-path { width: 100%; min-width: 0; }
  .hopviz-track { height: 110px; }
  .hopviz-track-line {
    left: 50%;
    right: auto;
    top: 6px;
    bottom: 6px;
    width: 2px;
    height: auto;
    transform: translateX(-50%);
    background: linear-gradient(to bottom, transparent, var(--hop-accent) 12%, var(--hop-accent) 88%, transparent);
  }
  .hopviz-relay { left: 50%; top: var(--relay-pos); }
  .hopviz-packet {
    left: 50%;
    top: 0%;
    animation-name: hopviz-packet-y;
  }
}

/* ── Reduced motion: static layout, no loops, no entrance ──────────────── */
@media (prefers-reduced-motion: reduce) {
  .hopviz-chain > * { animation: none; opacity: 1; }
  .hopviz-seg { animation: none; opacity: 0.7; }
  .hopviz-relay-bar { animation: none; }
  .hopviz-packet { display: none; }
}
</style>
