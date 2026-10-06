<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useAuthStore } from "../stores/auth";
import * as chat from "../services/chat";
import type { ChatMessage, ChatProfile, ChatReaction } from "../services/chat";
import { CHAT_STICKERS, stickerContent, unsticker, type ChatSticker } from "../chatStickers";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();

const auth = useAuthStore();
const messages = ref<ChatMessage[]>([]);
const reactions = ref<Record<string, Record<string, string[]>>>({});
const profiles = ref<Record<string, ChatProfile>>({});
const draft = ref("");
const error = ref("");
const loading = ref(false);
const showStickers = ref(false);
const replyTarget = ref<ChatMessage | null>(null);
const listEl = ref<HTMLElement | null>(null);
let sub: { close: () => void } | null = null;

const ownPubkey = computed(() => chat.pubkeyFromNpub(auth.npub));
const canSend = computed(() => auth.hasActiveSigner);
const mediaStickers = computed(() => CHAT_STICKERS.filter((sticker) => sticker.image));
const textStickers = computed(() => CHAT_STICKERS.filter((sticker) => !sticker.image));

onMounted(() => {
  if (auth.isLoggedIn) void start();
});

onUnmounted(() => {
  sub?.close();
});

watch(
  () => auth.isLoggedIn,
  (loggedIn) => {
    if (loggedIn) void start();
    else {
      sub?.close();
      sub = null;
      messages.value = [];
      profiles.value = {};
      reactions.value = {};
      replyTarget.value = null;
    }
  },
);

watch(
  () => props.open,
  (open) => {
    if (open) {
      if (!canSend.value) void auth.restoreSavedSigner();
      void scrollBottom();
    }
  },
);

async function start(): Promise<void> {
  if (sub) return;
  loading.value = true;
  error.value = "";
  try {
    messages.value = await chat.loadChat();
    await loadReactions();
    await loadMissingProfiles();
    sub = chat.subscribeChat(
      (message) => {
        if (!messages.value.some((m) => m.id === message.id)) {
          messages.value.push(message);
          messages.value.sort((a, b) => a.createdAt - b.createdAt);
          void loadMissingProfiles();
          void scrollBottom();
        }
      },
      (reaction) => addReaction(reaction),
    );
    await scrollBottom();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "Chat failed";
  } finally {
    loading.value = false;
  }
}

async function loadReactions(): Promise<void> {
  const loaded = await chat.loadReactions(messages.value.map((m) => m.id));
  reactions.value = {};
  for (const reaction of loaded) addReaction(reaction);
}

async function loadMissingProfiles(): Promise<void> {
  const authors = messages.value.map((m) => m.pubkey);
  if (ownPubkey.value) authors.push(ownPubkey.value);
  const missing = [...new Set(authors)].filter((p) => !profiles.value[p]);
  if (!missing.length) return;
  profiles.value = { ...profiles.value, ...(await chat.loadProfiles(missing)) };
}

async function send(): Promise<void> {
  const content = draft.value.trim();
  if (!content) return;
  await sendContent(content, replyTarget.value ?? undefined);
  draft.value = "";
  replyTarget.value = null;
}

async function sendSticker(sticker: ChatSticker): Promise<void> {
  await sendContent(stickerContent(sticker), replyTarget.value ?? undefined);
  replyTarget.value = null;
  showStickers.value = false;
}

async function sendContent(content: string, replyTo?: ChatMessage): Promise<void> {
  error.value = "";
  try {
    const message = await chat.sendChat(content, replyTo ? { id: replyTo.id, pubkey: replyTo.pubkey } : undefined);
    if (!messages.value.some((m) => m.id === message.id)) messages.value.push(message);
    await loadMissingProfiles();
    await scrollBottom();
  } catch (e) {
    if (e instanceof Error && /connect|signer|auth|missing remote|aborted/i.test(e.message)) {
      await auth.restoreSavedSigner();
    }
    error.value = e instanceof Error ? e.message : "Could not send";
  }
}

async function react(message: ChatMessage, content: string): Promise<void> {
  error.value = "";
  try {
    addReaction(await chat.sendReaction(message, content));
  } catch (e) {
    if (e instanceof Error && /connect|signer|auth|missing remote|aborted/i.test(e.message)) {
      await auth.restoreSavedSigner();
    }
    error.value = e instanceof Error ? e.message : "Could not react";
  }
}

function addReaction(reaction: ChatReaction): void {
  if (!reaction.messageId) return;
  const existing = reactions.value[reaction.messageId]?.[reaction.content] ?? [];
  if (existing.includes(reaction.pubkey)) return;
  reactions.value = {
    ...reactions.value,
    [reaction.messageId]: {
      ...(reactions.value[reaction.messageId] ?? {}),
      [reaction.content]: [...existing, reaction.pubkey],
    },
  };
}

function profile(pubkey: string): ChatProfile {
  return profiles.value[pubkey] ?? { pubkey, name: chat.shortPubkey(pubkey), picture: "" };
}

function timeLabel(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function stickerText(content: string): string | null {
  return unsticker(content)?.text ?? null;
}

function stickerKind(content: string): "sticker" | "gif" | null {
  return unsticker(content)?.kind ?? null;
}

function stickerImage(content: string): string | null {
  return unsticker(content)?.image ?? null;
}

function replyPreview(message: ChatMessage): ChatMessage | null {
  if (!message.replyToId) return null;
  return messages.value.find((m) => m.id === message.replyToId) ?? null;
}

function shortText(content: string): string {
  const sticker = unsticker(content);
  const text = sticker?.text ?? content;
  return text.length > 78 ? `${text.slice(0, 78)}...` : text;
}

function reactionEntries(message: ChatMessage): Array<{ content: string; count: number; mine: boolean }> {
  const own = ownPubkey.value;
  return Object.entries(reactions.value[message.id] ?? {}).map(([content, pubkeys]) => ({
    content,
    count: pubkeys.length,
    mine: !!own && pubkeys.includes(own),
  }));
}

async function scrollBottom(): Promise<void> {
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = listEl.value.scrollHeight;
}
</script>

<template>
  <div v-if="open" class="chat-backdrop" @click="emit('close')" />
  <aside :class="['chat-drawer', { open }]">
    <header>
      <div>
        <div class="label">private nostr-login chat</div>
        <h2 class="glow-cyan">mining desk heckle box</h2>
      </div>
      <button class="thin" aria-label="close chat" @click="emit('close')">×</button>
    </header>

    <div v-if="!canSend" class="notice reconnect">
      <span>
        Logged in, but the signer is not active in this tab. Reading works; sending needs a restored signer.
      </span>
      <button class="thin" :disabled="auth.busy" @click="auth.reconnectRemoteApp()">
        {{ auth.busy ? "opening" : "reconnect" }}
      </button>
    </div>
    <div v-if="error" class="notice err">{{ error }}</div>

    <div ref="listEl" class="messages">
      <div v-if="loading" class="empty muted">loading relay gossip...</div>
      <div v-else-if="!messages.length" class="empty muted">no chat yet. terrifying discipline.</div>
      <article
        v-for="message in messages"
        :key="message.id"
        :class="['msg', { own: message.pubkey === ownPubkey }]"
      >
        <img v-if="profile(message.pubkey).picture" :src="profile(message.pubkey).picture" alt="" />
        <div v-else class="avatar">{{ profile(message.pubkey).name.slice(0, 2).toUpperCase() }}</div>
        <div class="bubble">
          <div class="meta">
            <strong>{{ profile(message.pubkey).name }}</strong>
            <span>{{ timeLabel(message.createdAt) }}</span>
          </div>
          <button
            v-if="replyPreview(message)"
            type="button"
            class="reply-quote"
            @click="replyTarget = replyPreview(message)"
          >
            <strong>{{ profile(replyPreview(message)?.pubkey || '').name }}</strong>
            <span>{{ shortText(replyPreview(message)?.content || '') }}</span>
          </button>
          <p v-if="!stickerText(message.content)">{{ message.content }}</p>
          <div v-else :class="['sticker', stickerKind(message.content)]">
            <img v-if="stickerImage(message.content)" :src="stickerImage(message.content) || ''" alt="" />
            <span>{{ stickerText(message.content) }}</span>
          </div>
          <div class="message-actions">
            <button type="button" @click="replyTarget = message">reply</button>
            <button type="button" @click="react(message, '+')">+</button>
            <button type="button" @click="react(message, '🔥')">fire</button>
            <button type="button" @click="react(message, '😂')">lol</button>
          </div>
          <div v-if="reactionEntries(message).length" class="reactions">
            <button
              v-for="entry in reactionEntries(message)"
              :key="entry.content"
              type="button"
              :class="{ mine: entry.mine }"
              @click="react(message, entry.content)"
            >
              {{ entry.content }} {{ entry.count }}
            </button>
          </div>
        </div>
      </article>
    </div>

    <div v-if="showStickers" class="sticker-modal" @click.self="showStickers = false">
      <section class="sticker-dialog" aria-label="chat stickers">
        <div class="sticker-head">
          <div>
            <div class="label">stickers</div>
            <strong class="glow-amber">{{ CHAT_STICKERS.length }} signed insults</strong>
          </div>
          <button type="button" class="thin" aria-label="close stickers" @click="showStickers = false">×</button>
        </div>
        <div class="sticker-tray">
          <div class="sticker-section">
            <div class="section-title">image / gif stickers</div>
            <div class="sticker-grid media-grid">
              <button
                v-for="sticker in mediaStickers"
                :key="sticker.label"
                type="button"
                :disabled="!canSend"
                @click="sendSticker(sticker)"
              >
                <img :src="sticker.image || ''" alt="" />
                <span>{{ sticker.label }}</span>
                <small>{{ sticker.text }}</small>
              </button>
            </div>
          </div>

          <div class="sticker-section">
            <div class="section-title">text stickers</div>
            <div class="sticker-grid text-grid">
              <button
                v-for="sticker in textStickers"
                :key="sticker.label"
                type="button"
                :disabled="!canSend"
                @click="sendSticker(sticker)"
              >
                <span>{{ sticker.label }}</span>
                <small>{{ sticker.text }}</small>
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>

    <form class="composer" @submit.prevent="send">
      <div v-if="replyTarget" class="replying">
        <div>
          <span class="label">replying to {{ profile(replyTarget.pubkey).name }}</span>
          <p>{{ shortText(replyTarget.content) }}</p>
        </div>
        <button type="button" class="thin" @click="replyTarget = null">×</button>
      </div>
      <div class="composer-tools">
        <button
          type="button"
          class="sticker-toggle"
          :disabled="!canSend"
          @click="showStickers = !showStickers"
        >
          stickers
        </button>
      </div>
      <textarea
        v-model="draft"
        :disabled="!canSend"
        rows="2"
        maxlength="500"
        placeholder="type something irresponsible but signed..."
      />
      <button class="primary" :disabled="!canSend || !draft.trim()">send</button>
    </form>
  </aside>
</template>

<style scoped>
.chat-backdrop {
  position: fixed;
  inset: 0;
  z-index: 18;
  background: rgba(0, 0, 0, 0.2);
}
.chat-drawer {
  position: fixed;
  top: 0;
  right: 0;
  z-index: 20;
  width: min(430px, 92vw);
  height: 100dvh;
  --chat-pad: 16px;
  --composer-bottom: 24px;
  --composer-height: 70px;
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr);
  gap: 12px;
  padding: var(--chat-pad);
  background: rgba(7, 9, 15, 0.97);
  border-left: 1px solid var(--line-bright);
  box-shadow: -16px 0 40px rgba(0, 0, 0, 0.45);
  transform: translateX(102%);
  transition: transform 0.18s ease;
}
.chat-drawer.open {
  transform: translateX(0);
}
header {
  display: flex;
  justify-content: space-between;
  align-items: start;
  gap: 12px;
}
h2 {
  margin: 2px 0 0;
  font-size: 18px;
  letter-spacing: 0;
}
.notice {
  border: 1px solid var(--line);
  color: var(--fg-1);
  padding: 9px 10px;
  font-size: 11px;
  line-height: 1.35;
}
.notice.err {
  border-color: var(--neon-red);
  color: var(--neon-red);
}
.notice.reconnect {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  align-items: center;
}
.notice.reconnect span {
  min-width: 0;
}
.notice.reconnect button {
  flex: 0 0 auto;
}
.messages {
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-right: 4px;
  padding-bottom: calc(var(--composer-height) + var(--composer-bottom) + 18px);
}
.empty {
  padding: 24px 0;
  text-align: center;
  font-size: 12px;
}
.msg {
  display: grid;
  grid-template-columns: 34px 1fr;
  gap: 8px;
  align-items: start;
}
.msg.own {
  grid-template-columns: 1fr 34px;
}
.msg.own img,
.msg.own .avatar {
  grid-column: 2;
  grid-row: 1;
}
.msg.own .bubble {
  grid-column: 1;
  grid-row: 1;
}
img,
.avatar {
  width: 34px;
  height: 34px;
  border: 1px solid var(--line-bright);
  object-fit: cover;
}
.avatar {
  display: grid;
  place-items: center;
  color: var(--neon-cyan);
  background: var(--bg-2);
  font-size: 10px;
  font-weight: 700;
}
.bubble {
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.025);
  padding: 8px 10px;
}
.own .bubble {
  border-color: var(--neon-cyan);
}
.meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: var(--fg-2);
  font-size: 10px;
}
.meta strong {
  color: var(--fg-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.reply-quote {
  width: 100%;
  margin-top: 7px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  border-color: var(--line);
  color: var(--fg-2);
  text-align: left;
  text-transform: none;
  letter-spacing: 0.02em;
  font-size: 10px;
}
.reply-quote strong {
  color: var(--neon-cyan);
}
.reply-quote span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
p {
  margin: 5px 0 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 12px;
}
.message-actions,
.reactions {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-top: 8px;
}
.message-actions button,
.reactions button {
  min-height: 22px;
  padding: 3px 6px;
  color: var(--fg-2);
  border-color: var(--line);
  font-size: 9px;
}
.reactions button {
  color: var(--neon-amber);
  background: rgba(255, 216, 74, 0.04);
}
.reactions button.mine {
  border-color: var(--neon-cyan);
  color: var(--neon-cyan);
}
.sticker {
  border: 1px solid var(--neon-magenta);
  color: var(--neon-amber);
  padding: 10px;
  font-size: 13px;
  line-height: 1.35;
  text-transform: uppercase;
  box-shadow: inset 0 0 18px rgba(255, 61, 240, 0.08);
}
.sticker img {
  display: block;
  width: min(220px, 100%);
  aspect-ratio: 1 / 1;
  height: auto;
  margin-bottom: 8px;
  border: 1px solid var(--line-bright);
  object-fit: contain;
  background: var(--bg-0);
}
.sticker span {
  display: block;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.sticker.gif {
  animation: sticker-bounce 1.4s steps(2, end) infinite;
  background:
    linear-gradient(90deg, rgba(255, 61, 240, 0.12), rgba(41, 255, 230, 0.08)),
    rgba(255, 255, 255, 0.025);
}
.composer {
  position: fixed;
  right: var(--chat-pad);
  bottom: var(--composer-bottom);
  z-index: 4;
  width: calc(min(430px, 92vw) - (var(--chat-pad) * 2));
  min-height: var(--composer-height);
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr) 64px;
  gap: 8px;
  align-items: start;
  padding-top: 10px;
  border-top: 1px solid var(--line);
  background: rgba(7, 9, 15, 0.97);
}
.replying {
  grid-column: 1 / -1;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: start;
  padding: 8px;
  border: 1px solid var(--line);
  background: var(--bg-1);
}
.replying p {
  margin: 3px 0 0;
  color: var(--fg-1);
  font-size: 11px;
  line-height: 1.3;
}
.replying button {
  height: 26px;
  padding: 0 8px;
}
.composer-tools {
  min-width: 0;
}
.sticker-modal {
  position: absolute;
  inset: 0;
  z-index: 3;
  display: grid;
  align-items: end;
  padding: 16px;
  background: var(--bg-0);
}
.sticker-dialog {
  min-height: 0;
  max-height: min(520px, calc(100% - 24px));
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 10px;
  padding: 12px;
  border: 1px solid var(--neon-magenta);
  background: var(--bg-1);
  box-shadow: var(--shadow-magenta);
}
.sticker-head {
  display: flex;
  justify-content: space-between;
  align-items: start;
  gap: 12px;
}
.sticker-head strong {
  display: block;
  margin-top: 2px;
  font-size: 14px;
}
.sticker-tray {
  min-height: 0;
  overflow: auto;
  padding-right: 2px;
}
.sticker-section + .sticker-section {
  margin-top: 14px;
}
.section-title {
  margin-bottom: 8px;
  color: var(--fg-2);
  font-size: 10px;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}
.sticker-grid {
  display: grid;
  gap: 8px;
}
.media-grid {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}
.text-grid {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}
.sticker-grid button {
  min-width: 0;
  min-height: 138px;
  display: flex;
  flex-direction: column;
  gap: 5px;
  align-items: flex-start;
  justify-content: flex-start;
  padding: 9px 10px;
  color: var(--neon-amber);
  border-color: var(--line-bright);
  font-size: 10px;
  text-align: left;
  overflow-wrap: anywhere;
}
.text-grid button {
  min-height: 74px;
}
.sticker-grid button img {
  width: 100%;
  aspect-ratio: 1 / 1;
  min-height: 120px;
  max-height: 156px;
  object-fit: contain;
  border: 1px solid var(--line);
  background: var(--bg-0);
}
.sticker-grid button span {
  color: var(--neon-cyan);
}
.sticker-grid button small {
  color: var(--fg-1);
  font-size: 10px;
  line-height: 1.25;
  text-transform: none;
  letter-spacing: 0.02em;
}
.sticker-toggle {
  width: 100%;
  height: 58px;
  padding: 0 6px;
  color: var(--neon-amber);
  border-color: var(--line-bright);
  font-size: 10px;
}
textarea {
  height: 58px;
  min-height: 58px;
  max-height: 92px;
  resize: none;
  line-height: 1.3;
}
.composer button {
  align-self: start;
  height: 58px;
  padding: 0 8px;
}
@keyframes sticker-bounce {
  0%,
  100% {
    transform: translate(0, 0);
    filter: hue-rotate(0deg);
  }
  50% {
    transform: translate(1px, -1px);
    filter: hue-rotate(22deg);
  }
}
@media (max-width: 760px) {
  .chat-backdrop {
    background: rgba(0, 0, 0, 0.44);
  }
  .chat-drawer {
    top: auto;
    bottom: 0;
    width: 100vw;
    height: 100svh;
    max-height: none;
    --chat-pad: 14px;
    grid-template-rows: auto auto minmax(0, 1fr) auto;
    padding: 14px 14px calc(72px + env(safe-area-inset-bottom));
    transform: translateY(105%);
    border-left: 0;
    border-top: 1px solid var(--line-bright);
  }
  .chat-drawer.open {
    transform: translateY(0);
  }
  .messages {
    gap: 10px;
    padding-bottom: 0;
  }
  .composer {
    position: static;
    width: auto;
    grid-template-columns: 72px minmax(0, 1fr) 58px;
    gap: 6px;
  }
  .sticker-modal {
    padding: 10px;
  }
  .sticker-dialog {
    max-height: calc(100% - 12px);
  }
  .sticker-tray {
    padding-right: 0;
  }
  .media-grid,
  .text-grid {
    grid-template-columns: 1fr;
  }
}
</style>
