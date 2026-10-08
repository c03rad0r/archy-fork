import { after, test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

process.env.JWT_SECRET = "local-test-only-secret-32-characters-long";
process.env.DATUM_ADMIN_PASSWORD = "local-test-only";
process.env.NOSTR_OWNER_PUBKEYS = "a".repeat(64);
const { ChatStore } = await import("../src/chat/store.js");
const dir = mkdtempSync(join(tmpdir(), "gashboard-chat-"));
after(() => rmSync(dir, { recursive: true, force: true }));
const owner = "a".repeat(64);
const viewer = "b".repeat(64);
const newcomer = "c".repeat(64);

test("encrypted history, reactions and key wraps survive restart with private permissions", () => {
  const file = join(dir, "chat.json");
  const store = new ChatStore(file);
  store.shareKey(owner, [{ recipientPubkey: owner, ciphertext: "encrypted-key" }]);
  const message = store.addMessage({ pubkey: owner, content: "encrypted-message" });
  store.addReaction({ pubkey: owner, messageId: message.id, content: "encrypted-reaction" });
  assert.deepEqual(new ChatStore(file).list(), store.list());
  assert.equal(statSync(file).mode & 0o777, 0o600);
  const copy = store.list();
  copy.keyWraps[0]!.ciphertext = "changed";
  assert.equal(store.list().keyWraps[0]!.ciphertext, "encrypted-key");
  writeFileSync(file, "{}");
  assert.throws(() => new ChatStore(file));
});

test("new invitations and simultaneous first visits cannot overwrite the room key", () => {
  const store = new ChatStore(join(dir, "invites.json"));
  store.shareKey(owner, [{ recipientPubkey: owner, ciphertext: "original-key" }]);
  assert.throws(() => store.shareKey(viewer, [{ recipientPubkey: viewer, ciphertext: "different-key" }]), /existing chat member/);
  store.shareKey(owner, [{ recipientPubkey: viewer, ciphertext: "same-key-for-viewer" }]);
  assert.throws(() => store.shareKey(viewer, [{ recipientPubkey: viewer, ciphertext: "replacement-key" }]), /already shared/);
  store.shareKey(viewer, [{ recipientPubkey: newcomer, ciphertext: "same-key-for-newcomer" }]);
  assert.equal(store.list().keyWraps[0]!.ciphertext, "original-key");
  assert.equal(store.list().keyWraps.length, 3);
});

test("failed persistence does not report or retain an unsaved message", () => {
  const parent = join(dir, "blocked");
  const store = new ChatStore(join(parent, "chat.json"));
  writeFileSync(parent, "not a directory");
  assert.throws(() => store.addMessage({ pubkey: owner, content: "ciphertext" }));
  assert.deepEqual(store.list().messages, []);
  assert.equal(readFileSync(parent, "utf8"), "not a directory");
});
