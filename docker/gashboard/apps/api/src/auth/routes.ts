import { Router } from "express";
import rateLimit from "express-rate-limit";
import { verifyNip98 } from "../nostr/nip98.js";
import { isPubkeyAllowed, allowedPubkeys, isOwner } from "../nostr/allowlist.js";
import { issueSession } from "./jwt.js";
import { requireAuth } from "./middleware.js";
import { unauthorized, forbidden } from "../errors.js";
import { logger } from "../logger.js";

export const authRouter = Router();

const loginLimiter = rateLimit({
  windowMs: 60_000,
  limit: 10,
  standardHeaders: "draft-7",
  legacyHeaders: false,
  message: { error: { code: "rate_limited", message: "Too many login attempts." } },
});

function fullUrl(req: import("express").Request): string {
  const proto = (req.headers["x-forwarded-proto"] as string | undefined)?.split(",")[0] ?? req.protocol;
  const host = (req.headers["x-forwarded-host"] as string | undefined) ?? req.headers.host;
  return `${proto}://${host}${req.originalUrl}`;
}

authRouter.post("/login", loginLimiter, (req, res, next) => {
  const result = verifyNip98({
    authHeader: req.headers.authorization,
    method: req.method,
    fullUrl: fullUrl(req),
  });
  if (!result.ok) {
    logger.warn({ reason: result.reason }, "nip98_rejected");
    return next(unauthorized("nip98_failed"));
  }
  if (!isPubkeyAllowed(result.pubkey)) {
    logger.warn({ pub: `${result.pubkey.slice(0, 8)}…` }, "pubkey_not_allowlisted");
    return next(forbidden("not_allowlisted"));
  }
  const session = issueSession(result.pubkey);
  res.json({
    token: session.token,
    npub: session.npub,
    expiresAt: session.expiresAt,
  });
});

authRouter.get("/me", requireAuth, (req, res) => {
  res.json({ npub: req.session!.npub, isOwner: isOwner(req.session!.pubkey) });
});

authRouter.get("/chat-config", requireAuth, (_req, res) => {
  res.json({ pubkeys: allowedPubkeys() });
});
