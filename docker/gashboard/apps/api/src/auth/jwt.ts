import jwt from "jsonwebtoken";
import { nip19 } from "nostr-tools";
import { config } from "../config.js";

export type SessionPayload = {
  pubkey: string;
  npub: string;
  iat: number;
  exp: number;
};

export type IssuedSession = {
  token: string;
  npub: string;
  expiresAt: number;
};

export function issueSession(hexPubkey: string): IssuedSession {
  const issuedAt = Math.floor(Date.now() / 1000);
  const expiresAt = issuedAt + config.jwt.ttlSeconds;
  const npub = nip19.npubEncode(hexPubkey);
  const token = jwt.sign(
    { pubkey: hexPubkey, npub, iat: issuedAt, exp: expiresAt },
    config.jwt.secret,
    { algorithm: "HS256" },
  );
  return { token, npub, expiresAt };
}

export function verifySession(token: string): SessionPayload | null {
  try {
    const decoded = jwt.verify(token, config.jwt.secret, { algorithms: ["HS256"] });
    if (typeof decoded === "string") return null;
    if (typeof decoded.pubkey !== "string" || typeof decoded.npub !== "string") return null;
    return decoded as SessionPayload;
  } catch {
    return null;
  }
}
