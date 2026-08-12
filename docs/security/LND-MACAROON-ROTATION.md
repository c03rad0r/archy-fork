# Rotating this node's Lightning credentials

A Lightning macaroon is a **bearer token**: whoever holds one can spend from the
node's wallet. There is no revocation list and no expiry. If a macaroon is ever
read by something you do not control — a leaked endpoint, a screenshot, a phone
that has since been lost, an app that ran a version with a published
vulnerability — that ability persists until the macaroons are rotated.

Rotation is therefore a **routine operator action**, not an emergency procedure.
Two paths do the same work:

| Path | Use when |
|---|---|
| **Dashboard** — Settings → *Lightning credentials* | Normal case. Password-confirmed, shows progress, repairs BTCPay for you. |
| **`scripts/security/rotate-lnd-macaroon.sh`** | No dashboard reachable, or you want a detect-only report. |

## What rotation actually does

LND derives every macaroon it issues from a root key in `macaroons.db`. Remove
that root key plus the issued `*.macaroon` files, restart, and LND mints a fresh
root key and a fresh set of macaroons when the wallet unlocks. Every macaroon
issued before that moment — including any an attacker holds — stops verifying.

## Why your funds and channels survive

Macaroons are bearer tokens, not keys. Coins live in `wallet.db` and channel
state in `channel.db`; channels are secured by the node's identity and channel
keys, none of which are derived from the macaroon root key. Neither database is
opened, moved or deleted.

Both paths **prove** this rather than asserting it: they record the node's
identity pubkey and its channel census before rotating, and refuse to report
success if either differs afterwards.

Two details in that check are deliberate and should not be "tightened":

- **Channels are compared as a total, not as `num_active_channels`.** The active
  count only counts channels whose peer is currently online, so it legitimately
  dips for minutes after *any* restart while peers reconnect. Asserting on it
  alone would abort a perfectly healthy rotation.
- **`wallet.db` is not compared byte-for-byte.** btcwallet records chain-sync
  progress inside it, so the file changes on every start. Asserting byte-identity
  would fire a frightening false alarm on a completely healthy rotation.

## What it never does

- No macaroon **content** reaches a response, an error, a log line, or the
  progress feed the dashboard polls. Everything reported is a SHA-256 digest or a
  byte count — enough to prove the material changed without disclosing it to
  whoever is reading the screen.
- No path from "rotate my credentials" to "delete my wallet". LND's boot path
  self-heals a wallet no candidate password can open by wiping and recreating it;
  correct for an unattended boot, catastrophic here. Rotation unlocks through
  `container::lnd::unlock_existing_wallet_no_wipe`, so a wallet whose password
  this node does not hold surfaces as a **failed rotation** with the wallet
  intact.

## Nothing else may touch LND mid-rotation

Between "stop LND" and "start LND" the rotation owns a stopped container whose
credential material is being deleted. Two background actors would step in there
unasked: the **health monitor** restarts any container it finds stopped, and the
**reconciler** starts one whose unit is enabled. Either brings LND back up
mid-deletion — and LND re-mints `macaroons.db` on unlock, so the deletion loop
would race a live process writing that file, or "succeed" against material that
had already been regenerated. The operator would be told they had rotated while
the old root key was still in service.

The rotation therefore holds `app_ops::op_lock("lnd")` for its whole duration.
That is the lock both actors already consult (`lifecycle_op_in_flight`, reached
in the health monitor via `lifecycle_op_covers_container`), and it also
serialises against the `package.start`/`stop`/`restart` workers, so an operator
hitting "Restart" on Lightning mid-rotation queues rather than interleaving. A
rotation requested while one of those is running fails fast with a short
explanation instead of waiting silently.

Deliberately **not** the `user-stopped` marker that `recreate_wallet_destructively`
uses for its own window: that marker is a file on disk, so a rotation that died
between marking and clearing would leave Lightning suppressed *permanently* —
fixable only by finding and editing JSON on the node. The lock guard releases
when it drops, on every path including a panic.

## The BTCPay coupling — the part that bites

**BTCPay Server keeps its own inline copy of the admin macaroon**, and it cannot
self-heal. LND's data directory is owned by its container's mapped uid, so BTCPay
cannot bind-mount the macaroon file (EACCES across the userns boundary). The
connection string therefore carries the macaroon as hex:

```
type=lnd-rest;server=https://lnd:8080/;macaroon=<hex>;certthumbprint=<hex>
```

delivered as the `btcpay-lnd-connection` secret file. Rotate the macaroons and
that copy becomes a dead credential. Nothing notices on its own, because the
daemon only regenerates this secret when LND's **TLS cert thumbprint** changes —
and macaroon rotation does not touch the cert.

The resulting state is the dangerous one: **BTCPay is up, LND is up, both report
healthy, and every Lightning invoice BTCPay tries to create fails.**

Repair needs two things, and one without the other is cosmetic:

1. **Rewrite the secret** (`container::lnd::rewrite_btcpay_lnd_connection_secret`).
   This is what makes the change visible: `secret_env_hash` is derived from the
   resolved secret contents, so a changed file reads as label drift on the
   running container.
2. **Recreate the container.** `btcpay-server` is on the restart-sensitive list,
   and the reconcile loop runs in `ExistingOnly` mode *always* — boot and
   periodic alike — where env drift on a restart-sensitive app is detected and
   then deliberately skipped. Rewriting the secret alone therefore changes
   nothing that is running. Observed directly on a development node, once per
   tick, for half an hour:

   ```
   container drift detected during boot reconcile; leaving running
   restart-sensitive app untouched app_id=btcpay-server
   ```

   The dashboard path calls
   `ContainerOrchestrator::mark_credential_rotated("btcpay-server")`, which is
   the flag the drift check consults to override restart-sensitivity. It is the
   same carve-out FED-07 added for the Fedimint gateway, and the reasoning is
   identical: restart sensitivity protects apps that are *working*, and this one
   is working only in appearance.

**The shell script cannot set that in-process flag**, so it does the equivalent
from outside: it deletes the secret (the daemon regenerates it within a tick),
then removes the `btcpay-server` container so the orchestrator's own
desired-state recovery rebuilds it around unchanged data. That recovery is what
makes this safe rather than a hand-rolled remove-and-run — it fires because the
app is still installed and was in the last running-containers snapshot. The
script then prints the commands to confirm it actually happened, because a
failure here is invisible.

## Slow nodes: the unlock budget

LND opens `channel.db`, `graph.db` and `wallet.db` before it serves the unlocker
at all, and on a busy node that is genuinely slow — **2m38s measured on a box
running 30 containers**. The unlock helper used to give up after ~60s, which on
such a node could never succeed.

That timeout was not a harmless retry. Reconcile records the post-start hook as
failed, restarts LND, and the slow database open starts over: a restart loop that
leaves the wallet permanently locked and every LND-dependent app (BTCPay's
internal node included) broken, on exactly the nodes least able to afford it.

The not-ready budget is now ~10 minutes (`UNLOCK_NOT_READY_ATTEMPTS`). Waiting
longer costs nothing, because a genuinely wrong password still exits on the first
pass through the candidate list — the `all_rejected` fast path is untouched.

## Verifying a rotation

The dashboard shows all of this. From a shell:

```bash
# 1. Fingerprint changed (digest only — never print the macaroon)
sudo sha256sum /var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon

# 2. Same node, same channels
podman exec lnd lncli --network=mainnet getinfo \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["identity_pubkey"], \
      d["num_active_channels"] + d["num_inactive_channels"], d["num_pending_channels"])'

# 3. BTCPay is carrying the CURRENT macaroon, not the rotated-out one
CUR=$(sudo od -An -v -tx1 /var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon | tr -d ' \n')
SEC=$(sudo sed -n 's/.*macaroon=\([0-9a-f]*\).*/\1/p' /var/lib/archipelago/secrets/btcpay-lnd-connection)
[ "$CUR" = "$SEC" ] && echo "current" || echo "STALE — BTCPay's Lightning is broken"

# 4. BTCPay was actually recreated (a silent failure looks like success)
podman inspect btcpay-server --format '{{.Created}}'
```

Check 3 is the one people skip, and it is the one that fails.

## Afterwards

- **Re-pair every wallet app**, Zeus most importantly. Open the Lightning app in
  the dashboard and scan the pairing QR again; it serves the new macaroon.
- **Delete the backup once re-pairing is done.** Both paths back the old material
  up to `/var/lib/archipelago/lnd/macaroon-rotation-<stamp>` (0700) so a mistake
  is recoverable. That directory holds the **old root key** and is still
  sensitive: `sudo rm -rf <path>`.

## Related

- `docs/security/BITCOIN-RPC-PROXY-EXPOSURE.md` — the leak that first made
  rotation necessary, and the operator decision not to rotate the fleet for it.
- `scripts/security/rotate-lnd-macaroon.sh` — the shell path, including its
  ordering guard (it refuses to rotate on a binary that still leaks
  `/lnd-connect-info`, since the new macaroon would leak within seconds).
