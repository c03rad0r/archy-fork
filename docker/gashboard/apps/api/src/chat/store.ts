import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { AppError } from "../errors.js";

export type StoredChatMessage = {
  id: string;
  pubkey: string;
  content: string;
  createdAt: number;
  replyToId: string;
  replyToPubkey: string;
};

export type StoredChatReaction = {
  id: string;
  pubkey: string;
  messageId: string;
  content: string;
  createdAt: number;
};

export type StoredChatKeyWrap = {
  recipientPubkey: string;
  senderPubkey: string;
  ciphertext: string;
  updatedAt: number;
};

const MAX_MESSAGES = 500;
const MAX_REACTIONS = 2000;

export type ChatSnapshot = {
  messages: StoredChatMessage[];
  reactions: StoredChatReaction[];
  keyWraps: StoredChatKeyWrap[];
};

const pubkey = z.string().regex(/^[0-9a-f]{64}$/);
const savedChat = z.object({
  messages: z.array(z.object({ id: z.string(), pubkey, content: z.string(), createdAt: z.number(), replyToId: z.string(), replyToPubkey: z.string() })).max(MAX_MESSAGES),
  reactions: z.array(z.object({ id: z.string(), pubkey, messageId: z.string(), content: z.string(), createdAt: z.number() })).max(MAX_REACTIONS),
  keyWraps: z.array(z.object({ recipientPubkey: pubkey, senderPubkey: pubkey, ciphertext: z.string(), updatedAt: z.number() })),
});

export class ChatStore {
  private state: ChatSnapshot;

  constructor(private readonly file: string) {
    try {
      this.state = savedChat.parse(JSON.parse(readFileSync(file, "utf8")));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      this.state = { messages: [], reactions: [], keyWraps: [] };
    }
  }

  list(): ChatSnapshot { return structuredClone(this.state); }

  private save(next: ChatSnapshot): void {
    mkdirSync(dirname(this.file), { recursive: true, mode: 0o700 });
    writeFileSync(`${this.file}.tmp`, JSON.stringify(next) + "\n", { mode: 0o600 });
    renameSync(`${this.file}.tmp`, this.file);
    this.state = next;
  }

  addMessage(input: { pubkey: string; content: string; replyToId?: string; replyToPubkey?: string }): StoredChatMessage {
    const message = { ...input, id: randomUUID(), createdAt: Math.floor(Date.now() / 1000), replyToId: input.replyToId ?? "", replyToPubkey: input.replyToPubkey ?? "" };
    this.save({ ...this.state, messages: [...this.state.messages, message].slice(-MAX_MESSAGES) });
    return message;
  }

  addReaction(input: { pubkey: string; messageId: string; content: string }): StoredChatReaction {
    const reaction = { ...input, id: randomUUID(), createdAt: Math.floor(Date.now() / 1000) };
    this.save({ ...this.state, reactions: [...this.state.reactions, reaction].slice(-MAX_REACTIONS) });
    return reaction;
  }

  shareKey(senderPubkey: string, wraps: Array<{ recipientPubkey: string; ciphertext: string }>): StoredChatKeyWrap[] {
    // Only a member who already holds the room key can add recipients. Existing
    // wraps are immutable: concurrent first visits must not replace a room key.
    if (this.state.keyWraps.length && !this.state.keyWraps.some(w => w.recipientPubkey === senderPubkey)) {
      throw new AppError(409, "chat_key_pending", "Ask an existing chat member to open chat and share the room key.");
    }
    if (!this.state.keyWraps.length && !wraps.some(w => w.recipientPubkey === senderPubkey)) {
      throw new AppError(400, "chat_self_wrap_required", "Include your own encrypted room key.");
    }
    if (wraps.some(w => this.state.keyWraps.some(existing => existing.recipientPubkey === w.recipientPubkey))) {
      throw new AppError(409, "chat_key_exists", "The room key is already shared. Reload chat.");
    }
    const added = wraps.map(w => ({ ...w, senderPubkey, updatedAt: Math.floor(Date.now() / 1000) }));
    this.save({ ...this.state, keyWraps: [...this.state.keyWraps, ...added] });
    return added;
  }
}
