<script setup lang="ts">
import { RELEASE_NOTES } from "../releaseNotes";

defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <div v-if="open" class="release-backdrop" @click="emit('close')" />
  <aside :class="['release-drawer', { open }]">
    <header>
      <div>
        <div class="label">release notes</div>
        <h2 class="glow-amber">what changed while the heaters screamed</h2>
      </div>
      <button class="thin" aria-label="close release notes" @click="emit('close')">×</button>
    </header>

    <div class="notes">
      <article v-for="note in RELEASE_NOTES" :key="note.hash" class="note">
        <div class="note-head">
          <strong>{{ note.title }}</strong>
          <code>{{ note.hash }}</code>
        </div>
        <ul>
          <li v-for="bullet in note.bullets" :key="bullet">{{ bullet }}</li>
        </ul>
      </article>
    </div>
  </aside>
</template>

<style scoped>
.release-backdrop {
  position: fixed;
  inset: 0;
  z-index: 18;
  background: rgba(0, 0, 0, 0.22);
}
.release-drawer {
  position: fixed;
  top: 0;
  right: 0;
  z-index: 21;
  width: min(520px, 94vw);
  height: 100vh;
  display: grid;
  grid-template-rows: auto 1fr;
  gap: 14px;
  padding: 16px;
  background: rgba(7, 9, 15, 0.98);
  border-left: 1px solid var(--line-bright);
  box-shadow: -16px 0 40px rgba(0, 0, 0, 0.48);
  transform: translateX(102%);
  transition: transform 0.18s ease;
}
.release-drawer.open {
  transform: translateX(0);
}
header,
.note-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: start;
}
h2 {
  margin: 2px 0 0;
  font-size: 18px;
  letter-spacing: 0;
}
.notes {
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-right: 4px;
}
.note {
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.025);
  padding: 12px;
}
.note-head strong {
  color: var(--neon-cyan);
}
code {
  color: var(--fg-2);
  font-size: 10px;
}
ul {
  margin: 8px 0 0;
  padding-left: 18px;
}
li {
  margin: 5px 0;
  color: var(--fg-1);
  font-size: 12px;
  line-height: 1.4;
}
@media (max-width: 760px) {
  .release-drawer {
    top: auto;
    bottom: 0;
    width: 100vw;
    height: 65vh;
    transform: translateY(105%);
    border-left: 0;
    border-top: 1px solid var(--line-bright);
  }
  .release-drawer.open {
    transform: translateY(0);
  }
}
</style>
