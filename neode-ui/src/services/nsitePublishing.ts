import DOMPurify from 'dompurify'
import { rpcClient } from '@/api/rpc-client'
import { publishing } from './publishing'

export interface SignedNsiteEvent { id: string; pubkey: string; kind: number; created_at: number; tags: string[][]; content: string; sig: string }
export interface NsiteReceipt { identity_id: string; server: string; event: SignedNsiteEvent; accepted_relays: string[]; deletion_requested: boolean }
export interface NsiteIdentity { id: string; name: string; nostr_pubkey: string; is_node: boolean }
export interface PreparedNsite { html: string; sha256: string; server: string; identifier: string; authorization: Record<string, unknown>; manifest: Record<string, unknown> }

// Match the platform app signer and its derived Blossom upload allowlist,
// including older node records that do not carry the explicit is_node flag.
function isProfileIdentity(identity: NsiteIdentity): boolean {
  return !identity.is_node && !identity.id.trim().toLowerCase().startsWith('node-')
    && identity.name.trim().toLowerCase() !== 'node' && /^[a-f0-9]{64}$/.test(identity.nostr_pubkey)
}

export function relayAddresses(raw: string): string[] {
  const list = [...new Set(raw.split(/[\s,]+/).filter(Boolean).map(value => {
    const url = new URL(value)
    if (url.protocol !== 'wss:' || url.username || url.password || url.hash || value.length > 300) throw new Error('Use secure wss:// relay URLs without credentials or fragments')
    return url.href
  }))]
  if (!list.length || list.length > 8) throw new Error('Choose between one and eight relays')
  return list
}
export async function nsiteIdentities(): Promise<NsiteIdentity[]> {
  const data = await rpcClient.call<{ identities: NsiteIdentity[] }>({ method: 'identity.list', maxRetries: 0 })
  return data.identities.filter(isProfileIdentity)
}
async function sign(identity: NsiteIdentity, event: Record<string, unknown>): Promise<SignedNsiteEvent> {
  if (!isProfileIdentity(identity)) throw new Error('Choose a profile identity for website files')
  const signed = await rpcClient.call<SignedNsiteEvent>({ method: 'identity.nostr-sign', params: { id: identity.id, event }, maxRetries: 0 })
  if (signed.pubkey !== identity.nostr_pubkey || signed.kind !== event.kind) throw new Error('Signer returned a different identity or event kind')
  for (const key of ['created_at', 'content', 'tags'] as const) {
    if (event[key] !== undefined && JSON.stringify(signed[key]) !== JSON.stringify(event[key])) throw new Error('Signer changed the reviewed event; this event will not be sent')
  }
  return signed
}
export async function storeLocalWebsite(projectId: string, version: number, identity: NsiteIdentity): Promise<void> {
  if (!isProfileIdentity(identity)) throw new Error('Choose a profile identity for local files')
  const prepared = await rpcClient.call<{ authorization: Record<string, unknown> }>({ method: 'publishing.blossom-prepare', params: { id: projectId, version }, maxRetries: 0 })
  const authorization = await sign(identity, prepared.authorization)
  await rpcClient.call({ method: 'publishing.blossom-store', params: { id: projectId, version, authorization }, timeout: 70000, maxRetries: 0 })
}
export function sendToRelay(url: string, event: SignedNsiteEvent): Promise<boolean> {
  return new Promise(resolve => {
    let socket: WebSocket
    try { socket = new WebSocket(url) } catch { resolve(false); return }
    let settled = false
    const finish = (ok: boolean) => { if (settled) return; settled = true; clearTimeout(timer); socket.close(); resolve(ok) }
    const timer = setTimeout(() => finish(false), 15000)
    socket.onopen = () => socket.send(JSON.stringify(['EVENT', event]))
    socket.onerror = () => finish(false)
    socket.onclose = () => finish(false)
    socket.onmessage = message => {
      if (typeof message.data !== 'string' || message.data.length > 65536) return
      try {
        const reply = JSON.parse(message.data)
        if (reply[0] === 'OK' && reply[1] === event.id) finish(reply[2] === true)
      } catch { /* Ignore unrelated relay messages. */ }
    }
  })
}
async function broadcast(event: SignedNsiteEvent, relays: string[]): Promise<string[]> {
  const result = await Promise.all(relays.map(async relay => ({ relay, ok: await sendToRelay(relay, event) })))
  return result.filter(r => r.ok).map(r => r.relay)
}
async function record(projectId: string, receipt: NsiteReceipt): Promise<void> {
  // Persist this operation's receipt without overwriting a newer draft or other
  // transport. Only retry the optimistic conflict, never upload/sign/broadcast.
  for (let attempt = 0; attempt < 3; attempt++) {
    const status = await publishing.status()
    try { await publishing.update(status.state.version, { action: 'record-nsite', id: projectId, receipt }); return }
    catch (error) {
      if (attempt === 2 || !(error instanceof Error) || !error.message.includes('changed')) throw error
    }
  }
}
async function readback(response: Response, expected: Uint8Array): Promise<void> {
  if (!response.ok || !response.body) throw new Error('Uploaded website could not be fetched back')
  const reader = response.body.getReader()
  let offset = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (offset + value.length > expected.length || value.some((byte, index) => byte !== expected[offset + index])) throw new Error('Blossom returned different website bytes')
      offset += value.length
    }
    if (offset !== expected.length) throw new Error('Blossom returned an incomplete website')
  } finally { await reader.cancel() }
}
async function uploadDescriptor(response: Response): Promise<{ sha256: string; size: number }> {
  if (!response.body) throw new Error('Blossom upload receipt is empty')
  const reader = response.body.getReader()
  const bytes = new Uint8Array(8192)
  let size = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (size + value.length > bytes.length) throw new Error('Blossom upload receipt exceeds the size limit')
      bytes.set(value, size); size += value.length
    }
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, size)))
  } finally { await reader.cancel() }
}
export async function prepareNsite(projectId: string, version: number, server: string, savedHtml: string): Promise<PreparedNsite> {
  return await rpcClient.call<PreparedNsite>({ method: 'publishing.nsite-prepare', params: { id: projectId, version, server, html: DOMPurify.sanitize(savedHtml, { WHOLE_DOCUMENT: true, FORBID_TAGS: ['meta', 'base', 'iframe', 'object', 'embed', 'form', 'script', 'link'] }) }, maxRetries: 0 })
}
export async function publishNsite(projectId: string, version: number, identity: NsiteIdentity, relays: string[], p: PreparedNsite): Promise<NsiteReceipt> {
  if (identity.is_node) throw new Error('Use a profile identity, not the operational node identity')
  const current = await publishing.status()
  if (current.state.version !== version) throw new Error('The project changed after review. Review the publication again.')
  const expiry = (p.authorization.tags as string[][] | undefined)?.find(t => t[0] === 'expiration')?.[1]
  if (!expiry || Number(expiry) <= Date.now() / 1000) throw new Error('The review expired. Prepare and review the publication again.')
  const authorization = await sign(identity, p.authorization)
  const bytes = new TextEncoder().encode(p.html)
  const encoded = btoa(String.fromCharCode(...new TextEncoder().encode(JSON.stringify(authorization))))
  const uploaded = await fetch(`${p.server}/upload`, { method: 'PUT', credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(30000), headers: { 'Content-Type': 'text/html; charset=utf-8', 'X-SHA-256': p.sha256, Authorization: `Nostr ${encoded}` }, body: p.html })
  if (uploaded.status === 402) throw new Error('The Blossom server requires payment. No payment was made; choose another server or arrange storage yourself.')
  if (!uploaded.ok) throw new Error(`Blossom upload failed (${uploaded.status}). The server may retain an uploaded copy.`)
  const descriptor = await uploadDescriptor(uploaded)
  if (descriptor.sha256 !== p.sha256 || descriptor.size !== bytes.length) throw new Error('Blossom upload receipt does not match the website. The server may retain a copy.')
  await readback(await fetch(`${p.server}/${p.sha256}`, { credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(30000) }), bytes)
  const event = await sign(identity, p.manifest)
  const receipt: NsiteReceipt = { identity_id: identity.id, server: p.server, event, accepted_relays: [], deletion_requested: false }
  // Save the exact signed manifest before network delivery, for recovery.
  await record(projectId, receipt)
  const delivered = { ...receipt, accepted_relays: await broadcast(event, relays) }
  await record(projectId, delivered)
  if (!delivered.accepted_relays.length) throw new Error('No relay accepted the manifest. The upload and signed manifest were retained; retry delivery from this project.')
  return delivered
}
export async function retryNsite(projectId: string, receipt: NsiteReceipt, relays: string[]): Promise<NsiteReceipt> {
  if (receipt.deletion_requested) throw new Error('Publish explicitly to restore a site after a deletion request')
  const accepted = await broadcast(receipt.event, relays)
  const next = { ...receipt, accepted_relays: [...new Set([...receipt.accepted_relays, ...accepted])] }
  await record(projectId, next)
  if (!accepted.length) throw new Error('No relay accepted this delivery attempt')
  return next
}
export async function requestNsiteDeletion(projectId: string, receipt: NsiteReceipt, identity: NsiteIdentity, relays: string[]): Promise<void> {
  if (identity.id !== receipt.identity_id || identity.nostr_pubkey !== receipt.event.pubkey) throw new Error('Choose the identity that published this nsite')
  const identifier = receipt.event.tags.find(t => t[0] === 'd')?.[1]
  if (!identifier) throw new Error('Missing named-site identifier')
  const event = await sign(identity, { kind: 5, created_at: Math.floor(Date.now() / 1000), content: 'Remove this website manifest', tags: [['e', receipt.event.id], ['a', `35128:${receipt.event.pubkey}:${identifier}`], ['k', '35128']] })
  const accepted = await broadcast(event, [...new Set([...receipt.accepted_relays, ...relays])])
  if (!accepted.length) throw new Error('No relay accepted the deletion request. The nsite may remain available.')
  await record(projectId, { ...receipt, deletion_requested: true })
}
export function namedNsiteUrl(receipt: NsiteReceipt, gateway: string): string {
  const url = new URL(gateway)
  if (url.protocol !== 'https:' || url.username || url.password || url.port || url.search || url.hash || url.pathname !== '/') throw new Error('Enter the gateway HTTPS origin without a path')
  const identifier = receipt.event.tags.find(t => t[0] === 'd')?.[1] ?? ''
  if (!/^[a-z0-9-]{1,13}$/.test(identifier) || identifier.endsWith('-') || !/^[a-f0-9]{64}$/.test(receipt.event.pubkey)) throw new Error('Invalid named nsite')
  const author = BigInt(`0x${receipt.event.pubkey}`).toString(36).padStart(50, '0')
  return `https://${author}${identifier}.${url.hostname}/`
}
