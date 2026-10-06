import { Router } from "express";
import { z } from "zod";
import { requireAuth } from "../auth/middleware.js";
import { addMessage, addReaction, listChat, upsertKeyWrap } from "./store.js";

export const chatRouter = Router();

const MessageBody = z.object({
  content: z.string().trim().min(1).max(12000),
  replyToId: z.string().max(128).optional(),
  replyToPubkey: z.string().max(128).optional(),
});

const ReactionBody = z.object({
  messageId: z.string().min(1).max(128),
  content: z.string().trim().min(1).max(12000),
});

const KeyWrapBody = z.object({
  wraps: z.array(z.object({
    recipientPubkey: z.string().regex(/^[0-9a-f]{64}$/),
    ciphertext: z.string().trim().min(1).max(12000),
  })).min(1).max(10),
});

chatRouter.use(requireAuth);

chatRouter.get("/", (_req, res) => {
  res.json(listChat());
});

chatRouter.post("/messages", (req, res) => {
  const body = MessageBody.parse(req.body);
  const message = addMessage({
    pubkey: req.session!.pubkey,
    content: body.content,
    ...(body.replyToId ? { replyToId: body.replyToId } : {}),
    ...(body.replyToPubkey ? { replyToPubkey: body.replyToPubkey } : {}),
  });
  res.status(201).json(message);
});

chatRouter.post("/reactions", (req, res) => {
  const body = ReactionBody.parse(req.body);
  const reaction = addReaction({
    pubkey: req.session!.pubkey,
    messageId: body.messageId,
    content: body.content,
  });
  res.status(201).json(reaction);
});

chatRouter.post("/key-wraps", (req, res) => {
  const body = KeyWrapBody.parse(req.body);
  const wraps = body.wraps.map((wrap) =>
    upsertKeyWrap({
      recipientPubkey: wrap.recipientPubkey,
      senderPubkey: req.session!.pubkey,
      ciphertext: wrap.ciphertext,
    }),
  );
  res.status(201).json({ wraps });
});
