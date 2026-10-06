<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();
const route = useRoute();
const bunkerUri = ref("");
const showBunkerInput = ref(false);
const waitingRemote = ref(false);
const choosingIdentity = ref(false);

async function loginExt(initialLaunch = false): Promise<void> {
  if (auth.busy || choosingIdentity.value) return;
  choosingIdentity.value = true;
  auth.error = null;
  try {
    const native = window.archipelagoNostr;
    if (native && (!initialLaunch || !native.getSelectedIdentity())) {
      await native.selectIdentity();
    }
    await auth.loginExtension();
    void router.push({ name: "dashboard" });
  } catch (error) {
    auth.error = error instanceof Error ? error.message : "Sign-in cancelled";
  } finally {
    choosingIdentity.value = false;
  }
}

async function loginBunker(): Promise<void> {
  if (!bunkerUri.value.trim().startsWith("bunker://")) {
    auth.error = 'URI must start with "bunker://"';
    return;
  }
  try {
    await auth.loginBunker(bunkerUri.value.trim());
    void router.push({ name: "dashboard" });
  } catch {
    /* surfaced via auth.error */
  }
}

async function loginRemoteApp(): Promise<void> {
  waitingRemote.value = true;
  try {
    await auth.loginRemoteApp();
    void router.push({ name: "dashboard" });
  } catch {
    /* surfaced via auth.error */
  } finally {
    waitingRemote.value = false;
  }
}

async function resumeRemoteApp(): Promise<void> {
  if (!auth.hasPendingRemoteAppLogin()) return;
  waitingRemote.value = true;
  try {
    await auth.resumeRemoteAppLogin();
    void router.push({ name: "dashboard" });
  } catch {
    /* surfaced via auth.error */
  } finally {
    waitingRemote.value = false;
  }
}

onMounted(() => {
  if (route.query.remote === "return") {
    void resumeRemoteApp();
  } else if (window.archipelagoNostr && auth.nativeLoginPending && !auth.isLoggedIn) {
    auth.nativeLoginPending = false;
    void loginExt(true);
  }
});
</script>

<template>
  <div class="login">
    <div class="panel">
      <div class="header">
        <div class="title glow-cyan">// auth_required</div>
        <div class="subtitle muted">
          gashboard is a closed dashboard. only allow-listed npubs may pass.
        </div>
      </div>

      <div class="options">
        <button class="primary big" :disabled="auth.busy || choosingIdentity" @click="loginExt(false)">
          {{ auth.busy ? "..." : "sign in with Nostr" }}
          <small>Archipelago identity or browser extension</small>
        </button>

        <button
          class="primary big"
          :disabled="auth.busy || choosingIdentity"
          @click="loginRemoteApp"
        >
          {{ waitingRemote ? "waiting for signer..." : "open signer app" }}
          <small>Primal, Amber, or any Nostr Connect signer</small>
        </button>

        <button
          class="big"
          :disabled="auth.busy || choosingIdentity"
          @click="showBunkerInput = !showBunkerInput"
        >
          bunker URI
          <small>advanced fallback · nsecbunker</small>
        </button>

        <div v-if="showBunkerInput" class="bunker">
          <label class="label">paste bunker URI</label>
          <input
            v-model="bunkerUri"
            placeholder="bunker://<pubkey>?relay=wss://…&secret=…"
            spellcheck="false"
            autocomplete="off"
          />
          <button class="primary" :disabled="auth.busy || choosingIdentity" @click="loginBunker">
            {{ auth.busy ? "connecting…" : "connect" }}
          </button>
        </div>
      </div>

      <div v-if="auth.error" class="err">
        ✗ {{ auth.error }}
      </div>

      <div class="foot muted">
        choose your signer, select your identity, and approve the sign-in request.
      </div>
    </div>
  </div>
</template>

<style scoped>
.login {
  width: min(540px, 92vw);
}
.header { margin-bottom: 24px; }
.title {
  font-size: 22px;
  letter-spacing: 0.18em;
  margin-bottom: 6px;
  font-weight: 700;
}
.subtitle { font-size: 12px; }
.options { display: flex; flex-direction: column; gap: 14px; }
button.big {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 14px 16px;
  font-size: 14px;
}
button.big small {
  font-size: 10.5px;
  font-weight: 400;
  text-transform: none;
  color: var(--fg-2);
  letter-spacing: 0.04em;
}
.bunker {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px dashed var(--line-bright);
  background: rgba(0, 0, 0, 0.25);
}
.err {
  margin-top: 18px;
  padding: 10px 12px;
  border: 1px solid var(--neon-red);
  color: var(--neon-red);
  font-size: 12px;
}
.foot {
  margin-top: 24px;
  font-size: 11px;
  letter-spacing: 0.04em;
}
</style>
