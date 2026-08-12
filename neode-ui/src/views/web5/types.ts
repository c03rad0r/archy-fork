// Shared types for Web5 subcomponents

export interface ProfitsData {
  total_sats: number
  content_sales_sats: number
  routing_fees_sats: number
}

export interface RegisteredNameData {
  id: string
  name: string
  domain: string
  nip05: string
  identity_id: string
  did: string
  nostr_pubkey: string | null
  status: string
  registered_at: string
  expires_at: string | null
}

export interface Nip05Result {
  name: string
  domain: string
  nostr_pubkey: string | null
  relays: string[]
  verified: boolean
}

export interface VCData {
  id: string
  issuer: string
  subject: string
  type: string
  claims: Record<string, unknown>
  issued_at: string
  expires_at: string | null
  status: string
}

export interface NostrRelayData {
  url: string
  connected: boolean
  enabled: boolean
  added_at: string
}

export interface NostrRelayStatsData {
  total_relays: number
  connected_count: number
  enabled_count: number
}

export interface WalletTransaction {
  tx_hash: string
  amount_sats: number
  direction: 'incoming' | 'outgoing'
  num_confirmations: number
  time_stamp: number
  total_fees: number
  dest_addresses: string[]
  label: string
  block_height: number
}

export interface HwWalletDevice {
  type: string
  vendor_id: string
  product_id: string
  manufacturer: string
  product: string
}

export interface ContentItemData {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  description: string
  access: string | { paid: { price_sats: number } }
  added_at: string
}

export interface PeerContentItem {
  id: string
  filename: string
  mime_type: string
  size_bytes: number
  description: string
  access: string | { paid: { price_sats: number } }
}

export interface ConnectionRequest {
  id: string
  from_did: string
  from_onion?: string
  from_pubkey?: string
  message?: string
  created_at: string
}

export interface IdentityProfile {
  display_name?: string
  about?: string
  picture?: string
  banner?: string
  website?: string
  nip05?: string
  lud16?: string
}

export interface ManagedIdentity {
  id: string
  name: string
  purpose: string
  pubkey: string
  did: string
  created_at: string
  is_default: boolean
  nostr_pubkey?: string
  nostr_npub?: string
  profile?: IdentityProfile
}

export interface DwnStatusData {
  running: boolean
  version: string
  sync_status: string
  last_sync: string | null
  messages_synced: number
  storage_bytes: number
  message_count: number
  protocol_count: number
  registered_protocols: string[]
  peer_sync_targets: string[]
}

export interface DwnProtocol {
  protocol: string
  published: boolean
  types: Record<string, unknown>
  structure: Record<string, unknown>
  dateRegistered: string
}

export interface DwnMessageEntry {
  record_id: string
  author: string
  date_created: string
  descriptor: {
    interface: string
    method: string
    protocol?: string
    schema?: string
    dataFormat?: string
  }
  data?: unknown
}

export type VisibilityLevel = 'hidden' | 'discoverable' | 'public'

export type Peer = { onion: string; pubkey: string; name?: string; did?: string }
