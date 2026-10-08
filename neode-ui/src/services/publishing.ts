import { rpcClient } from '@/api/rpc-client'
import type { NsiteReceipt } from './nsitePublishing'

export type PublishRoute = 'fips' | 'public-web' | 'tor' | 'nostr'
export interface PublishDomain { hostname: string; destination: string | null }
export interface WebsiteRevision { id: string; created_at: string; html: string }
export interface WebsiteProject {
  id: string; name: string; routes: PublishRoute[]; domain: PublishDomain | null
  draft: string; revisions: WebsiteRevision[]
  fips_publication?: { port: number; html: string; created_at: string; public_archive?: string | null; nsite_asset?: { html: string; receipt: { sha256: string; size: number } } | null } | null
  tor_publication?: { port: number; html: string; created_at: string; public_archive?: string | null; nsite_asset?: { html: string; receipt: { sha256: string; size: number } } | null } | null
  nsite_receipt?: NsiteReceipt | null
  local_archive?: { sha256: string; size: number; pubkey: string; created_at: string } | null
}
export interface PublishingState {
  schema: number; version: number; connections: PublishRoute[]; projects: Record<string, WebsiteProject>
}
export interface PublishingStatus {
  state: PublishingState; fips_address: string | null; publication_enabled: boolean; public_archive_enabled?: boolean; notice: string
  listeners?: { project_id: string; address: string | null; listening: boolean; externally_verified: boolean; error: string | null }[]
  onions?: { project_id: string; onion_address: string | null; listening: boolean; externally_verified: boolean; error: string | null }[]
  gateway?: { configured: boolean; host?: string; port?: number; domains?: string[]; certificate_mode?: string; routes: { id: string; domain: string }[]; error?: string }
  nostr_relays?: string[]
  apps: { id: string; name: string; port: number; authentication: string; listener_claimed: boolean; guest_access?: boolean }[]
  grants?: { id: string; label: string; apps: string[]; expires_at: number | null }[]
}
export interface DnsPlan {
  records: { record_type: string; name: string; value: string; ttl: number }[]
  verified: boolean; notes: string[]; instructions_url: string
}
export interface HttpsCheck { hostname: string; sha256: string; checked_at: string }
export const PUBLISH_ROUTES: { id: PublishRoute; title: string; description: string }[] = [
  { id: 'fips', title: 'FIPS network', description: 'Use your node’s FIPS address without a public gateway.' },
  { id: 'public-web', title: 'Public web', description: 'Use a domain and a reverse proxy you control.' },
  { id: 'tor', title: 'Tor', description: 'Your node controls its onion address and keeps it when you unpublish.' },
  { id: 'nostr', title: 'Nostr / nsites', description: 'Share a signed static copy through Nostr and Blossom.' },
]
export const publishing = {
  status: () => rpcClient.call<PublishingStatus>({ method: 'publishing.status', maxRetries: 1 }),
  verifyHttps: (id: string, version: number) => rpcClient.call<HttpsCheck>({ method: 'publishing.verify-https', params: { id, version }, timeout: 30000, maxRetries: 0 }),
  update: (version: number, change: Record<string, unknown>) => rpcClient.call<{state: PublishingState; project_id: string | null}>({
    method: 'publishing.update', params: { version, change }, maxRetries: 0,
  }),
  dns: (domain: PublishDomain) => rpcClient.call<DnsPlan>({ method: 'publishing.dns', params: { ...domain }, maxRetries: 0 }),
  generate: (prompt: string, model: string) => rpcClient.call<{html: string; provider: string}>({
    method: 'publishing.generate', params: { prompt, model }, timeout: 150000, maxRetries: 0,
  }),
}

// This document is also sandboxed with no allow-* tokens by its iframe. The CSP
// precedes model content and cannot be relaxed by a second meta tag. No fetches,
// scripts, forms, navigation of the parent, cookies or management origin access.
export function websitePreview(html: string): string {
  return '<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; style-src \'unsafe-inline\'; img-src data:; font-src \'none\'; base-uri \'none\'; form-action \'none\'"><meta name="referrer" content="no-referrer"></head><body>' + html + '</body></html>'
}
