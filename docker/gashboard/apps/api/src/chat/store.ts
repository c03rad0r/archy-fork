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

const messages: StoredChatMessage[] = [];
const reactions: StoredChatReaction[] = [];
const keyWraps: StoredChatKeyWrap[] = [];

export function listChat(): {
  messages: StoredChatMessage[];
  reactions: StoredChatReaction[];
  keyWraps: StoredChatKeyWrap[];
} {
  return { messages: [...messages], reactions: [...reactions], keyWraps: [...keyWraps] };
}

export function addMessage(input: {
  pubkey: string;
  content: string;
  replyToId?: string;
  replyToPubkey?: string;
}): StoredChatMessage {
  const message: StoredChatMessage = {
    id: crypto.randomUUID(),
    pubkey: input.pubkey,
    content: input.content,
    createdAt: Math.floor(Date.now() / 1000),
    replyToId: input.replyToId ?? "",
    replyToPubkey: input.replyToPubkey ?? "",
  };
  messages.push(message);
  if (messages.length > MAX_MESSAGES) messages.splice(0, messages.length - MAX_MESSAGES);
  return message;
}

export function addReaction(input: {
  pubkey: string;
  messageId: string;
  content: string;
}): StoredChatReaction {
  const reaction: StoredChatReaction = {
    id: crypto.randomUUID(),
    pubkey: input.pubkey,
    messageId: input.messageId,
    content: input.content,
    createdAt: Math.floor(Date.now() / 1000),
  };
  reactions.push(reaction);
  if (reactions.length > MAX_REACTIONS) reactions.splice(0, reactions.length - MAX_REACTIONS);
  return reaction;
}

export function upsertKeyWrap(input: {
  recipientPubkey: string;
  senderPubkey: string;
  ciphertext: string;
}): StoredChatKeyWrap {
  const updatedAt = Math.floor(Date.now() / 1000);
  const existing = keyWraps.find((wrap) => wrap.recipientPubkey === input.recipientPubkey);
  if (existing) {
    existing.senderPubkey = input.senderPubkey;
    existing.ciphertext = input.ciphertext;
    existing.updatedAt = updatedAt;
    return existing;
  }
  const wrap: StoredChatKeyWrap = {
    recipientPubkey: input.recipientPubkey,
    senderPubkey: input.senderPubkey,
    ciphertext: input.ciphertext,
    updatedAt,
  };
  keyWraps.push(wrap);
  return wrap;
}
