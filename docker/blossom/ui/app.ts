import { sha256 } from 'npm:@noble/hashes@2.0.1/sha2.js';

declare global {
  interface Window {
    nostr?: { getPublicKey(): Promise<string>; signEvent(event: unknown): Promise<Record<string, unknown>> };
    archipelagoNostr?: { selectIdentity?(): Promise<void> };
  }
}
const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const fileInput = element<HTMLInputElement>('file');
const approve = element<HTMLInputElement>('approve');
const upload = element<HTMLButtonElement>('upload');
const refresh = element<HTMLButtonElement>('refresh');
let pubkey = '';
let working = false;
function update() {
  upload.disabled = working || !pubkey || !approve.checked || !fileInput.files?.length;
  refresh.disabled = working || !pubkey;
  fileInput.disabled = working;
  element<HTMLButtonElement>('identity').disabled = working;
}
async function perform(fn: () => Promise<void>) {
  working = true; update(); element('status').textContent = '';
  try { await fn(); } catch (e) { element('status').textContent = e instanceof Error ? e.message : 'Request failed'; }
  finally { working = false; update(); }
}
async function auth(action: string, hash?: string) {
  if (!window.nostr) throw new Error('Archipelago signer is unavailable. Reinstall the app bridge; never enter a private key here.');
  if (await window.nostr.getPublicKey() !== pubkey) throw new Error('Identity changed. Choose your identity and review the file again.');
  const now = Math.floor(Date.now() / 1000);
  const tags = [['t', action], ['expiration', String(now + 300)], ['server', location.hostname]];
  if (hash) tags.push(['x', hash]);
  const signed = await window.nostr.signEvent({ kind: 24242, created_at: now, tags, content: `Authorize local Blossom ${action}` });
  if (signed.pubkey !== pubkey) throw new Error('Signer returned another identity. Nothing was sent.');
  return 'Nostr ' + btoa(JSON.stringify(signed));
}
element('identity').onclick = () => perform(async () => {
  if (!window.nostr) throw new Error('Archipelago signer is unavailable');
  await window.archipelagoNostr?.selectIdentity?.();
  const key = await window.nostr.getPublicKey();
  if (!/^[a-f0-9]{64}$/.test(key)) throw new Error('Invalid signer identity');
  pubkey = key; approve.checked = false;
  element('pubkey').textContent = key; element('files').replaceChildren();
});
fileInput.onchange = () => {
  approve.checked = false;
  const file = fileInput.files?.[0];
  element('file-review').textContent = file ? `${file.name} · ${file.size} bytes · ${file.type || 'unknown type'}` : 'Choose a file up to 16 MiB.';
  update();
};
approve.onchange = update;
upload.onclick = () => perform(async () => {
  const file = fileInput.files?.[0];
  if (!file || !approve.checked || file.size > 16777216) throw new Error('Choose and approve a file up to 16 MiB');
  const bytes = new Uint8Array(await file.arrayBuffer());
  const hash = Array.from(sha256(bytes), b => b.toString(16).padStart(2, '0')).join('');
  const authorization = await auth('upload', hash);
  const response = await fetch('/upload', { method: 'PUT', headers: { Authorization: authorization, 'Content-Type': file.type || 'application/octet-stream' }, body: bytes, credentials: 'same-origin', redirect: 'error' });
  if (!response.ok) throw new Error(`Local upload failed (${response.status}). No external copy was requested.`);
  const descriptor = await response.json();
  if (descriptor.sha256 !== hash || descriptor.size !== bytes.length) throw new Error('Storage returned an unexpected file descriptor');
  approve.checked = false;
  element('status').textContent = `Stored on this node. SHA-256: ${hash}. No Nostr announcement was published.`;
});
refresh.onclick = () => perform(async () => {
  const authorization = await auth('list');
  const response = await fetch(`/list/${pubkey}?limit=100`, { headers: { Authorization: authorization }, credentials: 'same-origin', redirect: 'error' });
  if (!response.ok) throw new Error(`Could not list files (${response.status})`);
  const files = await response.json();
  if (!Array.isArray(files)) throw new Error('Unexpected file list');
  const list = element('files'); list.replaceChildren();
  for (const file of files.slice(0, 100)) {
    if (!/^[a-f0-9]{64}$/.test(file.sha256)) continue;
    const item = document.createElement('li');
    const link = document.createElement('a');
    link.href = '/' + file.sha256; link.download = file.sha256;
    link.textContent = `${file.sha256} · ${file.size} bytes`;
    item.append(link); list.append(item);
  }
});
