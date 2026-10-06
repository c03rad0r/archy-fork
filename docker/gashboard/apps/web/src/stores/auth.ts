import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "../services/api";
import * as signer from "../services/signer";

export const useAuthStore = defineStore("auth", () => {
  const stored = api.loadToken();
  const npub = ref<string | null>(stored?.npub ?? null);
  const token = ref<string | null>(stored?.token ?? null);
  const error = ref<string | null>(null);
  const busy = ref(false);
  const restoringSigner = ref(false);

  const isLoggedIn = computed(() => !!token.value);
  const hasActiveSigner = computed(() => signer.activeSignerState.value !== null);

  if (token.value) void restoreSavedSigner();

  async function loginExtension(): Promise<void> {
    error.value = null;
    busy.value = true;
    try {
      await signer.loginWithExtension();
      const r = await api.login();
      token.value = r.token;
      npub.value = r.npub;
      api.saveToken(r.token, r.npub);
    } catch (e) {
      signer.clearSigner();
      error.value = e instanceof Error ? e.message : "Login failed";
      throw e;
    } finally {
      busy.value = false;
    }
  }

  async function loginBunker(uri: string): Promise<void> {
    error.value = null;
    busy.value = true;
    try {
      await signer.loginWithBunker(uri);
      const r = await api.login();
      token.value = r.token;
      npub.value = r.npub;
      api.saveToken(r.token, r.npub);
    } catch (e) {
      signer.clearSigner();
      error.value = e instanceof Error ? e.message : "Login failed";
      throw e;
    } finally {
      busy.value = false;
    }
  }

  async function loginRemoteApp(): Promise<void> {
    error.value = null;
    busy.value = true;
    try {
      await signer.loginWithRemoteApp();
      const r = await api.login();
      token.value = r.token;
      npub.value = r.npub;
      api.saveToken(r.token, r.npub);
    } catch (e) {
      signer.clearSigner();
      error.value = e instanceof Error ? e.message : "Remote signer login failed";
      throw e;
    } finally {
      busy.value = false;
    }
  }

  async function reconnectRemoteApp(): Promise<void> {
    error.value = null;
    busy.value = true;
    try {
      await signer.reconnectRemoteApp();
      const r = await api.login();
      token.value = r.token;
      npub.value = r.npub;
      api.saveToken(r.token, r.npub);
    } catch (e) {
      signer.clearSigner();
      error.value = e instanceof Error ? e.message : "Remote signer reconnect failed";
      throw e;
    } finally {
      busy.value = false;
    }
  }

  async function resumeRemoteAppLogin(): Promise<void> {
    error.value = null;
    busy.value = true;
    try {
      await signer.resumeRemoteAppLogin();
      const r = await api.login();
      token.value = r.token;
      npub.value = r.npub;
      api.saveToken(r.token, r.npub);
    } catch (e) {
      signer.clearSigner();
      error.value = e instanceof Error ? e.message : "Remote signer login failed";
      throw e;
    } finally {
      busy.value = false;
    }
  }

  function logout(): void {
    api.clearToken();
    signer.clearSigner();
    token.value = null;
    npub.value = null;
    error.value = null;
  }

  async function restoreSavedSigner(): Promise<boolean> {
    if (hasActiveSigner.value) return true;
    restoringSigner.value = true;
    try {
      return await signer.restoreSavedSigner();
    } catch (e) {
      error.value = e instanceof Error ? e.message : "Could not restore signer";
      return false;
    } finally {
      restoringSigner.value = false;
    }
  }

  return {
    npub,
    token,
    error,
    busy,
    restoringSigner,
    isLoggedIn,
    hasActiveSigner,
    loginExtension,
    loginBunker,
    loginRemoteApp,
    reconnectRemoteApp,
    resumeRemoteAppLogin,
    restoreSavedSigner,
    hasPendingRemoteAppLogin: signer.hasPendingRemoteAppLogin,
    logout,
  };
});
