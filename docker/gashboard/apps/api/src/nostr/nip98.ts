import { verifyEvent, type Event as NostrEvent } from "nostr-tools";

export type Nip98Input = {
  authHeader: string | undefined;
  method: string;
  fullUrl: string;
};

export type Nip98Result =
  | { ok: true; pubkey: string }
  | { ok: false; reason: string };

const KIND_HTTP_AUTH = 27235;
const FRESHNESS_WINDOW_S = 120;

function findTag(event: NostrEvent, name: string): string | undefined {
  for (const tag of event.tags) {
    if (tag[0] === name && typeof tag[1] === "string") return tag[1];
  }
  return undefined;
}

function timingSafeEqualStr(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

export function verifyNip98(input: Nip98Input): Nip98Result {
  const { authHeader, method, fullUrl } = input;

  if (!authHeader) return { ok: false, reason: "missing_authorization" };
  const match = authHeader.match(/^Nostr\s+(.+)$/i);
  if (!match) return { ok: false, reason: "wrong_scheme" };
  const b64 = match[1]!.trim();

  let event: NostrEvent;
  try {
    const json = Buffer.from(b64, "base64").toString("utf8");
    event = JSON.parse(json) as NostrEvent;
  } catch {
    return { ok: false, reason: "decode_failed" };
  }

  if (event.kind !== KIND_HTTP_AUTH) return { ok: false, reason: "wrong_kind" };
  if (!verifyEvent(event)) return { ok: false, reason: "bad_signature" };

  const nowS = Math.floor(Date.now() / 1000);
  if (Math.abs(nowS - event.created_at) > FRESHNESS_WINDOW_S) {
    return { ok: false, reason: "stale" };
  }

  const tagMethod = findTag(event, "method");
  const tagUrl = findTag(event, "u");
  if (!tagMethod || !tagUrl) return { ok: false, reason: "missing_tags" };

  if (!timingSafeEqualStr(tagMethod.toUpperCase(), method.toUpperCase())) {
    return { ok: false, reason: "method_mismatch" };
  }
  if (!timingSafeEqualStr(tagUrl, fullUrl)) {
    return { ok: false, reason: "url_mismatch" };
  }
  if (!/^[0-9a-f]{64}$/.test(event.pubkey)) {
    return { ok: false, reason: "malformed_pubkey" };
  }

  return { ok: true, pubkey: event.pubkey };
}
