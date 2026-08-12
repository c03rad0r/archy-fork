// Machine-readable reason codes the backend embeds as a `[CODE]` token in
// receive-address errors (see core .../api/rpc/lnd/wallet.rs). Mapping the code
// directly is precise — unlike the substring heuristics below, it cannot
// mislabel an unreachable-REST failure as "wallet is locked" (the .228 bug).
const RECEIVE_CODE_MESSAGES: Record<string, string> = {
  LND_REST_UNREACHABLE:
    'Bitcoin address is not ready yet because the Lightning wallet service is still starting up or recovering. Please try again in a moment.',
  LND_WALLET_LOCKED:
    'Bitcoin address is not ready because the Lightning wallet is locked. Unlock or initialize LND first.',
  LND_WALLET_UNINITIALIZED:
    'Bitcoin address is not ready because the Lightning wallet has not been set up yet. Finish wallet setup, then try again.',
  LND_SYNCING:
    'Bitcoin address is not ready while the wallet is still syncing with the Bitcoin network. Try again once sync has progressed.',
  LND_ERROR:
    'Bitcoin address is not ready yet. Check that the Lightning app is healthy, then try again.',
}

export function explainReceiveAddressFailure(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error || '')

  // Prefer the structured reason code when present.
  const code = message.match(/\[([A-Z_]+)\]/)?.[1]
  if (code && RECEIVE_CODE_MESSAGES[code]) {
    return RECEIVE_CODE_MESSAGES[code]
  }

  const lower = message.toLowerCase()

  if (lower.includes('wallet') && (lower.includes('locked') || lower.includes('unlock'))) {
    return 'Bitcoin address is not ready because the Lightning wallet is locked. Unlock or initialize LND first.'
  }
  if (lower.includes('uninitialized') || lower.includes('not initialized') || lower.includes('initwallet')) {
    return 'Bitcoin address is not ready because the Lightning wallet has not been initialized yet.'
  }
  if (lower.includes('sync') || lower.includes('chain backend') || lower.includes('neutrino')) {
    return 'Bitcoin address is not ready while Bitcoin or LND is still syncing. Try again once sync has progressed.'
  }
  if (lower.includes('rest connection failed') || lower.includes('failed to parse newaddress response')) {
    return 'Bitcoin address is not ready because LND is not responding cleanly yet. Check that the Lightning app is healthy and retry.'
  }
  if (lower.includes('connection') || lower.includes('connect') || lower.includes('unavailable') || lower.includes('refused')) {
    return 'Bitcoin address is not ready because LND is not reachable yet. Check that the Lightning app is running.'
  }
  if (lower.includes('did not return') || lower.includes('empty address')) {
    return 'Bitcoin address is not ready because LND did not return an address. The wallet may still be locked, uninitialized, or waiting for Bitcoin to sync.'
  }

  return message || 'Bitcoin address is not ready yet. Check Bitcoin and LND status, then try again.'
}
