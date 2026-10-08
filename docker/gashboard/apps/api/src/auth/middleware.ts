import type { NextFunction, Request, Response } from "express";
import { unauthorized } from "../errors.js";
import { isPubkeyAllowed } from "../nostr/allowlist.js";
import { verifySession } from "./jwt.js";

declare module "express-serve-static-core" {
  interface Request {
    session?: { pubkey: string; npub: string };
  }
}

export function requireAuth(req: Request, _res: Response, next: NextFunction): void {
  const header = req.header("authorization");
  const match = header?.match(/^Bearer\s+(.+)$/i);
  if (!match) return next(unauthorized("no_session"));

  const session = verifySession(match[1]!.trim());
  if (!session || !isPubkeyAllowed(session.pubkey)) return next(unauthorized("bad_session"));

  req.session = { pubkey: session.pubkey, npub: session.npub };
  next();
}
