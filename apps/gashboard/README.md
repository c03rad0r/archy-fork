# Gashboard on Archipelago

Install DATUM and configure its payout address, then install Gashboard. The app
connects to `http://datum:7152` on `archy-net`, so recreating DATUM does not require
copying a new container IP. The shared DATUM admin password is injected as a
platform secret and never sent to the browser. This PR depends on the DATUM app
package being merged and its build context being shipped.

## Sign in and invite miners

Use **Sign in with Nostr** inside Archipelago to choose a node identity through
the native signer. A standalone visitor can use a browser extension or remote
Nostr signer. No private key is entered into Gashboard.

All user identities offered by Archipelago's signer are dashboard owners through
`NODE_IDENTITY_PUBKEYS`; the appliance identity is excluded. Owners can open
**Access**, paste another miner's `npub`, and add them as a viewer. Share the
Gashboard URL on port 1337 over your intended node access route. Viewers can read
the entire fleet and join dashboard chat; they cannot edit membership or configure
DATUM. Access is independent of the miner's Stratum worker name. Signing with an
unlisted identity is rejected even if that person mines through DATUM.

Owners can remove a viewer in Access. Every authenticated request rechecks the
list, so an already-issued JWT stops working immediately. Removing a viewer does
not erase information or chat keys that their browser previously received.
Membership lives in `/var/lib/archipelago/gashboard/access.json`; preserve this
directory and the platform JWT secret across reinstall. Node owners are managed
in Archipelago identities, not in Gashboard. Restart Gashboard after changing
node identities to refresh owner access.

The app gate uses `auth: open` because invited miners have Gashboard membership,
not node administrator accounts. Gashboard still authenticates every data API.
The gate supplies HTTPS and iframe header handling. A node administrator can
enable the gate's extra login if only node users should reach the app.

## Source and native signer

`docker/gashboard` vendors the user-supplied Gashboard source at commit
`68b606b` from `/home/yaya/Projects/gashboard`, with Archipelago integration and
membership changes reviewed here. Its configured remote,
`https://git.tx1138.com/lfg2025/gashboard.git`, was unavailable over TLS during
preparation; upstream freshness has not been verified. Vendoring makes the build
independent of that server. The original application declares the MIT license.

The image bakes the canonical `neode-ui/public/nostr-provider.js`; the install
hook refreshes its persisted copy from the node. Refresh the baked copy when
updating the package. The server serves the provider uncached with
`data-app-id="gashboard"` and `data-no-nip98`, preserving Gashboard's own NIP-98
login. The service worker never caches the signer. Existing NIP-07 browser
providers take precedence over the node provider. The CSP permits the native
signer broker frame in standalone/companion launches.

`/healthz` checks that the dashboard API is serving. DATUM connection failures are
reported in the dashboard snapshot rather than disguised as a healthy mining
fleet. Contribution history persists under `/data`; live miner connections and
current hash rate recover through repeated DNS-based polling. Miner display names
come from their worker names, and history uses the full Stratum username so
multiple miners of the same model are not combined under a preset nickname.
Use a distinct worker name for each miner. Old Umbrel history should not be
copied blindly: its ledger used preset nicknames as identities.

Before release, validate HTTP/HTTPS iframe launch, companion launch, native
identity consent, invited-user login, revocation, DATUM address change/recovery,
and install/start/stop/reinstall/reboot on a dedicated Archipelago test node.

## Local validation (2026-10-06)

API access-control and worker-identity tests, TypeScript checks, production builds,
manifest validation and generated catalog drift checks pass. Both container images
build and run with read-only roots, cap-drop ALL and no-new-privileges on Docker.
Gashboard's digest-authenticated polling recovered after DATUM moved from
172.22.0.2 to 172.22.0.4, without restarting or reconfiguring Gashboard, and after
another DATUM restart with preserved settings.

The access/login flow passed Chromium 153, Firefox 155 and WebKit 26.6, each at
1440x900 and 390x844: native-provider NIP-98 through a simulated host frame,
invalid-key feedback, invitation, reload persistence, removal, and layout fit.
API tests also verify owner-only edits, rejected outsider login, persisted
membership, owner protection, corruption refusal, and revoked JWT/SSE access.
Browser scenarios and screenshots remain outside the repository in the shared
browser-check workspace. These browser tests simulate the app gate and signer
host; they do not establish real-node consent, HTTPS or Android acceptance.

The generated storefront entries are review candidates. No signed catalog,
registry image or release was published. Keep actual-node acceptance
separate from these local results.

## Operator-authorized node deployment (2026-10-06)

Installed alongside DATUM on archi-dev-box and yaya-server using rootless Podman,
read-only roots and the node-generated secrets. Both apps report healthy. Their
My Apps tiles show normalized icons, names and Launch controls. Chromium verified
Gashboard's embedded launch, native identity selection/signing, and owner Access
page on both nodes. Standalone native login and fresh DATUM polling passed too.

On yaya, Chromium 153, Firefox 155 and WebKit 26.6 passed owner login, Access-page
layout and reload persistence at 1440x900 and 390x844. These are Linux browser and
viewport checks, not Android companion or physical iPhone acceptance. Anonymous
requests to mining data and membership endpoints returned 401. No viewers were
added to the live allowlist during these read-only UI checks.

DATUM's normal package restart on yaya changed its address from 10.89.0.9 to
10.89.0.11; Gashboard was not restarted or reconfigured and its authenticated stats
API returned a successful poll less than five seconds old afterwards. Gashboard
also completed its own normal package restart. The previous Docker tests cover
invitation, revocation and membership persistence; full live-node membership,
HTTPS, companion, preserved-data reinstall and reboot acceptance remain separate.

Payouts are not configured and no real miner shares were submitted in this check.
These are node test deployments of review candidates, not catalog publication.
