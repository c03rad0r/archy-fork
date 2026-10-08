// Two signing paths: NIP-07 browser extension and NIP-46 remote signer
// (covers Primal app + Amber via bunker:// URI). The active signer is held
// in module state — there's only one logged-in user at a time.

import type { Event as NostrEvent, EventTemplate } from "nostr-tools";
import type { Filter } from "nostr-tools/filter";
import { SimplePool } from "nostr-tools/pool";
import { readonly, ref } from "vue";

export type SignerKind = "extension" | "bunker";

export type Signer = {
  kind: SignerKind;
  getPublicKey(): Promise<string>;
  signEvent(template: EventTemplate): Promise<NostrEvent>;
  encrypt(pubkey: string, plaintext: string): Promise<string>;
  decrypt(pubkey: string, ciphertext: string): Promise<string>;
};

const activeSigner = ref<Signer | null>(null);
const NOSTR_CONNECT_RELAYS = ["wss://relay.primal.net"];
const NOSTR_CONNECT_TIMEOUT_MS = 120_000;
const NOSTR_CONNECT_PENDING_KEY = "gashboard.nostrconnect.pending";
const SIGNER_SESSION_KEY = "gashboard.signer.session";
const pool = new SimplePool();

export function getActiveSigner(): Signer | null {
  return activeSigner.value;
}

export function hasActiveSigner(): boolean {
  return activeSigner.value !== null;
}

export function clearSigner(): void {
  activeSigner.value = null;
  clearSavedSignerSession();
}

export const activeSignerState = readonly(activeSigner);

export async function loginWithExtension(): Promise<string> {
  if (!window.nostr) {
    throw new Error("No Nostr signer available. Open Gashboard from Archipelago, or enable a NIP-07 browser extension.");
  }
  const ext = window.nostr;
  const pubkey = await ext.getPublicKey();
  activeSigner.value = {
    kind: "extension",
    getPublicKey: async () => pubkey,
    signEvent: async (template: EventTemplate) =>
      (await ext.signEvent({
        kind: template.kind,
        created_at: template.created_at,
        tags: template.tags,
          content: template.content,
        })) as NostrEvent,
    encrypt: async (target: string, plaintext: string) => {
      if (ext.nip44) return ext.nip44.encrypt(target, plaintext);
      if (ext.nip04) return ext.nip04.encrypt(target, plaintext);
      throw new Error("Your signer cannot encrypt private chat messages");
    },
    decrypt: async (sender: string, ciphertext: string) => {
      if (ext.nip44) return ext.nip44.decrypt(sender, ciphertext);
      if (ext.nip04) return ext.nip04.decrypt(sender, ciphertext);
      throw new Error("Your signer cannot decrypt private chat messages");
    },
  };
  saveSignerSession({ kind: "extension", pubkey });
  return pubkey;
}

export async function loginWithBunker(bunkerUri: string): Promise<string> {
  // Lazy-load applesauce-signers — only pulled when bunker login is attempted
  // so the dashboard works for users who only use NIP-07.
  const mod = await import("applesauce-signers");
  const Ctor =
    (mod as { NostrConnectSigner?: any }).NostrConnectSigner ??
    ((mod as { default?: { NostrConnectSigner?: any } }).default?.NostrConnectSigner);
  if (!Ctor) {
    throw new Error("applesauce-signers does not expose NostrConnectSigner");
  }
  const fromUri = Ctor.fromBunkerURI ?? Ctor.fromURI;
  if (!fromUri) {
    throw new Error("NostrConnectSigner has no fromBunkerURI / fromURI helper");
  }
  const signer = await fromUri.call(Ctor, bunkerUri);
  const pubkey: string = await signer.getPublicKey();
  const remote = signer.remote || pubkey;
  activeSigner.value = {
    kind: "bunker",
    getPublicKey: async () => pubkey,
    signEvent: async (template: EventTemplate) =>
      (await signer.signEvent(template)) as NostrEvent,
    encrypt: async (target: string, plaintext: string) => signer.nip44Encrypt(target, plaintext),
    decrypt: async (sender: string, ciphertext: string) => signer.nip44Decrypt(sender, ciphertext),
  };
  saveSignerSession({
    kind: "bunker",
    key: Array.from(signer.signer.key),
    secret: signer.secret,
    relays: signer.relays,
    remote,
    pubkey,
  });
  return pubkey;
}

export async function loginWithRemoteApp(): Promise<string> {
  return connectRemoteApp({ openApp: true });
}

export async function reconnectRemoteApp(): Promise<string> {
  clearSigner();
  return connectRemoteApp({ openApp: true });
}

export async function resumeRemoteAppLogin(): Promise<string> {
  return connectRemoteApp({ openApp: false });
}

export function hasPendingRemoteAppLogin(): boolean {
  return !!sessionStorage.getItem(NOSTR_CONNECT_PENDING_KEY);
}

async function connectRemoteApp(opts: { openApp: boolean }): Promise<string> {
  const mod = await import("applesauce-signers");
  const NostrConnectSigner =
    (mod as { NostrConnectSigner?: any }).NostrConnectSigner ??
    ((mod as { default?: { NostrConnectSigner?: any } }).default?.NostrConnectSigner);
  const PrivateKeySigner =
    (mod as { PrivateKeySigner?: any }).PrivateKeySigner ??
    ((mod as { default?: { PrivateKeySigner?: any } }).default?.PrivateKeySigner);
  if (!NostrConnectSigner || !PrivateKeySigner) {
    throw new Error("Nostr Connect signer support is unavailable");
  }

  const pending = loadPendingRemoteLogin();
  const clientSigner = pending
    ? new PrivateKeySigner(new Uint8Array(pending.key))
    : new PrivateKeySigner();
  const signer = new NostrConnectSigner({
    relays: pending?.relays ?? NOSTR_CONNECT_RELAYS,
    signer: clientSigner,
    ...(pending ? { secret: pending.secret } : {}),
    subscriptionMethod: subscribeToRelays,
    publishMethod: publishToRelays,
  });

  const permissions = [
    ...(NostrConnectSigner.buildSigningPermissions?.([27235, 4]) ?? ["sign_event:27235", "sign_event:4"]),
    "nip44_encrypt",
    "nip44_decrypt",
  ];
  if (!pending) {
    savePendingRemoteLogin({
      key: Array.from(clientSigner.key),
      secret: signer.secret,
      relays: NOSTR_CONNECT_RELAYS,
    });
  }

  if (opts.openApp) {
    const uri = withCallback(
      signer.getNostrConnectURI({
        name: "gashboard",
        url: window.location.origin,
        permissions,
      }),
    );
    openSignerApp(uri);
  }

  const abort = new AbortController();
  const timeout = window.setTimeout(() => abort.abort(), NOSTR_CONNECT_TIMEOUT_MS);
  try {
    await signer.waitForSigner(abort.signal);
    if (!signer.remote) throw new Error("Remote signer did not complete the connection");
    const pubkey: string = await signer.getPublicKey();
    activeSigner.value = {
      kind: "bunker",
      getPublicKey: async () => pubkey,
      signEvent: async (template: EventTemplate) =>
        (await signer.signEvent(template)) as NostrEvent,
      encrypt: async (target: string, plaintext: string) => signer.nip44Encrypt(target, plaintext),
      decrypt: async (sender: string, ciphertext: string) => signer.nip44Decrypt(sender, ciphertext),
    };
    saveSignerSession({
      kind: "bunker",
      key: Array.from(clientSigner.key),
      secret: signer.secret,
      relays: signer.relays,
      remote: signer.remote,
      pubkey,
    });
    clearPendingRemoteLogin();
    return pubkey;
  } finally {
    window.clearTimeout(timeout);
    if (!signer.isConnected) await signer.close().catch(() => {});
  }
}

export async function restoreSavedSigner(): Promise<boolean> {
  if (activeSigner.value) return true;
  const saved = loadSavedSignerSession();
  if (!saved) return false;

  if (saved.kind === "extension") {
    if (!window.nostr) return false;
    const ext = window.nostr;
    const pubkey = await ext.getPublicKey();
    if (saved.pubkey && saved.pubkey !== pubkey) {
      clearSavedSignerSession();
      return false;
    }
    activeSigner.value = {
      kind: "extension",
      getPublicKey: async () => pubkey,
      signEvent: async (template: EventTemplate) =>
        (await ext.signEvent({
          kind: template.kind,
          created_at: template.created_at,
          tags: template.tags,
            content: template.content,
          })) as NostrEvent,
      encrypt: async (target: string, plaintext: string) => {
        if (ext.nip44) return ext.nip44.encrypt(target, plaintext);
        if (ext.nip04) return ext.nip04.encrypt(target, plaintext);
        throw new Error("Your signer cannot encrypt private chat messages");
      },
      decrypt: async (sender: string, ciphertext: string) => {
        if (ext.nip44) return ext.nip44.decrypt(sender, ciphertext);
        if (ext.nip04) return ext.nip04.decrypt(sender, ciphertext);
        throw new Error("Your signer cannot decrypt private chat messages");
      },
    };
    saveSignerSession({ kind: "extension", pubkey });
    return true;
  }

  const mod = await import("applesauce-signers");
  const NostrConnectSigner =
    (mod as { NostrConnectSigner?: any }).NostrConnectSigner ??
    ((mod as { default?: { NostrConnectSigner?: any } }).default?.NostrConnectSigner);
  const PrivateKeySigner =
    (mod as { PrivateKeySigner?: any }).PrivateKeySigner ??
    ((mod as { default?: { PrivateKeySigner?: any } }).default?.PrivateKeySigner);
  if (!NostrConnectSigner || !PrivateKeySigner) return false;

  const signer = new NostrConnectSigner({
    relays: saved.relays,
    signer: new PrivateKeySigner(new Uint8Array(saved.key)),
    secret: saved.secret,
    remote: saved.remote,
    pubkey: saved.pubkey,
    subscriptionMethod: subscribeToRelays,
    publishMethod: publishToRelays,
  });
  activeSigner.value = {
    kind: "bunker",
    getPublicKey: async () => saved.pubkey,
    signEvent: async (template: EventTemplate) =>
      (await signer.signEvent(template)) as NostrEvent,
    encrypt: async (target: string, plaintext: string) => signer.nip44Encrypt(target, plaintext),
    decrypt: async (sender: string, ciphertext: string) => signer.nip44Decrypt(sender, ciphertext),
  };
  return true;
}

type PendingRemoteLogin = {
  key: number[];
  secret: string;
  relays: string[];
};

type SavedSignerSession =
  | {
      kind: "extension";
      pubkey: string;
    }
  | {
      kind: "bunker";
      key: number[];
      secret: string;
      relays: string[];
      remote: string;
      pubkey: string;
    };

function loadPendingRemoteLogin(): PendingRemoteLogin | null {
  try {
    const raw = sessionStorage.getItem(NOSTR_CONNECT_PENDING_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PendingRemoteLogin;
    if (!Array.isArray(parsed.key) || !parsed.secret || !Array.isArray(parsed.relays)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function savePendingRemoteLogin(pending: PendingRemoteLogin): void {
  sessionStorage.setItem(NOSTR_CONNECT_PENDING_KEY, JSON.stringify(pending));
}

function clearPendingRemoteLogin(): void {
  sessionStorage.removeItem(NOSTR_CONNECT_PENDING_KEY);
}

function loadSavedSignerSession(): SavedSignerSession | null {
  try {
    const raw = localStorage.getItem(SIGNER_SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SavedSignerSession;
    if (parsed.kind === "extension" && parsed.pubkey) return parsed;
    if (
      parsed.kind === "bunker" &&
      Array.isArray(parsed.key) &&
      parsed.secret &&
      Array.isArray(parsed.relays) &&
      parsed.remote &&
      parsed.pubkey
    ) {
      return parsed;
    }
    return null;
  } catch {
    return null;
  }
}

function saveSignerSession(session: SavedSignerSession): void {
  localStorage.setItem(SIGNER_SESSION_KEY, JSON.stringify(session));
}

function clearSavedSignerSession(): void {
  localStorage.removeItem(SIGNER_SESSION_KEY);
}

function withCallback(uri: string): string {
  const separator = uri.includes("?") ? "&" : "?";
  return `${uri}${separator}callback=${encodeURIComponent(`${window.location.origin}/auth/nostr-callback`)}`;
}

function openSignerApp(uri: string): void {
  const isMobile = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(
    navigator.userAgent,
  );
  if (isMobile) {
    window.location.href = uri;
    return;
  }
  const opened = window.open(uri, "_blank", "noopener,noreferrer");
  if (!opened) window.location.href = uri;
}

function subscribeToRelays(relays: string[], filters: Filter[]): AsyncIterable<NostrEvent | string> {
  return {
    [Symbol.asyncIterator]() {
      const queue: Array<NostrEvent | string> = [];
      let wake: (() => void) | null = null;
      let closed = false;
      const sub = pool.subscribeMany(relays, filters, {
        onevent(event) {
          queue.push(event as NostrEvent);
          wake?.();
        },
        onclose(reasons) {
          for (const reason of reasons) queue.push(reason);
          closed = true;
          wake?.();
        },
      });

      return {
        async next(): Promise<IteratorResult<NostrEvent | string>> {
          while (!queue.length && !closed) {
            await new Promise<void>((resolve) => {
              wake = resolve;
            });
            wake = null;
          }
          const value = queue.shift();
          if (value) return { value, done: false };
          return { value: undefined, done: true };
        },
        async return(): Promise<IteratorResult<NostrEvent | string>> {
          closed = true;
          sub.close();
          wake?.();
          return { value: undefined, done: true };
        },
      };
    },
  };
}

async function publishToRelays(relays: string[], event: NostrEvent): Promise<void> {
  const results = await Promise.allSettled(pool.publish(relays, event));
  if (!results.some((r) => r.status === "fulfilled")) {
    throw new Error("Could not publish Nostr Connect request to relay");
  }
}
