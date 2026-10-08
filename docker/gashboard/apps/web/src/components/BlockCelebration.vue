<script setup lang="ts">
import { ref, watch } from "vue";
import type { DatumSnapshot } from "../types";
import { BLOCK_CELEBRATION_LINES } from "../strings";

const props = defineProps<{ snapshot: DatumSnapshot | null }>();

const showing = ref(false);
const message = ref("");
let lastBlockFoundAt = 0;

watch(
  () => props.snapshot,
  (snap) => {
    if (!snap) return;
    // Block-found field not yet wired — placeholder for when the / homepage
    // parser surfaces lastBlockFoundAt. For now this is dormant; trigger via
    // window.gashboardCelebrate() in the console for a preview.
    void lastBlockFoundAt; // silence unused
  },
);

if (typeof window !== "undefined") {
  (window as unknown as { gashboardCelebrate?: () => void }).gashboardCelebrate = () => {
    const idx = Math.floor(Math.random() * BLOCK_CELEBRATION_LINES.length);
    message.value = BLOCK_CELEBRATION_LINES[idx] ?? "";
    showing.value = true;
    setTimeout(() => (showing.value = false), 8000);
  };
}
</script>

<template>
  <Transition name="fade">
    <div v-if="showing" class="overlay">
      <div class="card glow-magenta">
        <div class="title">★  B L O C K  F O U N D  ★</div>
        <div class="msg">{{ message }}</div>
        <button @click="showing = false">acknowledge</button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.overlay {
  position: fixed; inset: 0;
  background: rgba(7, 9, 15, 0.85);
  backdrop-filter: blur(8px);
  display: grid;
  place-items: center;
  z-index: 1000;
}
.card {
  text-align: center;
  padding: 40px 50px;
  border: 2px solid var(--neon-magenta);
  background: var(--bg-1);
  box-shadow: 0 0 40px var(--neon-magenta);
  display: flex;
  flex-direction: column;
  gap: 18px;
  max-width: 520px;
}
.title {
  font-size: 24px;
  letter-spacing: 0.4em;
  font-weight: 700;
}
.msg {
  font-size: 14px;
  font-style: italic;
  line-height: 1.5;
  color: var(--fg-1);
}
.fade-enter-active, .fade-leave-active { transition: opacity 0.4s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
