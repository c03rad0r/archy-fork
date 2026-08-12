export interface AppStatus {
  id: string
  status: string
  version?: string
}

export interface NodeState {
  timestamp: string
  apps: AppStatus[]
  cpu_usage_percent?: number
  mem_used_bytes?: number
  mem_total_bytes?: number
  disk_used_bytes?: number
  disk_total_bytes?: number
  uptime_secs?: number
  tor_active?: boolean
}

export interface FederatedNode {
  did: string
  pubkey: string
  onion: string
  trust_level: string
  added_at: string
  name?: string
  last_seen?: string
  last_state?: NodeState
  /** bech32 FIPS npub this peer advertised (when known). */
  fips_npub?: string
  /** Transport used on the most recent successful reach: 'fips' | 'tor' | 'mesh' | 'lan'. */
  last_transport?: 'fips' | 'tor' | 'mesh' | 'lan'
  /** RFC 3339 timestamp of last_transport. */
  last_transport_at?: string
  /**
   * Error from the most recent federation sync attempt with this peer, or
   * absent when the last attempt succeeded. Persisted per peer so a stale
   * peer is visibly different from a healthy one instead of the failure
   * living only in the node's debug log (FED-02).
   */
  last_sync_error?: string
  /** RFC 3339 timestamp of last_sync_error. */
  last_sync_error_at?: string
  /**
   * How this peer's trust level came to be what it is. `null` means it was
   * recorded before provenance was tracked — which is exactly the population
   * worth reviewing, since it may include grants made by the fail-open paths
   * that `uninvited-join` / `transitive-merge` now cap at Observer.
   */
  trust_source?: 'invite' | 'uninvited-join' | 'transitive-merge' | 'manual' | null
}

export interface DwnStatus {
  sync_status: string
  last_sync: string | null
  messages_synced: number
  message_count: number
}

export interface SyncResult {
  did: string
  status: string
  apps?: number
  error?: string
}
