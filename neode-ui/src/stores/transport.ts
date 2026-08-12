// Pinia store for transport layer state (unified routing: mesh > lan > tor)
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'

export type TransportKind = 'mesh' | 'lan' | 'fips' | 'tor'

export interface TransportInfo {
  kind: TransportKind
  available: boolean
}

export interface TransportStatus {
  transports: TransportInfo[]
  mesh_only: boolean
  peer_count: number
}

export interface TransportPeer {
  did: string
  pubkey_hex: string
  name: string | null
  trust_level: string | null
  mesh_contact_id: number | null
  lan_address: string | null
  onion_address: string | null
  preferred_transport: TransportKind
  available_transports: TransportKind[]
  last_seen: string | null
}

export const useTransportStore = defineStore('transport', () => {
  const status = ref<TransportStatus | null>(null)
  const peers = ref<TransportPeer[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  const meshOnly = computed(() => status.value?.mesh_only ?? false)

  const availableTransports = computed(() =>
    (status.value?.transports ?? []).filter((t) => t.available).map((t) => t.kind)
  )

  async function fetchStatus() {
    try {
      loading.value = true
      error.value = null
      const res = await rpcClient.call<TransportStatus>({ method: 'transport.status', dedup: true })
      status.value = res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to fetch transport status'
    } finally {
      loading.value = false
    }
  }

  async function fetchPeers() {
    try {
      const res = await rpcClient.call<{ peers: TransportPeer[] }>({
        method: 'transport.peers',
        dedup: true,
      })
      peers.value = res.peers
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to fetch transport peers'
    }
  }

  async function sendMessage(did: string, payload: string) {
    try {
      error.value = null
      const res = await rpcClient.call<{ sent: boolean; transport_used: TransportKind }>({
        method: 'transport.send',
        params: { did, payload },
      })
      return res
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to send via transport'
      throw err
    }
  }

  async function setMeshOnly(enabled: boolean) {
    try {
      error.value = null
      await rpcClient.call<{ mesh_only: boolean; configured: boolean }>({
        method: 'transport.set-mode',
        params: { mesh_only: enabled },
      })
      await fetchStatus()
    } catch (err: unknown) {
      error.value = err instanceof Error ? err.message : 'Failed to set transport mode'
      throw err
    }
  }

  async function refreshAll() {
    await Promise.all([fetchStatus(), fetchPeers()])
  }

  return {
    status,
    peers,
    loading,
    error,
    meshOnly,
    availableTransports,
    fetchStatus,
    fetchPeers,
    sendMessage,
    setMeshOnly,
    refreshAll,
  }
})
