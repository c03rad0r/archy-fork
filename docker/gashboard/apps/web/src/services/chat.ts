import { gcm } from "@noble/ciphers/aes";
import type { Event as NostrEvent } from "nostr-tools";
import { nip19 } from "nostr-tools";
import { SimplePool } from "nostr-tools/pool";
import { fetchChat, fetchChatPubkeys, postChatKeyWraps, postChatMessage, postChatReaction } from "./api";
import { getActiveSigner, hasActiveSigner } from "./signer";
import type { ChatSnapshot } from "../types";

const RELAYS = ["wss://relay.primal.net"];
const POLL_MS = 2500;
const pool = new SimplePool();

export type ChatMessage = {
  id: string;
  pubkey: string;
  content: string;
  createdAt: number;
  replyToId: string;
  replyToPubkey: string;
};

export type ChatReaction = {
  id: string;
  pubkey: string;
  messageId: string;
  content: string;
  createdAt: number;
};

export type ChatProfile = {
  pubkey: string;
  name: string;
  picture: string;
};

let reactionCache: ChatReaction[] = [];
let cachedRoomKey: Uint8Array | null = null;
let cachedRoomKeyRaw = "";

export function clearChatSession(): void {
  cachedRoomKey?.fill(0);
  cachedRoomKey = null;
  cachedRoomKeyRaw = "";
  reactionCache = [];
}

export function canSendChat(): boolean {
  return hasActiveSigner();
}

export function pubkeyFromNpub(npub: string | null): string | null {
  if (!npub) return null;
  try {
    const decoded = nip19.decode(npub);
    return decoded.type === "npub" ? decoded.data : null;
  } catch {
    return null;
  }
}

export async function loadChat(): Promise<ChatMessage[]> {
  const snapshot = await fetchChat();
  await ensureRoomKey(snapshot);
  reactionCache = await Promise.all(snapshot.reactions.map(decryptReaction));
  return (await Promise.all(snapshot.messages.map(decryptMessage))).sort((a, b) => a.createdAt - b.createdAt);
}

export async function loadReactions(messageIds: string[]): Promise<ChatReaction[]> {
  const wanted = new Set(messageIds);
  if (!reactionCache.length) {
    const snapshot = await fetchChat();
    await ensureRoomKey(snapshot);
    reactionCache = await Promise.all(snapshot.reactions.map(decryptReaction));
  }
  return reactionCache.filter((r) => wanted.has(r.messageId));
}

export function subscribeChat(
  onMessage: (message: ChatMessage) => void,
  onReaction?: (reaction: ChatReaction) => void,
): { close: () => void } {
  const seenMessages = new Set<string>();
  const seenReactions = new Set<string>();
  let closed = false;

  async function poll(): Promise<void> {
    if (closed) return;
    const snapshot = await fetchChat();
    await ensureRoomKey(snapshot);
    for (const message of snapshot.messages) {
      if (seenMessages.has(message.id)) continue;
      seenMessages.add(message.id);
      onMessage(await decryptMessage(message));
    }
    for (const reaction of snapshot.reactions) {
      if (seenReactions.has(reaction.id)) continue;
      seenReactions.add(reaction.id);
      onReaction?.(await decryptReaction(reaction));
    }
  }

  const timer = window.setInterval(() => {
    void poll().catch(() => {});
  }, POLL_MS);
  void poll().catch(() => {});

  return {
    close() {
      closed = true;
      window.clearInterval(timer);
    },
  };
}

export async function sendChat(
  content: string,
  replyTo?: { id: string; pubkey: string },
): Promise<ChatMessage> {
  const encrypted = await encryptPayload(content);
  const message = await postChatMessage({
    content: encrypted,
    ...(replyTo ? { replyToId: replyTo.id, replyToPubkey: replyTo.pubkey } : {}),
  });
  return { ...message, content };
}

export async function sendReaction(message: ChatMessage, content: string): Promise<ChatReaction> {
  const encrypted = await encryptPayload(content);
  const reaction = await postChatReaction({ messageId: message.id, content: encrypted });
  return { ...reaction, content };
}

export async function loadProfiles(pubkeys: string[]): Promise<Record<string, ChatProfile>> {
  const authors = [...new Set(pubkeys)].filter(Boolean);
  if (!authors.length) return {};
  const events = await pool.querySync(RELAYS, { kinds: [0], authors }, { maxWait: 2500 });
  const latest = new Map<string, NostrEvent>();
  for (const event of events) {
    const prev = latest.get(event.pubkey);
    if (!prev || event.created_at > prev.created_at) latest.set(event.pubkey, event);
  }
  const out: Record<string, ChatProfile> = {};
  for (const [pubkey, event] of latest) {
    try {
      const meta = JSON.parse(event.content) as { name?: string; display_name?: string; picture?: string };
      out[pubkey] = {
        pubkey,
        name: meta.display_name || meta.name || shortPubkey(pubkey),
        picture: meta.picture || "",
      };
    } catch {
      out[pubkey] = { pubkey, name: shortPubkey(pubkey), picture: "" };
    }
  }
  return out;
}

export function shortPubkey(pubkey: string): string {
  return `${pubkey.slice(0, 8)}:${pubkey.slice(-4)}`;
}

type EncryptedEnvelope = {
  v: 1;
  alg: "aes-gcm";
  room: "gashboard-chat";
  iv: string;
  ciphertext: string;
};

async function encryptPayload(plaintext: string): Promise<string> {
  const key = await ensureRoomKey();
  try {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    // WebCrypto is unavailable on LAN HTTP. Noble preserves the same AES-GCM
    // wire format (ciphertext followed by the 128-bit authentication tag).
    const ciphertext = gcm(key, iv).encrypt(new TextEncoder().encode(plaintext));
    return JSON.stringify({
      v: 1,
      alg: "aes-gcm",
      room: "gashboard-chat",
      iv: base64(iv),
      ciphertext: base64(new Uint8Array(ciphertext)),
    } satisfies EncryptedEnvelope);
  } catch (e) {
    throw new Error(
      e instanceof Error
        ? `Private chat encryption failed: ${e.message}. Reconnect signer if the room key is missing.`
        : "Private chat encryption failed. Reconnect signer if the room key is missing.",
    );
  }
}

async function decryptPayload(value: string): Promise<string> {
  try {
    const envelope = JSON.parse(value) as EncryptedEnvelope;
    if (envelope.v !== 1 || envelope.alg !== "aes-gcm" || envelope.room !== "gashboard-chat") {
      return "[encrypted with unknown chat format]";
    }
    const key = await ensureRoomKey();
    const iv = unbase64(envelope.iv);
    if (iv.length !== 12) throw new Error("Invalid chat nonce");
    const decrypted = gcm(key, iv).decrypt(unbase64(envelope.ciphertext));
    return new TextDecoder().decode(decrypted);
  } catch {
    return "[encrypted - decrypt failed]";
  }
}

async function decryptMessage(message: ChatMessage): Promise<ChatMessage> {
  return { ...message, content: await decryptPayload(message.content) };
}

async function decryptReaction(reaction: ChatReaction): Promise<ChatReaction> {
  return { ...reaction, content: await decryptPayload(reaction.content) };
}

let roomKeyPending: Promise<Uint8Array> | null = null;

async function ensureRoomKey(snapshot?: ChatSnapshot): Promise<Uint8Array> {
  if (roomKeyPending) return roomKeyPending;
  if (cachedRoomKey && !snapshot) return cachedRoomKey;
  roomKeyPending = unlockRoom(snapshot);
  try { return await roomKeyPending; }
  finally { roomKeyPending = null; }
}

async function unlockRoom(snapshot?: ChatSnapshot): Promise<Uint8Array> {
  const signer = getActiveSigner();
  if (!signer) throw new Error("Reconnect your signer to unlock private chat");
  const own = await signer.getPublicKey();
  const current = snapshot ?? await fetchChat();
  const wrap = current.keyWraps.find((item) => item.recipientPubkey === own);
  if (wrap) {
    if (!cachedRoomKey) await importRoomKey(await signer.decrypt(wrap.senderPubkey, wrap.ciphertext));
    const recipients = await fetchChatPubkeys();
    const missing = recipients.filter(key => !current.keyWraps.some(w => w.recipientPubkey === key));
    await publishRoomKey(cachedRoomKeyRaw, missing);
    return cachedRoomKey!;
  }
  if (current.keyWraps.length || current.messages.length || current.reactions.length) {
    throw new Error("Your chat invitation is waiting for its encryption key. Ask an existing chat member to open chat, then reopen this panel.");
  }

  const raw = base64(crypto.getRandomValues(new Uint8Array(32)));
  // Publish our own wrap first to claim an empty room atomically. A competing
  // first visitor must use the winning key, never overwrite it with their own.
  try {
    await postChatKeyWraps({ wraps: [{ recipientPubkey: own, ciphertext: await signer.encrypt(own, raw) }] });
  } catch (error) {
    if ((error as { code?: string }).code?.startsWith("chat_key_")) return unlockRoom();
    throw error;
  }
  await importRoomKey(raw);
  return unlockRoom();
}

async function publishRoomKey(raw: string, recipients: string[]): Promise<void> {
  if (!recipients.length) return;
  const signer = getActiveSigner();
  if (!signer) throw new Error("Reconnect your signer to share private chat key");
  // The API accepts ten recipients per request; invitations support larger rooms.
  for (let offset = 0; offset < recipients.length; offset += 10) {
    const wraps = await Promise.all(recipients.slice(offset, offset + 10).map(async recipientPubkey => ({
      recipientPubkey,
      ciphertext: await signer.encrypt(recipientPubkey, raw),
    })));
    try { await postChatKeyWraps({ wraps }); }
    catch (error) {
      // Another existing member may have shared these recipients during signing.
      // Polling refreshes the remaining recipients without replacing any wrap.
      if ((error as { code?: string }).code !== "chat_key_exists") throw error;
    }
  }
}

async function importRoomKey(raw: string): Promise<Uint8Array> {
  if (cachedRoomKey && cachedRoomKeyRaw === raw) return cachedRoomKey;
  const key = unbase64(raw);
  if (key.length !== 32) throw new Error("Invalid private chat room key");
  cachedRoomKey = key;
  cachedRoomKeyRaw = raw;
  return cachedRoomKey;
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function unbase64(value: string): Uint8Array {
  const binary = atob(value);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}
