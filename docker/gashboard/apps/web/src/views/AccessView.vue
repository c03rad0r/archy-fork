<script setup lang="ts">
import { onMounted, ref } from "vue";
import { loadToken } from "../services/api";

type Member = { npub: string; role: "owner" | "viewer" };
const members = ref<Member[]>([]);
const owner = ref(false);
const loaded = ref(false);
const busy = ref(false);
const npub = ref("");
const error = ref("");
const notice = ref("");

async function request(method = "GET", key = ""): Promise<void> {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    const token = loadToken()?.token;
    const res = await fetch(`/api/access${method === "DELETE" ? `/${encodeURIComponent(key)}` : ""}`, {
      method,
      headers: { authorization: `Bearer ${token ?? ""}`, "content-type": "application/json" },
      ...(method === "POST" ? { body: JSON.stringify({ npub: key }) } : {}),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error?.message ?? "Could not update access. Please sign in again.");
    owner.value = data.isOwner;
    members.value = data.members;
    loaded.value = true;
    if (method === "POST") { npub.value = ""; notice.value = "Viewer added. They can now sign in with that Nostr identity."; }
    if (method === "DELETE") notice.value = "Viewer removed. Existing sessions no longer have access.";
  } catch (e) {
    error.value = e instanceof Error ? e.message : "Could not load access list.";
  } finally { busy.value = false; }
}

onMounted(() => { void request(); });
</script>

<template>
  <section class="panel access">
    <h1 class="glow-cyan">Your mining crew</h1>
    <p class="muted">Choose who can see your fleet, shares, and dashboard chat.</p>
    <p v-if="error" role="alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <p v-if="!loaded && !error">Loading access list…</p>
    <p v-if="loaded && !owner">Ask a node owner to add or remove people. Your account has viewer access.</p>
    <template v-if="loaded && owner">
      <form @submit.prevent="request('POST', npub.trim())">
        <label for="viewer-npub">Invite a viewer by Nostr public key</label>
        <div class="invite">
          <input id="viewer-npub" v-model="npub" placeholder="npub1…" autocomplete="off" spellcheck="false" required :disabled="busy" />
          <button class="primary" type="submit" :disabled="busy || !npub.trim()">Add viewer</button>
        </div>
        <small class="muted">Viewers can follow mining and join chat. Only node owners can change this list.</small>
      </form>
      <ul>
        <li v-for="member in members" :key="member.npub">
          <div><span class="role">{{ member.role }}</span><code>{{ member.npub }}</code></div>
          <button v-if="member.role === 'viewer'" :disabled="busy" :aria-label="`Remove ${member.npub}`" @click="request('DELETE', member.npub)">Remove</button>
        </li>
      </ul>
      <p class="muted">Owners come from Archipelago identities. Restart Gashboard after changing node identities. Invited viewers stay on this list across restarts.</p>
    </template>
  </section>
</template>

<style scoped>
.access { max-width: 880px; margin: 0 auto; }
h1 { margin-top: 0; }
form { margin: 28px 0; }
label { display: block; margin-bottom: 12px; }
.invite { display: flex; gap: 12px; margin-bottom: 12px; }
input { flex: 1; min-width: 0; }
ul { padding: 0; list-style: none; }
li { display: flex; align-items: center; gap: 16px; padding: 16px 0; border-bottom: 1px solid var(--line); }
li > div { flex: 1; min-width: 0; }
code { display: block; overflow-wrap: anywhere; font-size: 12px; margin-top: 8px; }
.role { color: var(--neon-cyan); text-transform: uppercase; }
[role="alert"] { color: var(--neon-red); }
[role="status"] { color: var(--neon-cyan); }
@media (max-width: 520px) { .invite { flex-direction: column; } }
</style>
