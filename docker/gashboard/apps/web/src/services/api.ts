import type { ApiError, ChatSnapshot, DatumSnapshot, LoginResponse } from "../types";
import { buildNip98AuthHeader } from "./nip98";

const TOKEN_KEY = "gashboard.jwt";
const NPUB_KEY = "gashboard.npub";

export function loadToken(): { token: string; npub: string } | null {
  const token = localStorage.getItem(TOKEN_KEY);
  const npub = localStorage.getItem(NPUB_KEY);
  if (!token || !npub) return null;
  return { token, npub };
}

export function saveToken(token: string, npub: string): void {
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(NPUB_KEY, npub);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(NPUB_KEY);
}

async function asError(res: Response): Promise<Error> {
  let body: ApiError | undefined;
  try {
    body = (await res.json()) as ApiError;
  } catch {
    /* ignore */
  }
  const code = body?.error?.code ?? `HTTP_${res.status}`;
  const message = body?.error?.message ?? res.statusText;
  const err = new Error(`${code}: ${message}`);
  (err as Error & { code?: string }).code = code;
  return err;
}

export async function login(): Promise<LoginResponse> {
  const url = `${window.location.origin}/api/auth/login`;
  const auth = await buildNip98AuthHeader("POST", url);
  const res = await fetch(url, {
    method: "POST",
    headers: { authorization: auth, "content-type": "application/json" },
    body: "{}",
  });
  if (!res.ok) throw await asError(res);
  return (await res.json()) as LoginResponse;
}

async function authedGet<T>(path: string): Promise<T> {
  const stored = loadToken();
  if (!stored) throw new Error("Not logged in");
  const res = await fetch(path, {
    headers: { authorization: `Bearer ${stored.token}` },
  });
  if (res.status === 401) {
    clearToken();
    throw new Error("Session expired");
  }
  if (!res.ok) throw await asError(res);
  return (await res.json()) as T;
}

async function authedPost<T>(path: string, body: unknown): Promise<T> {
  const stored = loadToken();
  if (!stored) throw new Error("Not logged in");
  const res = await fetch(path, {
    method: "POST",
    headers: {
      authorization: `Bearer ${stored.token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (res.status === 401) {
    clearToken();
    throw new Error("Session expired");
  }
  if (!res.ok) throw await asError(res);
  return (await res.json()) as T;
}

export async function fetchSnapshot(): Promise<DatumSnapshot> {
  return authedGet<DatumSnapshot>("/api/datum/stats");
}

export async function fetchChatPubkeys(): Promise<string[]> {
  const res = await authedGet<{ pubkeys: string[] }>("/api/auth/chat-config");
  return res.pubkeys;
}

export function fetchChat(): Promise<ChatSnapshot> {
  return authedGet<ChatSnapshot>("/api/chat");
}

export function postChatMessage(body: {
  content: string;
  replyToId?: string;
  replyToPubkey?: string;
}): Promise<ChatSnapshot["messages"][number]> {
  return authedPost<ChatSnapshot["messages"][number]>("/api/chat/messages", body);
}

export function postChatReaction(body: {
  messageId: string;
  content: string;
}): Promise<ChatSnapshot["reactions"][number]> {
  return authedPost<ChatSnapshot["reactions"][number]>("/api/chat/reactions", body);
}

export function postChatKeyWraps(body: {
  wraps: Array<{ recipientPubkey: string; ciphertext: string }>;
}): Promise<{ wraps: ChatSnapshot["keyWraps"] }> {
  return authedPost<{ wraps: ChatSnapshot["keyWraps"] }>("/api/chat/key-wraps", body);
}
