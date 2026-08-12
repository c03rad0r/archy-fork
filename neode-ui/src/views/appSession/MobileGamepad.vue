<template>
  <!-- Mobile gamepad overlay — NES-styled D-pad + action buttons.
       Sends postMessage({ type: 'arcade-input', key, player, action }) to iframe. -->
  <div class="mobile-gamepad">
    <!-- D-Pad (left side) -->
    <div class="gamepad-dpad">
      <button
        class="dpad-btn dpad-up"
        @touchstart.prevent="down('ArrowUp')"
        @touchend.prevent="up('ArrowUp')"
        @touchcancel.prevent="up('ArrowUp')"
        aria-label="Up"
      >
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 4l-6 8h12z"/></svg>
      </button>
      <button
        class="dpad-btn dpad-left"
        @touchstart.prevent="down('ArrowLeft')"
        @touchend.prevent="up('ArrowLeft')"
        @touchcancel.prevent="up('ArrowLeft')"
        aria-label="Left"
      >
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M4 12l8-6v12z"/></svg>
      </button>
      <div class="dpad-center" />
      <button
        class="dpad-btn dpad-right"
        @touchstart.prevent="down('ArrowRight')"
        @touchend.prevent="up('ArrowRight')"
        @touchcancel.prevent="up('ArrowRight')"
        aria-label="Right"
      >
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M20 12l-8-6v12z"/></svg>
      </button>
      <button
        class="dpad-btn dpad-down"
        @touchstart.prevent="down('ArrowDown')"
        @touchend.prevent="up('ArrowDown')"
        @touchcancel.prevent="up('ArrowDown')"
        aria-label="Down"
      >
        <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 20l6-8H6z"/></svg>
      </button>
    </div>

    <!-- Center: START / SELECT + utility buttons -->
    <div class="gamepad-meta">
      <div class="meta-row">
        <button
          class="meta-btn"
          @touchstart.prevent="tap('Escape')"
          aria-label="Select"
        >SEL</button>
        <button
          class="meta-btn"
          @touchstart.prevent="tap('Enter')"
          aria-label="Start"
        >START</button>
      </div>
      <div class="meta-utility">
        <button class="util-btn" aria-label="Refresh" @click="$emit('refresh')">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" width="14" height="14">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
        <button class="util-btn" aria-label="Open in browser" @click="$emit('openBrowser')">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" width="14" height="14">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
        </button>
        <button class="util-btn" aria-label="Close" @click="$emit('close')">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" width="14" height="14">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Action buttons (right side) — triangle layout -->
    <div class="gamepad-actions">
      <button
        class="action-btn action-c"
        @touchstart.prevent="down('c')"
        @touchend.prevent="up('c')"
        @touchcancel.prevent="up('c')"
        aria-label="Special"
      ></button>
      <div class="action-row">
        <button
          class="action-btn action-b"
          @touchstart.prevent="down('b')"
          @touchend.prevent="up('b')"
          @touchcancel.prevent="up('b')"
          aria-label="Kick"
        ></button>
        <button
          class="action-btn action-a"
          @touchstart.prevent="down('a')"
          @touchend.prevent="up('a')"
          @touchcancel.prevent="up('a')"
          aria-label="Punch"
        ></button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const props = defineProps<{
  iframeRef: HTMLIFrameElement | null
  player?: number
}>()

defineEmits<{
  refresh: []
  openBrowser: []
  close: []
}>()

function send(key: string, action: 'down' | 'up') {
  props.iframeRef?.contentWindow?.postMessage(
    { type: 'arcade-input', key, player: props.player ?? 1, action },
    '*'
  )
}

function down(key: string) { send(key, 'down') }
function up(key: string) { send(key, 'up') }
function tap(key: string) { send(key, 'down'); setTimeout(() => send(key, 'up'), 80) }
</script>

<style scoped>
.mobile-gamepad {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  padding: 12px 20px;
  padding-bottom: calc(12px + var(--safe-area-bottom, env(safe-area-inset-bottom, 0px)));
  background: rgba(0, 0, 0, 0.85);
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
}

/* ── D-Pad ── */
.gamepad-dpad {
  display: grid;
  grid-template-columns: 48px 48px 48px;
  grid-template-rows: 48px 48px 48px;
  gap: 2px;
  flex-shrink: 0;
}

.dpad-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: rgba(255, 255, 255, 0.7);
  transition: background 0.1s;
}
.dpad-btn:active {
  background: rgba(251, 146, 60, 0.3);
  color: white;
}
.dpad-btn svg {
  width: 20px;
  height: 20px;
}
.dpad-up    { grid-column: 2; grid-row: 1; }
.dpad-left  { grid-column: 1; grid-row: 2; }
.dpad-center {
  grid-column: 2;
  grid-row: 2;
  background: rgba(255, 255, 255, 0.03);
  border-radius: 4px;
}
.dpad-right { grid-column: 3; grid-row: 2; }
.dpad-down  { grid-column: 2; grid-row: 3; }

/* ── Meta buttons (START / SELECT) ── */
.gamepad-meta {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
}

.meta-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.meta-utility {
  display: flex;
  gap: 6px;
  align-items: center;
}

.util-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: rgba(255, 255, 255, 0.35);
  transition: background 0.15s, color 0.15s;
}
.util-btn:active {
  background: rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.7);
}

.meta-btn {
  padding: 6px 16px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.45);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1.5px;
  text-transform: uppercase;
}
.meta-btn:active {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.8);
}

/* ── Action buttons — triangle layout ── */
.gamepad-actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex-shrink: 0;
  gap: 6px;
}

.action-row {
  display: flex;
  gap: 10px;
}

.action-btn {
  width: 50px;
  height: 50px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 2px solid;
  transition: background 0.1s, transform 0.1s;
}
.action-btn:active {
  transform: scale(0.92);
}

.action-a {
  background: rgba(251, 146, 60, 0.2);
  border-color: rgba(251, 146, 60, 0.5);
  color: #fb923c;
}
.action-a:active {
  background: rgba(251, 146, 60, 0.45);
}

.action-b {
  background: rgba(96, 165, 250, 0.2);
  border-color: rgba(96, 165, 250, 0.5);
  color: #60a5fa;
}
.action-b:active {
  background: rgba(96, 165, 250, 0.45);
}

.action-c {
  background: rgba(255, 255, 255, 0.12);
  border-color: rgba(255, 255, 255, 0.4);
  color: #ffffff;
}
.action-c:active {
  background: rgba(255, 255, 255, 0.3);
}
</style>
