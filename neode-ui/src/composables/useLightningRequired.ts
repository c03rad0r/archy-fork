// Shared "this action needs a working Lightning node" gate (2026-08-02).
//
// Creating a Lightning invoice used to fail at the RPC layer whenever this
// node had no usable Lightning implementation — `lnd.createinvoice` returned
// connection-refused and the Receive screen rendered "Operation failed. Check
// server logs for details." That reads as the wallet being broken, when the
// truth is a missing (or stopped) prerequisite the user can act on.
//
// Callers ask `requireLightningNode()` BEFORE attempting the call. When there
// is no usable node it opens the global LightningRequiredModal and returns
// false, so the caller bails without surfacing an error at all.
//
// Keyed on package STATE, not mere presence: `package-data` carries an entry
// for a Lightning app that is known to this node but not actually running, so
// `id in packages` is NOT "installed and usable" — that assumption was the
// first version's bug, and it let the raw RPC error through on a node with no
// lnd container at all.
import { ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { rpcClient } from '@/api/rpc-client'
import { PackageState } from '@/types/api'

/** Package ids that provide a Lightning node.
 *
 * `lnd` ships today. Core Lightning is the next implementation the modal
 * offers — when its app id lands in the catalog, add it here and flip its
 * `available` flag in LightningRequiredModal; nothing else changes. */
export const LIGHTNING_NODE_APP_IDS = ['lnd'] as const

/** `absent` — nothing installed, offer to install one.
 *  `stopped` — installed but not running, point the user at My Apps.
 *  `no-funds` — running, but there is nothing to pay with / no inbound
 *      liquidity to be paid into; send the user to the Lightning setup goal.
 *  `running` — good to go. */
export type LightningStatus = 'absent' | 'stopped' | 'running' | 'no-funds'

/** WHY the funding modal opened — the old copy always said "you have no
 *  channel yet", which was a lie three ways: a just-opened channel sits in
 *  LND's pending list (invisible to the outbound sum) until it has ~3
 *  confirmations, channels can exist with all their balance on the far
 *  side, and a payment failure can look like a funding problem. The user
 *  sees "no channel" while looking at a wallet full of pending liquidity
 *  (framework-pt, 2026-09-01: "LND thinks I do not have a channel").
 *  `none` — genuinely no channels, the open-one flow is right.
 *  `pending` — channel(s) exist but are still confirming on-chain.
 *  `far-side` — open channel(s), but the needed direction has zero balance.
 *  `failed-payment` — LND refused a payment; looks like routing/liquidity. */
export type FundingReason = 'none' | 'pending' | 'far-side' | 'failed-payment'

// Module-scope: one source of truth shared by every caller and the single
// global modal mounted in App.vue.
const show = ref(false)
const status = ref<LightningStatus>('absent')
/** Which direction raised the funding modal, so the copy can be specific. */
const fundingDirection = ref<'send' | 'receive'>('receive')
/** Why the funding modal opened, so the copy states the node's real state. */
const fundingReason = ref<FundingReason>('none')

export function useLightningRequired() {
  // The store is resolved lazily, inside the functions that need it, rather
  // than at composable-call time: a component may legitimately be mounted in
  // a test (or any context) without an active Pinia, and merely *having* this
  // gate available must not be what breaks it.
  /** Best status across every known Lightning implementation. */
  function lightningStatus(): LightningStatus {
    const pkgs = (useAppStore().packages ?? {}) as Record<string, { state?: string } | undefined>
    let best: LightningStatus = 'absent'
    for (const id of LIGHTNING_NODE_APP_IDS) {
      const entry = pkgs[id]
      if (!entry) continue
      if (entry.state === PackageState.Running) return 'running'
      // Present but not running: installing, starting, stopped, exited…
      best = 'stopped'
    }
    return best
  }

  function hasLightningNode(): boolean {
    return lightningStatus() === 'running'
  }

  /**
   * Gate a Lightning-only action. Returns true to proceed; returns false and
   * opens the modal (in the mode matching why) when there is no usable node.
   */
  function requireLightningNode(): boolean {
    const s = lightningStatus()
    if (s === 'running') return true
    status.value = s
    show.value = true
    return false
  }

  function close() {
    show.value = false
  }

  /**
   * Raise the same modal in its funding mode: the node is installed and
   * running, but has no usable balance/liquidity yet. Reuses this modal
   * rather than inventing a second one, and routes to the Lightning setup
   * goal where funding and channel-opening already live.
   */
  function openLightningFunding(reason: FundingReason = 'none') {
    status.value = 'no-funds'
    fundingReason.value = reason
    show.value = true
  }

  /**
   * Map a failed Lightning attempt onto the funding modal when the node is
   * running but has nothing to pay with / no inbound liquidity. Returns true
   * when it handled the error, so the caller can skip showing a raw string.
   *
   * Matched on the message because LND surfaces these as plain text: there is
   * no distinct error code for "no channels" vs "no route" vs "insufficient
   * balance", and all three mean the same thing to the user — fund me.
   */
  function handleLightningFailure(err: unknown): boolean {
    if (lightningStatus() !== 'running') return false
    const msg = (err instanceof Error ? err.message : String(err ?? '')).toLowerCase()
    const fundingRelated = [
      'no route',
      'no routes',
      'insufficient',
      'no channel',
      'not enough',
      'balance',
      'unable to find a path',
      'no path',
    ].some((needle) => msg.includes(needle))
    if (!fundingRelated) return false
    // LND refused the payment itself — not necessarily "no channels", so
    // the modal must not claim it is. Most often this is routing/liquidity.
    openLightningFunding('failed-payment')
    return true
  }

  /**
   * The full readiness gate: node installed AND running AND with liquidity in
   * the direction being attempted.
   *
   * Node state alone is not enough — LND happily mints an invoice with zero
   * channels, so a state-only gate hands the user an invoice nobody can pay
   * (and a send that can only fail). `receive` needs inbound liquidity,
   * `send` needs outbound.
   *
   * Fails OPEN on an RPC error: if we cannot read the channel list we let the
   * attempt proceed rather than block a working wallet on a transient blip.
   */
  async function requireLightningReady(direction: 'send' | 'receive'): Promise<boolean> {
    if (!requireLightningNode()) return false
    try {
      const res = await rpcClient.call<{
        total_inbound?: number
        total_outbound?: number
        channels?: { status?: string; local_balance?: number; remote_balance?: number }[]
      }>({
        method: 'lnd.listchannels',
        timeout: 15000,
      })
      const liquidity = direction === 'receive' ? res?.total_inbound ?? 0 : res?.total_outbound ?? 0
      if (liquidity > 0) return true
      fundingDirection.value = direction
      // Zero in the needed direction — say WHY, from the same response.
      // The channel list carries pending entries (status 'pending_open');
      // the totals deliberately exclude them (nothing is spendable through
      // an unconfirmed channel), so "0 outbound + pending channels" is the
      // just-opened-a-channel state, not "no channel".
      const channels = res?.channels ?? []
      const hasPending = channels.some(c => c.status === 'pending_open')
      const hasOpen = channels.some(
        c => c.status === 'active' || c.status === 'inactive' || (!c.status && (c.local_balance || c.remote_balance)),
      )
      openLightningFunding(hasPending ? 'pending' : hasOpen ? 'far-side' : 'none')
      return false
    } catch {
      return true
    }
  }

  return {
    show,
    fundingDirection,
    fundingReason,
    status,
    lightningStatus,
    hasLightningNode,
    requireLightningNode,
    openLightningFunding,
    requireLightningReady,
    handleLightningFailure,
    close,
  }
}
