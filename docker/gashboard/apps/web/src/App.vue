<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import { useAuthStore } from "./stores/auth";
import { useStatsStore } from "./stores/stats";
import ChatDrawer from "./components/ChatDrawer.vue";
import ReleaseNotesDrawer from "./components/ReleaseNotesDrawer.vue";

const auth = useAuthStore();
const stats = useStatsStore();
const route = useRoute();
const router = useRouter();
function logout(): void {
  auth.logout();
  void router.push({ name: "login" });
}
const crt = ref(false);
const chatOpen = ref(false);
const releaseOpen = ref(false);
const LOW_HASHRATE_THS = 10;
const TOP_HASHRATE_THS = 70;
const heatLevel = computed(() => {
  const total = stats.snapshot?.pool.combinedHashrateThs ?? 0;
  return Math.max(0, Math.min(1, (total - LOW_HASHRATE_THS) / (TOP_HASHRATE_THS - LOW_HASHRATE_THS)));
});

onMounted(() => {
  const stored = localStorage.getItem("gashboard.crt") === "1";
  crt.value = stored;
  if (stored) document.body.classList.add("crt");
});

function toggleCrt(): void {
  crt.value = !crt.value;
  document.body.classList.toggle("crt", crt.value);
  localStorage.setItem("gashboard.crt", crt.value ? "1" : "0");
}

function shortNpub(n: string): string {
  return n.length > 16 ? `${n.slice(0, 8)}…${n.slice(-4)}` : n;
}

watch(
  heatLevel,
  (level) => {
    document.documentElement.style.setProperty("--heat", level.toFixed(3));
    document.documentElement.style.setProperty("--heat-color", level > 0.75 ? "#ff4f78" : "#ff3df0");
    document.body.classList.toggle("hot-hash", level > 0.75);
  },
  { immediate: true },
);
</script>

<template>
  <header class="topbar">
    <div class="brand glow-magenta flicker">
      ⌬ GASHBOARD
      <span class="muted">// solo lottery dashboard</span>
    </div>
    <nav v-if="auth.isLoggedIn" class="nav">
      <RouterLink to="/">status</RouterLink>
      <RouterLink to="/graphs">graphs</RouterLink>
      <RouterLink to="/access">access</RouterLink>
    </nav>
    <div class="actions">
      <button class="thin" @click="toggleCrt">CRT {{ crt ? "on" : "off" }}</button>
      <button class="thin" @click="releaseOpen = !releaseOpen">notes</button>
      <template v-if="auth.isLoggedIn">
        <button class="thin" @click="chatOpen = !chatOpen">chat</button>
        <span class="muted">{{ auth.npub ? shortNpub(auth.npub) : "" }}</span>
        <button class="thin" @click="logout">logout</button>
      </template>
    </div>
  </header>

  <nav v-if="auth.isLoggedIn" class="mobile-tabbar" aria-label="mobile navigation">
    <RouterLink to="/">status</RouterLink>
    <RouterLink to="/graphs">graphs</RouterLink>
      <RouterLink to="/access">access</RouterLink>
    <button type="button" @click="releaseOpen = !releaseOpen">notes</button>
    <button type="button" @click="chatOpen = !chatOpen">chat</button>
  </nav>

  <main :class="{ shellLogin: route.name === 'login' }">
    <RouterView />
  </main>

  <footer class="footbar muted">
    P(block) is a lifestyle, not a number · gashboard v0.1 · {{ new Date().getFullYear() }}
  </footer>

  <ChatDrawer v-if="auth.isLoggedIn" :open="chatOpen" @close="chatOpen = false" />
  <ReleaseNotesDrawer :open="releaseOpen" @close="releaseOpen = false" />
</template>

<style scoped>
.topbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 24px;
  border-bottom: 1px solid var(--line);
  background: rgba(7, 9, 15, 0.85);
  backdrop-filter: blur(6px);
  position: sticky;
  top: 0;
  z-index: 10;
}
.brand {
  font-weight: 700;
  letter-spacing: 0.18em;
  font-size: 15px;
}
.brand .muted {
  margin-left: 12px;
  letter-spacing: 0.04em;
  font-weight: 400;
  font-size: 11px;
}
.nav {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-left: auto;
  margin-right: 16px;
}
.nav a {
  color: var(--fg-1);
  border: 1px solid var(--line);
  padding: 4px 10px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.nav a:hover,
.nav a.router-link-active {
  border-color: var(--neon-cyan);
  color: var(--neon-cyan);
  box-shadow: var(--shadow-cyan);
}
.mobile-tabbar {
  display: none;
}
.actions {
  display: flex;
  gap: 12px;
  align-items: center;
}
button.thin {
  padding: 4px 10px;
  font-size: 11px;
}
main {
  padding: 24px;
  max-width: 1400px;
  margin: 0 auto;
}
main.shellLogin {
  display: grid;
  place-items: center;
  min-height: calc(100vh - 110px);
}
.footbar {
  padding: 14px 24px;
  border-top: 1px solid var(--line);
  font-size: 11px;
  text-align: center;
  letter-spacing: 0.08em;
}
@media (max-width: 760px) {
  .topbar {
    align-items: center;
    padding: 10px 14px;
  }
  .brand .muted,
  .nav {
    display: none;
  }
  .actions {
    width: auto;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
  }
  .actions button:nth-of-type(n + 2) {
    display: none;
  }
  .actions .muted {
    display: none;
  }
  .mobile-tabbar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 12;
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 0;
    padding-bottom: env(safe-area-inset-bottom);
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
    background: rgba(7, 9, 15, 0.94);
    backdrop-filter: blur(6px);
  }
  .mobile-tabbar a,
  .mobile-tabbar button {
    min-width: 0;
    border: 0;
    border-right: 1px solid var(--line);
    color: var(--fg-1);
    padding: 10px 4px;
    text-align: center;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    background: transparent;
  }
  .mobile-tabbar a:last-child,
  .mobile-tabbar button:last-child {
    border-right: 0;
  }
  .mobile-tabbar a.router-link-active,
  .mobile-tabbar button:hover {
    color: var(--neon-cyan);
    box-shadow: inset 0 -1px 0 var(--neon-cyan), var(--shadow-cyan);
  }
  main {
    padding: 16px 16px calc(76px + env(safe-area-inset-bottom));
  }
  .footbar {
    padding-bottom: calc(76px + env(safe-area-inset-bottom));
  }
}
</style>
