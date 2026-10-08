import { Router } from "express";
import { isPubkeyAllowed } from "../nostr/allowlist.js";
import { requireAuth } from "../auth/middleware.js";
import { getSnapshot } from "./poller.js";
import { upstreamUnavailable } from "../errors.js";

export const datumRouter = Router();

datumRouter.use(requireAuth);

datumRouter.get("/stats", (_req, res, next) => {
  const snap = getSnapshot();
  if (!snap) return next(upstreamUnavailable());
  res.json(snap);
});

datumRouter.get("/stream", (req, res) => {
  res.setHeader("Content-Type", "text/event-stream");
  res.setHeader("Cache-Control", "no-cache");
  res.setHeader("Connection", "keep-alive");
  res.flushHeaders?.();

  const send = () => {
    if (!isPubkeyAllowed(req.session!.pubkey)) { res.end(); return; }
    const snap = getSnapshot();
    if (snap) res.write(`data: ${JSON.stringify(snap)}\n\n`);
  };
  send();
  const interval = setInterval(send, 5_000);
  res.on("close", () => clearInterval(interval));
});
