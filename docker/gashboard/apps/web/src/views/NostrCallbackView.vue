<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();

onMounted(async () => {
  if (auth.hasPendingRemoteAppLogin()) {
    try {
      await auth.resumeRemoteAppLogin();
    } catch {
      /* surfaced on the login screen if resume fails */
    }
    void router.replace({ name: auth.isLoggedIn ? "dashboard" : "login", query: auth.isLoggedIn ? {} : { remote: "return" } });
    return;
  }
  if (auth.isLoggedIn) {
    void router.replace({ name: "dashboard" });
    return;
  }
  void router.replace({ name: "login", query: { remote: "return" } });
});
</script>

<template>
  <div class="callback panel">
    <div class="title glow-cyan">// signer_return</div>
    <p class="muted">Finishing remote signer login...</p>
  </div>
</template>

<style scoped>
.callback {
  width: min(460px, 92vw);
  text-align: center;
}
.title {
  font-size: 20px;
  font-weight: 700;
  letter-spacing: 0.16em;
  margin-bottom: 8px;
}
p {
  margin: 0;
  font-size: 12px;
}
</style>
