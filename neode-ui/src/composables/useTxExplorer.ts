// Shared "open this transaction in an explorer" logic (2026-07-22).
//
// Nodes with a PRUNED bitcoin node can't run the Mempool app at all, but the
// wallet UI's tx links used to blindly open the local app anyway (blank app
// frame). Routing:
//   - local Mempool app running  → open it, exactly as before
//   - otherwise                  → an EXTERNAL explorer, after a one-time
//     consent modal: viewing a tx on someone else's mempool tells that
//     server's operator which transaction you're interested in (plus your
//     IP), so the user must knowingly opt in and may point the link at
//     their own trusted instance instead.
//
// Preference + acknowledgement persist per browser in localStorage
// (`archipelago.tx-explorer.v1`); the Settings → System section edits the
// same values.
import { ref } from 'vue'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useContainerStore } from '@/stores/container'

export const DEFAULT_TX_EXPLORER = 'https://tx1138.com'
export const EXPLORER_PLACEHOLDER = 'https://mempool.guide'

const KEY = 'archipelago.tx-explorer.v1'

interface TxExplorerPrefs {
  url: string
  acknowledged: boolean
}

function loadPrefs(): TxExplorerPrefs {
  const defaults: TxExplorerPrefs = { url: DEFAULT_TX_EXPLORER, acknowledged: false }
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(KEY) || '{}') }
  } catch {
    return defaults
  }
}

// Module-scope state: one source of truth shared by every caller, the global
// consent modal, and the Settings section.
const prefs = ref<TxExplorerPrefs>(loadPrefs())
/** Tx hash awaiting user consent — non-null shows ExternalExplorerModal. */
const pendingTx = ref<string | null>(null)

function savePrefs() {
  localStorage.setItem(KEY, JSON.stringify(prefs.value))
}

export function useTxExplorer() {
  const launcher = useAppLauncherStore()
  const containers = useContainerStore()

  /** Normalized explorer base URL (no trailing slash). */
  function explorerUrl(): string {
    return (prefs.value.url || DEFAULT_TX_EXPLORER).trim().replace(/\/+$/, '')
  }

  function openExternal(txHash: string) {
    window.open(`${explorerUrl()}/tx/${txHash}`, '_blank', 'noopener,noreferrer')
  }

  /**
   * Entry point for every "view transaction" affordance in the app.
   *
   * The local Mempool app WINS whenever it is installed — including while
   * it is stopped or restarting, where the app session's own controls are
   * the right place to land. Only a node that genuinely does not have the
   * app (the pruned-node case this file was written for) ever reaches an
   * external explorer.
   *
   * The await is load-bearing, not incidental. `getAppState` reports
   * `not-installed` for an app it simply has not fetched yet, so the old
   * synchronous check sent a user with a perfectly healthy local Mempool
   * to a third-party explorer whenever they clicked before the container
   * list arrived — a privacy leak decided by a race, and the reason this
   * regression kept coming back (reported again on .228, 2026-08-06).
   */
  async function openTx(txHash: string) {
    await containers.ensureFetched()
    if (containers.getAppState('mempool') !== 'not-installed') {
      launcher.openSession('mempool', { path: `/tx/${txHash}` })
      return
    }
    if (prefs.value.acknowledged) {
      openExternal(txHash)
      return
    }
    pendingTx.value = txHash
  }

  /** Consent modal confirm: persist the (possibly edited) URL + ack, open. */
  function confirmPending(url: string, dontAskAgain: boolean) {
    const trimmed = url.trim()
    prefs.value.url = trimmed || DEFAULT_TX_EXPLORER
    if (dontAskAgain) prefs.value.acknowledged = true
    savePrefs()
    const tx = pendingTx.value
    pendingTx.value = null
    if (tx) openExternal(tx)
  }

  function cancelPending() {
    pendingTx.value = null
  }

  /** Settings hook: change the explorer and/or reset the consent. */
  function setExplorer(url: string, acknowledged?: boolean) {
    prefs.value.url = url.trim() || DEFAULT_TX_EXPLORER
    if (acknowledged !== undefined) prefs.value.acknowledged = acknowledged
    savePrefs()
  }

  return {
    prefs,
    pendingTx,
    explorerUrl,
    openTx,
    confirmPending,
    cancelPending,
    setExplorer,
  }
}
