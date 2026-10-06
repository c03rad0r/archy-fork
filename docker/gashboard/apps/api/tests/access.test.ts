import { after, test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { once } from "node:events";
import { finalizeEvent, generateSecretKey, getPublicKey, nip19 } from "nostr-tools";
import { AccessList } from "../src/access/store.js";

const dir = mkdtempSync(join(tmpdir(), "gashboard-access-"));
after(() => rmSync(dir, { recursive: true, force: true }));
const owner = generateSecretKey();
const viewer = generateSecretKey();
const outsider = generateSecretKey();
const ownerHex = getPublicKey(owner);
const viewerHex = getPublicKey(viewer);
const viewerNpub = nip19.npubEncode(viewerHex);

test("membership survives restart; owners cannot be removed and corruption fails closed", () => {
  const file = join(dir, "store.json");
  const store = new AccessList(file, [ownerHex], []);
  store.setViewer(viewerHex, true);
  assert.equal(new AccessList(file, [ownerHex], []).isAllowed(viewerHex), true);
  assert.throws(() => store.setViewer(ownerHex, false));
  store.setViewer(viewerHex, false);
  assert.equal(new AccessList(file, [ownerHex], []).isAllowed(viewerHex), false);
  // Removed platform owners are not retained in the persisted viewer list.
  assert.equal(new AccessList(file, [], []).isAllowed(ownerHex), false);
  writeFileSync(file, "{}");
  assert.throws(() => new AccessList(file, [ownerHex], []));
});

test("signed logins, owner-only invitations, immediate revocation, and provider cache policy", async () => {
  process.env.JWT_SECRET = "local-test-only-secret-32-characters-long";
  process.env.DATUM_ADMIN_PASSWORD = "local-test-only";
  process.env.NOSTR_OWNER_PUBKEYS = ownerHex;
  process.env.ACCESS_LIST_PATH = join(dir, "api-access.json");
  process.env.ARCHIPELAGO_PROVIDER_PATH = join(dir, "provider.js");
  process.env.LOG_LEVEL = "silent";
  writeFileSync(process.env.ARCHIPELAGO_PROVIDER_PATH, "window.nostr = { test: true };");
  const { buildApp } = await import("../src/server.js");
  const server = buildApp().listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const base = `http://127.0.0.1:${address.port}`;
  const call = (path: string, token = "", method = "GET", body?: unknown) => fetch(base + path, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  async function login(key: Uint8Array) {
    const event = finalizeEvent({ kind: 27235, created_at: Math.floor(Date.now() / 1000), tags: [["u", base + "/api/auth/login"], ["method", "POST"]], content: "" }, key);
    return fetch(base + "/api/auth/login", { method: "POST", headers: { authorization: "Nostr " + Buffer.from(JSON.stringify(event)).toString("base64") } });
  }
  try {
    assert.equal((await call("/api/access")).status, 401);
    assert.equal((await login(outsider)).status, 403);
    const ownerLogin = await login(owner);
    assert.equal(ownerLogin.status, 200);
    const ownerToken = (await ownerLogin.json()).token as string;
    assert.equal((await call("/api/access", ownerToken, "POST", { npub: "not-a-key" })).status, 400);
    assert.equal((await call("/api/access", ownerToken, "POST", { npub: viewerNpub })).status, 200);
    const viewerLogin = await login(viewer);
    assert.equal(viewerLogin.status, 200);
    const viewerToken = (await viewerLogin.json()).token as string;
    assert.equal((await call("/api/datum/stats", viewerToken)).status, 200);
    assert.equal((await call("/api/access", viewerToken, "POST", { npub: nip19.npubEncode(getPublicKey(outsider)) })).status, 403);
    assert.deepEqual(await (await call("/api/access", viewerToken)).json(), { isOwner: false, members: [] });
    assert.equal((await call(`/api/access/${nip19.npubEncode(ownerHex)}`, ownerToken, "DELETE")).status, 400);
    const stream = await call("/api/datum/stream", viewerToken);
    const reader = stream.body!.getReader();
    assert.equal((await reader.read()).done, false);
    assert.equal((await call(`/api/access/${viewerNpub}`, ownerToken, "DELETE")).status, 200);
    assert.equal((await reader.read()).done, true);
    assert.equal((await call("/api/datum/stats", viewerToken)).status, 401);
    assert.equal((await call("/api/chat", viewerToken)).status, 401);
    assert.equal((await login(viewer)).status, 403);
    assert.deepEqual(JSON.parse(readFileSync(process.env.ACCESS_LIST_PATH, "utf8")), []);
    const provider = await call("/nostr-provider.js");
    assert.equal(provider.headers.get("cache-control"), "no-cache, no-store");
    assert.match(await provider.text(), /if \(!window.nostr\)/);
    assert.equal((await call("/api/not-real")).status, 404);
  } finally {
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
