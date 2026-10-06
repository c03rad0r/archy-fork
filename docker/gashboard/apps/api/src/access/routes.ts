import { Router } from "express";
import { nip19 } from "nostr-tools";
import { requireAuth } from "../auth/middleware.js";
import { accessList } from "../nostr/allowlist.js";
import { badRequest, forbidden } from "../errors.js";

export const accessRouter = Router();
accessRouter.use(requireAuth);
accessRouter.get("/", (req, res) => {
  const isOwner = accessList.isOwner(req.session!.pubkey);
  res.setHeader("Cache-Control", "no-store");
  res.json({ isOwner, members: isOwner ? accessList.members() : [] });
});
accessRouter.use((req, _res, next) => {
  if (!accessList.isOwner(req.session!.pubkey)) return next(forbidden("owner_required"));
  next();
});

function publicKey(value: unknown): string {
  if (typeof value !== "string" || value.length > 100) throw badRequest("invalid_npub", "Enter a valid npub public key.");
  try {
    const decoded = nip19.decode(value.trim());
    if (decoded.type === "npub") return decoded.data;
  } catch { /* Map decoding failures to a client error. */ }
  throw badRequest("invalid_npub", "Enter a valid npub public key.");
}

function update(npub: unknown, enabled: boolean): void {
  const pubkey = publicKey(npub);
  if (accessList.isOwner(pubkey)) throw badRequest("node_owner", "Manage node owners in Archipelago identities.");
  if (enabled && !accessList.isAllowed(pubkey) && accessList.members().filter(m => m.role === "viewer").length >= 500) {
    throw badRequest("list_full", "The access list is limited to 500 viewers.");
  }
  accessList.setViewer(pubkey, enabled);
}

accessRouter.post("/", (req, res) => {
  update(req.body?.npub, true);
  res.json({ isOwner: true, members: accessList.members() });
});
accessRouter.delete("/:npub", (req, res) => {
  update(req.params.npub, false);
  res.json({ isOwner: true, members: accessList.members() });
});
