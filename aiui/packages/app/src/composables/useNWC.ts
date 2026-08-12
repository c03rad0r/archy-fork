import { ref } from 'vue'
import { storeApiKey, getApiKey, deleteApiKey } from '@/utils/key-vault'

const VAULT_PROVIDER = 'nwc'

export interface NWCConnection {
  relayUrl: string
  walletPubkey: string
  secret: string
}

const isConnected = ref(false)
const balance = ref<number | null>(null)
const connectionString = ref<string | null>(null)

async function loadConnection(): Promise<NWCConnection | null> {
  try {
    const stored = await getApiKey(VAULT_PROVIDER)
    if (stored) {
      connectionString.value = stored
      isConnected.value = true
      return parseConnectionString(stored)
    }
  } catch { /* ignore */ }
  return null
}

function parseConnectionString(str: string): NWCConnection | null {
  // Format: nostr+walletconnect://pubkey?relay=wss://...&secret=hex
  try {
    const url = new URL(str)
    const walletPubkey = url.hostname || url.pathname.replace('//', '')
    const relayUrl = url.searchParams.get('relay') ?? ''
    const secret = url.searchParams.get('secret') ?? ''
    if (!walletPubkey || !relayUrl || !secret) return null
    return { relayUrl, walletPubkey, secret }
  } catch {
    return null
  }
}

export function useNWC() {
  async function connect(nwcString: string): Promise<boolean> {
    const parsed = parseConnectionString(nwcString)
    if (!parsed) return false

    await storeApiKey(VAULT_PROVIDER, nwcString)
    connectionString.value = nwcString
    isConnected.value = true
    return true
  }

  async function disconnect() {
    await deleteApiKey(VAULT_PROVIDER)
    connectionString.value = null
    isConnected.value = false
    balance.value = null
  }

  async function getBalance(): Promise<number | null> {
    const conn = await loadConnection()
    if (!conn) return null

    // NIP-47: Send get_balance request via relay
    // This is a simplified version — full implementation needs NIP-47 event signing
    return null
  }

  async function payInvoice(bolt11: string): Promise<{ success: boolean; preimage?: string; error?: string }> {
    const conn = await loadConnection()
    if (!conn) return { success: false, error: 'Not connected' }

    // NIP-47: Send pay_invoice request via relay
    // Full implementation requires NIP-47 event creation and signing with the secret
    return { success: false, error: 'NWC pay requires NIP-47 event signing' }
  }

  // Load on init
  loadConnection()

  return {
    isConnected,
    balance,
    connectionString,
    connect,
    disconnect,
    getBalance,
    payInvoice,
  }
}
