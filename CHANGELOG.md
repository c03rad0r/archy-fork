# Changelog

## v1.8.22-alpha (2026-09-30)

- Network diagnostic failures no longer stop all apps or rebuild shared container networking.
- Prevented orphaned companion dashboards from repeatedly reinstalling themselves after their backend app was removed.
- Fixed companion dashboard builds still referencing a retired image registry.

- Fixed Angor Indexer health checks choosing IPv6 localhost for an IPv4 listener and unnecessarily restarting the working service.

- Prevented false app restarts by probing each published port at its actual bind address; Nginx Proxy Manager now checks its internal admin API.
- Added a backed-up migration for the recognized legacy Nginx Proxy Manager tunnel/LND port conflict in both OTA and ISO startup paths.

- Checked Bitcoin and Electrum companion dashboards instead of backend protocol ports, preserving dashboard access during initial sync.
- Removed web-interface waiting messages from headless services such as Phoenixd and clarified which interface is unavailable for launchable apps.

- Finished runtime app-file promotion before manifest loading, preventing startup catalog refresh from forgetting disk-only apps.

- Named the app in compact readiness messages and kept app-card actions aligned at the bottom.
- Removed duplicate Mempool cards caused by frontend container aliases in restored inventory.

- Kept installed apps visible through restarts and hard refreshes, and delayed app launches until their web interface is ready.
- Made Bitcoin version selection readable and usable in the ThinkPad kiosk, above the pruning settings.
- Restored GitWorkshop build files in installation/update payloads and made slow image-pull progress clearer.
- Fixed same-node Gitea access from Portainer, with persistent runtime migration, state backups and recovery after failed restarts.
- Preserved Gitea configuration and SSH operation during fresh setup and upgrades.
- Improved paid-file delivery, saved-file permissions and repeat-download compatibility; verified Tor-only payment with change, rejection refunds and free repeat downloads.
- Added a headless Angor Indexer service using the existing Mempool/ElectrumX stack, and an optional separate Angor relay.
- Prevented manifest command arguments containing apostrophes from being corrupted in generated services.

## v1.8.21-alpha (2026-09-30)

- Fixed Bitcoin and other containers being forcibly stopped after ten seconds during managed updates and restarts.
- Existing installations now receive the same graceful shutdown allowance as new containers, without restarting apps just to apply this setting.
- Prevented unnecessary Lightning restarts when Bitcoin has stayed running; dependency restarts now require an observed Bitcoin container change.
- Includes the Cashu payment, optional Bitcoin pruning, Lightning readiness, and explorer improvements from 1.8.20.

## v1.8.20-alpha (2026-09-29)

- Fixed Cashu file payments rejected despite a shared mint, and preserved the payment amount when mint fees reduce change.
- Payment failures now report whether a refund actually succeeded; missing files and unsupported payment methods are rejected before charging.
- Improved saving paid files into Files and reopening purchases without paying again.
- Bitcoin Core and Knots installation offers optional pruning on larger disks, using the same settings as automatic pruning.
- Fixed false missing-port checks that unnecessarily restarted Bitcoin and LND; recovery now respects managed shutdown timeouts.
- LND explains when it is waiting for Bitcoin installation, startup, or sync, without treating normal synchronization as a restart-worthy failure.
- Bitcoin startup messages explain block-index loading without exposing raw RPC errors, and Lightning keeps known balances clearly marked during outages.
- Changed the public transaction-explorer default to mempool.space while preserving local explorers and custom choices.

## v1.8.19-alpha (2026-09-28)

- Fixed the embedded AIUI chat page painting a second background and dark scrim over Archy’s dashboard background.
- Embedded AIUI now stays transparent so the dashboard background appears once.
- AIUI background fixes are now included reliably in OTA updates and fresh installations.

## v1.8.18-alpha (2026-09-18)

- Framework startup prioritizes Bitcoin and LND before unrelated containers, and unavailable LND balances remain unavailable instead of appearing as false zeroes.
- Cashu Receive guides unseeded wallets through recovery-phrase setup, with shorter backup guidance and a single-column layout.
- Added live Framework verification for automatic LND unlock, native balance preservation, Cashu address registration, and proof preservation.

## v1.8.17-alpha (2026-09-15)

- Minibits claims that every mint reports as already spent leave the retry queue, clearing repeated failure notices. Network errors and mixed mint failures remain queued for another attempt.
- Minibits polls its primary relay first and connects to public fallback relays only when the primary is unreachable, reducing unnecessary connections.
- Large payment backlogs are fetched from newest to oldest with a saved cursor, so polling can resume after interruptions or page limits. Payments sharing the same timestamp remain reachable.
- Added regression coverage for spent-claim classification, wrapped and mixed mint errors, same-second payments, and interrupted or multi-poll backlogs.

## v1.8.16-alpha (2026-09-15)

- App updates refresh and verify the signed catalog before changing containers. A failed refresh or manifest reload cancels the update, and automatic updates wait for a successful refresh.
- Fixed repeated Mempool update offers: downstream `-archyN` patches now sort above their upstream release, and moving a published image between registry namespaces does not hide a genuine upgrade.
- Updates inspect installed component versions, refuse known downgrades, skip containers already at the target versions, and verify the resulting versions before reporting success.
- Added regression coverage for stale catalogs, matching versions, publisher namespace changes, stack component updates, and keeping running containers untouched when no upgrade is needed.

## v1.8.15-alpha (2026-09-13)

- Cuprate is presented as one user-facing app in My Apps, including its UI launch button; the generated dashboard companion is hidden as an implementation detail instead of appearing under Services.
- Added regression coverage for Cuprate install and installed-state grouping.
- Release validation was rerun on the corrected tree before OTA and ISO publication.

## v1.8.14-alpha (2026-09-13)

- **Cuprate gains a first-party companion dashboard.** The Monero node now has a Bitcoin-style status UI, safe app grouping, a 450 GB disk-safety gate, and a restricted RPC that is never exposed as a launch page.
- **Bitcoin Core Tor enrollment uses the correct protocol identity.** `bitcoin-core` is forwarded on port 8333 and resolves to its own hidden-service directory without disturbing legacy Bitcoin aliases.
- **GitWorkshop opens Archipelago’s canonical ngit repository by default.** The launcher and registry promotion use the full maintainer/relay/`archy` coordinate, with regression coverage for Companion and browser-tab launches.
- **Release validation is stricter.** The registry gate now checks the complete canonical source deep link, and the merged candidate passed the full frontend and focused backend test suites.

## v1.8.13-alpha (2026-09-12)

- **GitWorkshop installs reliably on fresh nodes.** The app is classified as a user-facing app while its install placeholder is being created, so it remains visible under My Apps instead of Services.
- **Fresh GitWorkshop installs build the correct image.** The production orchestrator handles its bundled build context instead of sending the local image reference through the legacy registry-pull path.
- **Curated app classification is regression-tested.** Every user-facing app remains in My Apps during installation, while headless services stay in Services.

## v1.8.12-alpha (2026-09-11)

- **Fresh IndeedHub installs no longer share a fleet-wide encryption root.** The API now generates a persistent per-node AES master secret and shares it with the media worker through the platform's protected secret environment. Existing nodes migrate the exact legacy value they are already using before any container can be recreated, preserving access to encrypted data; an unreadable or empty existing root fails safely instead of being silently replaced. The manifest path, retired fallback installer, and container repair script follow the same rule.

- **The Companion download advertises and re-announces the APK it actually serves.** The Discover banner and its install prompt now share the no-cache APK metadata, visibly report Companion 0.5.32 build 52, and remember dismissal per Android build rather than forever, so an existing browser gets one useful update prompt when the APK changes. The ISO gate reads the expected version from the Android build itself instead of accepting the stale 0.5.28 payload.

- **GitWorkshop's dependency audit is clean.** The pinned upstream client keeps its separately reviewable Archipelago integration patch and now applies a deterministic dependency patch: safe lock refreshes plus targeted `fflate`, React Router, and Vitest upgrades remove all ten production advisories and all eight development advisories. A clean install reports zero vulnerabilities; type-check, all 152 upstream unit tests, and the exact Archipelago subpath build pass.

- **Every completed payment now gets the full Lightning-style receipt screen.** Cashu and Fedimint sends no longer leave the payment form open behind a token; wallet, QR-scan, Web5, and app-requested sends all replace their forms with the animated success state. Payment hashes, transaction IDs, ecash tokens/notes, mint details, and other useful references remain copyable in the receipt, and receive completions open the same distinct payment-success modal. Minibits claims retain a short-lived durable receipt so the visible modal still reports success when another dashboard or Companion context wins the claim-poll race, while concurrent watchers now share one bounded relay fetch instead of queueing several long polls.

- **TollGate provisioning closes the free-access path without taking over an admin network.** Confirmed upstream `TollGate-*` access points are moved from LAN onto the paid network, mint URLs are normalized consistently, and operators can set a validated Lightning payout address without replacing merchant keys or other revenue-share identities. Malformed existing identity data now stops provisioning safely instead of being overwritten.

- **Cashu receive gains a human-readable Minibits Lightning address.** The node derives the profile from the existing ecash recovery phrase, collects payments from the Minibits Nostr delivery relays, and redeems them into the Cashu wallet. Claim polling is single-flight, state and already-consumed tokens are written atomically with private permissions, same-second events are deduplicated without being skipped, restored seeds cannot reuse another wallet's profile, and pending claims retain the service key that encrypted them across key rotations. The UI identifies Minibits as a third-party beta service and recommends small balances.

- **Nostr sign-in returns directly to the app instead of a black or grey frame.** The top-level signer broker now stays loaded as a 1px non-interactive surface parked physically off-screen; removing or display-hiding its full-screen cross-origin iframe could leave stale compositor pixels above IndeeHub or GitWorkshop in Android WebView and mobile Chromium until refresh. One retained broker also keeps identity selection and its immediately following signing request in a continuous UI, while Companion no longer adds a separate 180ms cover that made GitWorkshop visibly flicker.

- **Gitea is sized for source and release hosting, not an empty demo.** Its manifest storage allowance is now 50GiB, release attachments accept individual files up to 10GiB, container-package owner storage remains unlimited, and HTTP/HTTPS proxy uploads share a streamed 10GiB ceiling. Existing repository, package, LFS and release data is unchanged.

- **Companion browser-tab signing now accepts the app gate's complete session.** A fresh external browser no longer needs a prior dashboard login/localStorage marker before the dashboard-origin signer can load. The app gate now issues both the shared HttpOnly node session and its matching readable CSRF token, so identity discovery and signing RPCs work after that one login instead of rendering a misleading “No identities found” state. Normal dashboard logout/session checks keep their existing behavior.

- **Fast Nostr identity choices now survive app startup and Companion tabs.** The tab/WebView broker waits for the application load event before opening its first-run picker, queues every NIP-07 call until the signer is initialized, and hands the just-selected public key directly to the immediate login request. GitWorkshop now turns that first-run choice into its normal extension account automatically, eliminating the startup race that surfaced as IndeedHub's “Could not get public key from extension.”

- **GitWorkshop makes network projects and Archipelago login explicit.** Its signed-in dashboard now includes recent repositories from the Nostr git index, the NIP-07 action reads “Extension / Archipelago,” and explicit Archipelago logins reopen the node identity chooser instead of silently reusing the first identity. Direct, user-triggered NIP-07 logins receive the same account-switch behavior for upstream apps such as IndeedHub.

- **IndeedHub tab signing now tracks the dashboard signer.** The injected provider supports the contained signer broker in direct tabs, is cache-busted, and is reconciled after dashboard-only updates as well as app installs and starts.

- **App launches now honor credentials everywhere.** Home, Spotlight, Discover, My Apps, and app-detail launches all pass through one platform-owned credential handoff, so Portainer's first-run token and the File Browser/PhotoPrism login details can no longer be skipped by launching from the Home grid.

- **Manage Updates returns to Download immediately after cancellation.** Canceling a stalled OTA now clears both the local staged state and progress state instead of leaving an incorrect Install button visible until the page is refreshed.

- **GitWorkshop no longer probes a desktop-only localhost relay or unauthenticated manifest.** The packaged upstream client disables its default `localhost:4869` nostrdb probe, uses credentialed manifest loading, drops dead lookup relays, and permits the dashboard's contained signer broker in its frame policy.

- **Rootless app ports self-heal when `pasta` drops a listener.** The five-minute container doctor compares every running container's declared Podman port bindings with actual host listeners and restarts only a container whose listener vanished. TCP and UDP are checked separately, avoiding false restarts of services such as NetBird's UDP port 3478. This covers the intermittent Nginx Proxy Manager port 8081 rebind failure without requiring a node reboot.

- **Nostr identity actions now use one contained, companion-safe signing experience.** The old full-screen signer has been replaced by the same in-app consent surface used by embedded apps, with the animated identity circle as a brief signing indicator and an explicit completion state. Editing an identity now ends on a dedicated success screen that reports relay coverage and the event ID instead of disappearing back into the form. The app developer guide defines this platform-owned NIP-07 flow and its browser/Companion test matrix so apps do not add a second signer UI.

- **Discovery merchandising is now owned by the signed app registry.** The catalog declares the Popular Apps set and contribution promotion; Discover renders two desktop rows of popular apps, then the “Your node. Your source.” banner, then the remaining apps. GitWorkshop uses a cache-busted copy of its current upstream mark, and its catalog entry identifies the canonical Archipelago maintainer npub.

- **Companion opens Source in its native WebView and installs the node certificate.** GitWorkshop is a top-level page in the Companion in-app browser—not a dashboard iframe—and its injected provider uses the contained, consent-gated signer broker. The generic native launcher turns relative app paths into complete URLs before handing them to Android. The Node certificate button uses Android's system credential installer in the companion instead of an unsupported WebView download.

- **Node certificate guidance now covers installation and the failures people actually see.** Settings includes the complete macOS, iOS/iPadOS, Windows, Android, Linux, Firefox, and Arch/Manjaro steps; reminds users to restart browsers that cache trust decisions; separates certificate trust from DNS; and maps common browser symptoms to their likely cause.

- **Tab and Companion Nostr sign-in no longer loses the broker or an early identity choice.** The signer route validates the shared app-gate session with the implemented, authenticated `system.get-hostname` RPC instead of the nonexistent `system.get-version`. The provider also exposes a sticky identity subscription so a GitWorkshop React listener that mounts just after selection still completes the normal NIP-07 login. The dashboard service worker no longer precaches the signer route or provider, preventing an old bridge from surviving an update. This repairs GitWorkshop automatic login and IndeeHub's external mobile-browser flow.

- **The App Store now makes Archipelago's source an invitation to contribute.** GitWorkshop has its real upstream icon and source-focused description, plus a dedicated “Your node. Your source.” banner explaining that users can browse the code, clone with ngit, and send issues, patches, and reviews over Nostr.

- **Source now packages GitWorkshop instead of maintaining a separate Nostr Git interface.** The pinned upstream client runs read-only behind the authenticated app gate, launches at the dashboard's same origin under `/app/archipelago-source/`, and uses the node's consent-gated NIP-07 bridge. The upstream revision declares no license; Archipelago's owner accepted that redistribution risk without representing the client as licensed. Production publication still requires a tested canonical Archipelago NIP-34/GRASP announcement.

- **Changing the node password now reports a wrong current password directly.** The backend was already rejecting the request before changing either the web or SSH password, but its error sanitizer replaced that safe, actionable explanation with “check server logs.” The real validation error now reaches the password dialog.

- **The periodic container doctor runs from the same canonical path used by OTA updates.** Its systemd unit and embedded bootstrap still pointed at the retired source-checkout path while release updates installed the script under `/opt/archipelago/scripts`, leaving the doctor failed on nodes without that checkout. ISO, OTA bootstrap, and the deployment smoke test now agree on the `/opt` path.

## v1.8.11-alpha (2026-09-07)

- **Cuprate now syncs without burning a core for days.** The app's shipped config now enables Cuprate's checkpoint-backed `fast_sync` path, raises the database cache to 8 GiB, and gives the container a 10 GiB memory limit so the cache has real headroom. A live comparison that motivated the change saw the affected node sit around 45% CPU while the corrected config held near low single digits at the same chain height and block rate. The restricted RPC remains fronted through the safe app gate/Tor path.

- **OpenWrt Gateway setup is documented from a real install, and two setup bugs are fixed.** The new guide walks a node operator through flashing a GL.iNet AX3000 to stock OpenWrt, pairing it with Archipelago, and installing TollGate pay-as-you-go WiFi. The installer now finds `opkg`/`apk` through the router's actual `PATH` instead of assuming `/usr/bin`, the UI no longer sends an empty password over a saved router connection, and the pinned TollGate package moves to `v0.5.0` with a native `.apk` install path where upstream provides one.

- **Release publishing now checks the public Gitea download links before a manifest goes live.** The publisher already fetched every artifact back and verified its size and SHA-256; this release adds a second guard for the release page itself, so a bad Gitea `ROOT_URL` or proxy setting cannot publish working files behind broken public HTTPS download links.

## v1.8.10-alpha (2026-09-02)

- **Lightning sends work again — v1.8.9's payment switch lost the fee budget.** Moving payments to LND 0.21's supported route (Router.SendPaymentV2) shipped without a fee limit, and the v2 API treats an absent limit as **zero allowed fees**: every real route carries a routing fee, so the pathfinder rejected them all and the wallet answered "No route to the recipient" on every send — all day, on healthy channels with plenty of liquidity. The router debug log made it unambiguous (`fee_limit=0 mSAT` on every failing wallet payment; the same payment succeeded by hand the moment a fee limit was set). Payments now carry lncli's default budget (the payment amount), the wallet's amount handling for zero-value invoices is preserved, and a unit test pins the limit can never be zero again.

- **A channel that drops its peer link now heals itself — on every node.** Restarting LND (an app update, a reboot, container churn) can leave a channel's peer connection down for hours while both endpoints keep the channel flagged disabled in the routing graph: the node looks perfectly healthy, the wallet shows balance, and every payment in either direction fails "no route to the recipient". Observed live: a node's only channel sat unroutable for ~17 hours after the LND 0.21.2 update, with no sign of it in any dashboard. The daemon now watches the channel graph as desired state — every open channel should have a live peer — and reconnects any that don't, using the peer's advertised addresses. Nodes without LND are untouched; an unreachable peer is retried gently, not hammered.

- **The Lightning wallet states the node's real funding state instead of "you have no channel."** Trying to send while a freshly opened channel was still waiting for on-chain confirmations — or when all its balance sits on the far side — raised a modal that claimed the node had NO channel at all (the outbound sum is legitimately zero in both states), pointed the user at opening a second channel, and — for payment routing failures — even showed the *receiving* copy. The funding gate now reads the channel list it already fetched: a confirming channel gets "it unlocks automatically once confirmed, nothing is needed from you", a far-side balance gets "you can receive, but there's nothing to send right now", a routing/liquidity payment failure says so instead of claiming channel problems, and only a genuinely channel-less node keeps the open-one guidance.

## v1.8.9-alpha (2026-09-01)

- **Lightning sends work again after the LND 0.21.2 update.** LND 0.21 removed the old synchronous payment route the node's backend paid through (`/v1/channels/transactions`) — every Lightning send answered the literal "Not Found" and the wallet showed "Payment failed: Not Found". The backend now pays through the supported Router.SendPaymentV2 route, keeps the same settle-then-report behaviour (a slow multi-hop payment is still tracked to completion, never falsely declared failed), and translates LND's failure reasons into plain advice. A new gate test speaks the payment route directly against the running LND, so an image/backend skew like this can never ship silently again.

- **The node no longer pins HSTS — HTTP access is a supported mode, and it stays working.** The HTTPS listener used to send `Strict-Transport-Security: max-age=31536000; includeSubDomains`; browsers that visited HTTPS once cached that and then silently upgraded the still-open HTTP dashboard's calls to HTTPS, which is a scheme change — cross-origin — so every request died as "CORS blocked / Failed to fetch" while the node was perfectly healthy. The HTTPS listener now actively clears the cached policy (`max-age=0`) and port 80 sends no HSTS at all, which is deliberate: the node's certificate is optional and self-signed, and devices that haven't installed the CA must keep plain-HTTP access (that's what Settings → Node certificate is for). If your browser already cached the old policy, visiting the dashboard over HTTPS once after this update clears it; a gate test now refuses any config that reintroduces the pin.

- **App frames open over HTTPS again — including the ones that "did not connect."** The launcher asked the signed catalog for each app's port policy under the name you click ("Mempool Web", "Bitcoin Knots"), but the catalog declares those ports under the manifest that owns them (the Mempool web container, Bitcoin UI). The lookup missed, the launcher handed the iframe an `http://` address, and the browser blocked it as mixed content — the app tile went blank or spun forever. Port resolution now follows launch aliases (mempool-web, bitcoin-knots/bitcoin-core, lnd, electrs and friends), falls back to a port-wide catalog scan when the id is unknown, and the catalog is warmed as soon as the dashboard loads rather than only in the App Store, so the very first app you open already knows which ports serve TLS.

- **Signing in to IndeeHub with Nostr works over HTTPS.** The NIP-07 bridge compared the app frame's origin for exact equality with the recorded `http://` app URL — a frame the browser upgraded to HTTPS (or any scheme change) was silently ignored, and replies addressed to the stale origin were refused outright, so Nostr sign-in quietly did nothing. The bridge now matches host and port (scheme intentionally ignored) and always replies to the frame's real origin.

- **Nginx Proxy Manager starts again.** Converting it to a platform manifest dropped two things its image needs: the `/etc/letsencrypt` mount its boot script hard-requires, and the `NET_BIND_SERVICE` capability its internal nginx needs to bind ports 80/443/81 under the orchestrator's `--cap-drop=ALL`. The result was an endless start/die loop (a node watched it restart 3,176 times). Both are declared in its manifest now, its certs live on unchanged under the same persistent app directory, and the signed catalog carries the fix so installed nodes heal on the next update.

- **Portainer's first-run token is in the app page, not buried in "server logs."** New Portainer versions mint a one-time setup token on a fresh install and print it only to the container logs — on an appliance that meant telling the user to go read a server log to get into their own app. The token now appears in the same launch interstitial as app login credentials (with a copy button), only while first-run setup is actually pending; once the admin account exists the card disappears on its own.

- **The Lightning wallet states the node's real funding state instead of "you have no channel."** Trying to send while a freshly opened channel was still waiting for on-chain confirmations — or when all its balance sits on the far side — raised a modal that claimed the node had no channel at all (the outbound sum is legitimately zero in both states). The funding gate now reads the channel list it already fetched: a confirming channel gets "it unlocks automatically once confirmed, nothing is needed from you", a far-side balance gets "you can receive, but there's nothing to send right now", a routing/liquidity payment failure says so instead of pointing at channel setup, and only a genuinely channel-less node is sent to open one.

## v1.8.8-alpha (2026-09-01)

- **SSH over the mesh is now a first-class setting.** Settings gains an "SSH over mesh" card: off by default, and when you allow it the node's mesh firewall opens port 22 — either to every mesh peer (behind an explicit "I understand" confirmation, because that's a real exposure) or only to the mesh addresses you list. The rule is owned by the node (the `90-ssh.nft` drop-in), so it survives upgrades and daemon reinstalls, and the card tells you up front whether sshd is running, whether it listens on IPv6 (the mesh is IPv6-only — this is what a broken attempt looks like before it happens), and whether password login is on (keys-only is the recommended pairing). From Termux on your phone, `fipssh <user>@<node-npub>` connects once the toggle is on — the npub is the durable address, and the command is shown with a copy button on the card.

- **The App Store now lists apps — not parts of apps.** The signed catalog carries every manifest because the node's update layer needs their pins, and the store briefly listed them all: Mempool API, LND UI, Bitcoin UI, the Pine voice engines, the IndeeHub and Immich backends, the mesh router and friends. Components are hidden from the store listing (they still appear where they belong — the Services tab of My Apps, once installed), and four entries that never earned a tile are gone outright: MorphOS server (old), the Web5 DID wallet, Lightning Stack (an untracked upstream bundle — LND covers the need), and CryptPad (never tested).

- **App icons now persist everywhere, in the proper container style.** Two fixes: installed apps render the icon from their own manifest — Cuprate no longer falls back to the generic A-mark on its Services tile — and the store grids (the Discover page) apply the same icon container treatment (backdrop, border, shadow) as My Apps, the detail pages, and Home. Manifest-declared UI apps also classify correctly again: Alby Hub installs into My Apps with a working tile, not into Services, because a probe miss no longer buries an app the manifest itself says has a frontend.

- **Installing from the store keeps you on the store page.** The install progress lives on the tile itself and the app appears in My Apps when it lands — no more being yanked to My Apps mid-browse.

## v1.8.7-alpha (2026-08-31)

- **What's New really does stop at v1.8.0 now.** The first correction removed old generated release blocks but missed six much older hand-written v1.2 sections at the bottom of the modal. Those sections are gone, and the release check now recognizes and rejects that legacy format too, so the history floor cannot falsely pass again.

- **The installer carries the same corrected release and Companion 0.5.28.** Its artifact gate now checks the companion APK version and the v1.8.0 What's New floor inside the finished ISO, so a stale frontend or phone app cannot be published under the current release label.

- **Crash dumps work on fresh installs as well as upgraded nodes.** The installer gate checks every kdump package inside the finished ISO, and `makedumpfile` is installed explicitly rather than accidentally relying on a recommended dependency that the minimal image deliberately omits.

- **Apps open over HTTPS when your node does.** Connect to your node over HTTPS and the apps you open — Vaultwarden in its own tab, BTCPay, Grafana, and the rest, on a remote browser or in the phone's in-app browser — now open on the same secure connection instead of silently dropping to plain HTTP. The node's app gate already served TLS on every app port; the dashboard was handing out `http://` addresses regardless of how you reached it. Ports the gate does not front (plain-HTTP publishes, and the API ports like Cuprate's RPC) deliberately stay on `http` — `https` there would simply fail to connect. Plain-HTTP access (the kiosk, LAN browsing) is unchanged.

- **Every app in the store is now a first-class platform app.** The remaining platform apps carry full manifests: the node's app gate fronts their web ports (TLS on the same port, the node login where appropriate, embedding fixes, Tor), installs go through the orchestrator like every other app, and their pins live in the signed catalog. Ollama stays loopback-only — it is the assistant's local model backend, not a web app. Retired apps are dropped from the catalog, and Cuprate's manifest — which carried a duplicated metadata block that strict parsers reject — is fixed.

- **Newly signed apps appear in the App Store immediately.** The App Store now serves the release-signed catalog the node has already fetched and verified — so publishing a signed app (like Cuprate) makes it appear for every updated node without waiting for a dashboard release. The unsigned community catalog remains only as a fallback for nodes that can't reach the registry. The same signed catalog now also decides which ports serve TLS, so nothing is upgraded to `https` that can't answer it.

## v1.8.6-alpha (2026-08-31)

- **Companion 0.5.28 is included in the node download this time, with the work that missed v1.8.5.** The companion hub can back up and restore its node list, act as a NIP-46 remote signer, and shows each paired node's FIPS mesh address with tap-to-copy. For Termux users, the included `fipssh` helper turns a durable node npub into its mesh address, so `fipssh user@npub1…` can reach SSH once that node has explicitly allowed port 22. The node-side “SSH over mesh” firewall toggle is not claimed here—it still needs implementation and remains off by default.

- **What's New now starts cleanly at v1.8.0 and is guaranteed to be newest-first.** Older alpha history no longer overwhelms the useful recent changes, the three stray v1.7 entries that appeared above current releases are gone, and the release check now fails if either the ordering or the v1.8.0 history floor drifts again.

- **A release can no longer advertise itself before its files exist.** New releases are prepared behind a pending manifest; the publisher uploads the backend and frontend, downloads both back and verifies their size and hash, and only then promotes the signed manifest to the path nodes read. The manifest generator also includes every curated What's New item instead of silently stopping after the first ten physical changelog lines.

## v1.8.5-alpha (2026-08-30)

- **Cuprate — an independent Monero node — is now an app.** Monero consensus validated by a second, unrelated codebase (Rust), the same layer of security-in-depth Bitcoin gets from Knots. Review caught two problems before anything shipped: the unrestricted RPC that can move funds stayed bound to the container's loopback (never published to the node, let alone the LAN — anything on the node could previously have reached it), and its restricted RPC moved off port 18089 to avoid colliding with Penpot. Honest caveat: upstream has cut no stable release yet, so the pin tracks an exact preview build (0.1.0-preview-18-g618ff14) and moves to their first tagged release when there is one.

- **A frozen node now explains itself — and comes back on its own.** The host now captures a memory dump into /var/crash when the kernel panics *or* wedges (a hung kiosk used to sit dead until someone power-cycled it; now it dumps, reboots itself, and leaves the evidence behind), and records failing-memory signals (ECC errors) into a database as they happen. This is the first change delivered by a new host-update channel: the node's own updater now carries OS-level packages and settings to already-deployed machines — the crash-kernel's memory reservation is the one part that waits for a reboot, and the node says so rather than pretending.

- **Uninstalling an app can no longer report success when it failed.** The declarative path used to swallow every teardown error and report the app uninstalled, leaving the tile behind and the truth in the logs. A failed uninstall now stops and shows the real per-app errors, so "still there" is never presented as "gone".

- **Pictures to internet-only mesh contacts work now.** Sending an attachment inline always took the radio path and failed with "Peer is federation-only (no radio twin)" for contacts reachable only over the internet — and the size-adviser kept recommending a radio transfer those peers can't receive. Both fixed: inline sends route over the federation when that's the only way to reach the peer, and the advice no longer offers radio-only transfers to radio-unreachable contacts.

- **Disk cleanup finally has honest numbers.** Space "free" on a drive was counted including the slice the filesystem keeps reserved for root — roughly 5% of the disk, 92 GB on one dev box — so the automatic cleanup that's supposed to kick in at 90% never triggered and stale container images piled up unnoticed. Reserved space now counts as used, which is what the threshold was always meant to measure.

- **Three small screens that were lying to you, fixed.** The "Bitcoin is synced — fund your wallet" toast no longer appears on a node where the wallet it means (LND) isn't installed — it points at installing LND instead. The seed-reveal screen hides its third prompt unless the password actually fails to decrypt (the backup passphrase only exists if you set one). And multi-version store cards stop quoting a version number you'll be asked to choose on the next screen anyway.

- **Mesh notifications survive a refresh, and a stale router no longer hides the fix.** Radio message unread counts are now remembered per contact instead of guessed from session state (the "one new message showed 11 unread" bug), cover Meshtastic, MeshCore and Reticulum alike, and deep-link to the right conversation; a single new message announces itself once. Separately, when the cached router address goes stale, the error card gains a "Reconfigure router" action instead of a Retry loop that can never succeed.

- **The app updater now knows what upstream shipped.** Every app's manifest records where it comes from — including the odd corners (GitLab-only projects, ghcr-only images) — and a checker sweeps all of them against upstream releases, so a pin that quietly rots for months is now visible instead of invisible. The first full sweep found 27 pins behind; the safe patch-level ones shipped with this release (strfry, BTCPay Server 2.4.3, the two nginx frontends), and the major jumps that may carry data migrations are deliberately held for their own careful passes.

## v1.8.4-alpha (2026-08-20)

- **Apps with their own login can now skip the node's login screen — Gitea and BTCPay Server do so out of the box.** Some apps bring a complete account system of their own, and putting the node's password page in front of them broke real workflows: git clients can't answer a browser login, and a BTCPay checkout link handed to a customer must open for that customer. These apps are now served directly on their own login, while the node still fronts the connection for everything else it does (embedding fixes, the "app is restarting" page, Tor). Every app gets a new **Settings → app → Access control** switch, so you can put the node login back in front of any app — or take it away from one — with one click, effective immediately. App developers declare the default in their manifest (`auth: open`), documented in the developer guide.

- **The phone remote now works inside apps on the TV — tap, scroll, and type everywhere.** The companion remote and keyboard drove the dashboard beautifully but died at the edge of any app screen (Gitea, BTCPay, and friends): for the browser, each app is a separate website embedded in the page, and simulated input is forbidden from crossing that wall. The on-screen display now accepts the remote's input the way a real mouse and keyboard arrive — below the page, through the browser itself — so it lands anywhere on screen, app screens and tabs included. Taps click, two-finger scrolling scrolls the app, and typing goes into whichever field you tapped. Existing kiosks pick this up with the update, no reinstall needed.
- **While you're driving with the phone remote, the old mouse pointer gets out of the way.** The computer's own pointer used to sit frozen wherever the physical mouse last left it — a second, dead cursor next to the live orange one. It now hides while the remote is in use and returns half a minute after the last remote input.
- **"Are you sure?" questions no longer freeze the remote.** A handful of confirmations (clearing mesh history, rebooting, deleting a backup, uninstalling an app) used the browser's built-in popup, which stops the whole page — including remote input — until someone clicks it with a real mouse. From the couch, that meant asking a question you couldn't answer. All of them are now proper in-app windows in the house style, fully driveable by remote.
- **A mesh radio now connects no matter which port it's plugged into — or replugged into.** Moving a radio to a different USB port could leave the mesh silently down: the node only checked a short fixed list of port names (a radio landing outside it was invisible), a hand-set serial-port override quietly outranked the device you'd just approved in the "Radio detected" window, and one whole family of boards (Espressif-based radios like recent Heltec/T-Deck models) never received a stable device name at all — the exact combination found live on a fleet machine this week. All three are fixed: every serial port is scanned, choosing a radio in the detection window clears any stale override, and Espressif boards get the same stable name as everyone else.
- **Mesh signal strength is honest now.** Every peer heard over Reticulum radio reported a signal strength of exactly 0 — which is also what you'd see with no radio at all, and what peers reached over the internet showed. Real receptions now show their true signal reading, and anything that arrived over a relay or the internet says so by showing none — so "the radio is working" and "the internet is doing the radio's job" no longer look identical. (The reading depends on the radio's firmware reporting it; boards that don't report per-packet signal stats show "unknown" rather than a made-up number, and the new radio diagnostics show at a glance whether yours reports them.)
- **A background error that repeated every 90 seconds, forever, is gone.** After setting up a node from its recovery phrase, the node kept introducing itself to its federation partners with its old temporary identity papers while signing with its new ones — every partner rejected the introduction, and both sides logged an error about it every minute and a half until the next restart. The identity switch now updates everything at once, a rejected introduction is no longer misreported as delivered, and a partner who has already answered is no longer re-asked on every cycle.

## v1.8.3-alpha (2026-08-14)

- **The network map on TVs: no more blank page, no more frozen page — and it moves again.** The map's entrance animation needed a smoothness that TV kiosk hardware can't always deliver, so the page could sit blank until a refresh; the previous fix cured the freeze by stopping the animation entirely, which went too far. Now the map appears instantly with everything already in place, then resumes its calm orbital motion at a gentler pace suited to TVs. Resizing or rotating any screen also redraws the map properly instead of leaving it tiny, stretched, or empty.
- **The dashboard's corner logo is back to normal.** The new glossy paint finish was meant for the big emblem on the screensaver, intro, and login screens — it had quietly spread to the small logo in the dashboard header, where it looked wrong. Each screen now gets exactly the treatment intended for it.
- **App icons no longer vanish in My Apps.** The freshly restyled Alby Hub and phoenixd icons could render as blank squares in some views — a subtlety in how the icon files declared their size. Fixed at the source, and the icon tool app developers use now produces immune files.

## v1.8.2-alpha (2026-08-13)

- **An app that can't be shown inside the dashboard now becomes a tab app by itself.** A few apps refuse to render inside another page no matter what — they break out with their own code or insist on owning the whole browser window. Opening one used to mean staring at a grey pane. Now the dashboard notices, offers the app in its own tab, and remembers: from then on that app's button opens a tab directly (with the little launch icon that tab apps carry), first click, every time. If a later update makes the app embeddable after all, the dashboard notices that too and goes back to embedding it.
- **The logo emblem got its glossy black paint finish — properly this time.** The circle behind the A on the screensaver, intro, and login now wears a deep wet-paint look: warm light blooming from the top edge, fine grain so the dark tones stay smooth instead of banding, and no more ring border. (An earlier rougher version of this experiment briefly shipped by accident and then vanished depending on which screen you were on — this is the finished, deliberate one, everywhere.)
- **New app icons now match the store's look, on every screen.** Alby Hub and phoenixd arrived with edge-to-edge logos that ignored the breathing room every other app icon has, and the app detail page skipped the icon backdrop entirely. Both icons are re-set on the standard canvas, the detail page now applies the same icon treatment as the store tiles, and app developers get a one-command tool that puts any logo onto the house canvas automatically.

## v1.8.1-alpha (2026-08-13)

- **Apps that refused to open inside the dashboard now embed like everything else.** Some apps ship browser headers that forbid being shown inside another page — correct hardening on the open web, but inside Archipelago it produced a dead grey pane when you opened them from My Apps (Alby Hub was the first to hit it). The app gate, which already checks your login on every request to an app, now removes just those framing headers on the way through; each app's own content-security rules pass through untouched. No more per-app proxy workarounds.
- **The network map no longer freezes kiosk TVs.** The animated federation map at 4K was too much for the deliberately conservative graphics settings the on-screen display used on every machine — settings chosen years back to stop audio crackle on much older hardware. Two fixes: on kiosk screens the map now opens in its flat 2D view (the 3D globe is one tap away, and remembered) and animates at half rate — invisible from the couch, half the work. And the display itself now recognizes what machine it runs on: older kiosk boxes keep the proven careful settings, modern ones finally get real GPU rendering.
- **New Settings → Display → Graphics choice for the on-screen display.** Auto (recommended) picks the right rendering mode for the machine by itself; Compatibility forces the most conservative mode if a screen ever stutters, tears, or crackles; Quality forces full GPU rendering on hardware the automatic detection doesn't recognize. Changing it restarts the on-screen display, like the size presets.

## v1.8.0-alpha (2026-08-12)

- **Archipelago is now open source.** The full source code of the node you are running — the orchestrator, the dashboard, the app platform, the mesh, the release tooling — is published for anyone to read, build and audit at source.archipelago-foundation.org/lfg2025/archy. A node that holds your money, your files and your communications should not ask to be taken on faith: from this release onward you, or anyone you trust, can see exactly what it does and follow every change we make in the open.
- **Installing an update is reliable again, and tells you what happened when it isn't.** Some nodes could download an update but never apply it — the button stayed on "Install", and no amount of retrying worked. The cause: applying the update consumed the downloaded files as it went, so if any one step hit a snag partway through, the leftover files were incomplete and every later attempt failed the safety re-check forever, needing a technician to recover. Applying no longer consumes the download — a failed apply can always be retried from the same files — and the pieces are now applied in a fixed order with the program itself last, so a hiccup can't leave a half-swapped node. When an apply does fail, the screen now shows the real reason and what to do ("download the update again"), and offers Download again instead of a dead "Install" button, rather than a generic "it failed".
- **Video on the kiosk stops tearing.** The kiosk's display had no vertical sync at all, so fast motion — IndeedHub films especially — showed horizontal tearing lines. The display driver now syncs every frame to the panel (no extra hardware needed, existing kiosks pick it up with this update), and on machines with a GPU, video decoding moves off the CPU onto the video hardware — smoother playback that also leaves more headroom for audio, not less.
- **The Back button finally does what you expect.** Pressing Back — the mouse's side button on a kiosk, a swipe on a phone, the toolbar button in any browser — used to navigate the screen underneath an open window, or leave the dashboard entirely. Back now closes the topmost open window first, one per press, exactly like a native app; closing a window yourself never leaves a phantom entry that makes you press Back twice.
- **No more bare IP addresses in your update or app-registry settings.** The update mirrors and the app registry each listed the same server twice — once by its proper name, once as a raw `http://146…` address left over from before the domain existed. The raw-address entries are retired: new nodes never see them, and existing nodes clean them out of their saved lists automatically on the next read. Everything now goes through the named, TLS-protected origin — which was always the same machine.
- **The phone companion app downloads over the proper domain.** The download QR pointed at a raw address over plain HTTP; it now points at the same file on the https domain. Scanning it gets you an encrypted download from a named server.
- **The Receive window now tells you when the money is on its way.** Previously it showed a QR code and left you to check elsewhere whether anything happened. Now, the moment the sender's transaction is broadcast, the QR gives way to a clock: the amount, the transaction ID (tap to copy), and a note that the funds arrive on their own — with a single Done button. If you keep the window open, the clock becomes a green check at the first confirmation. Verified live on a real node: payment detected within seconds of broadcast.

## v1.7.129-alpha (2026-08-10)

- **Every app is now supervised the same way — the last stragglers moved under systemd.** Five apps (Jellyfin, Nextcloud, Home Assistant, Uptime Kuma, Vaultwarden) still ran outside the node's per-app service management for a technical reason: their networking style died with whatever process started it, so they were kept alive by a separate workaround. That workaround is retired: these apps now migrate themselves onto the same managed units as everything else — own service, restart-on-anything, a ten-second breather between restarts so their networking can release its ports cleanly. The migration happens automatically on the node's next housekeeping pass, touches no app data, and was watched live on a real node: both test apps moved over on the first pass and came back healthy.
- **Leftover companion screens are cleaned up again — driven by real records this time.** When an app is uninstalled, its helper screen (the UI tile that fronts it) should go too. That cleanup was switched off in an earlier release after it wrongly removed the Bitcoin screen from a node whose Bitcoin was installed — it had been guessing "installed" from what happened to be running, and a separate bug made a running app look absent. The node now keeps a durable record of what you have installed, written at install time and cleared only by a real uninstall, and the cleanup consults only that record. If the record can't be read, the cleanup does nothing at all — "I couldn't check" is never treated as "nothing is installed" — and a helper must be orphaned for a sustained period before it is touched.
- **A warning that fired every minute on every node is gone.** The app catalog and the node disagreed about where Grafana's software comes from, so the node ignored the catalog's answer and logged a complaint roughly every 75 seconds, forever. The catalog was right — Grafana is served from the fleet's own registry, like Bitcoin Knots — and the node's records now agree with it.
- **The federation map became a real map.** The network view is now a 3D orbital scene of your federation — nodes as a point-cloud globe with calm motion, auto-fit centring, and a 2D top-down toggle that portrait and mobile screens use by default, with the scene filling the viewport instead of sitting in a letterbox. Inbound peer requests appear live on the map as blinking nodes you can accept or reject in place, and revisiting the view no longer replays the whole intro — the scene updates in place.
- **An app that's mid-restart shows a page that says so — and comes back by itself.** When an app's screen was briefly unreachable behind the gate, the browser got a bare error; it now gets a named page for that app that retries on its own until the app answers.
- Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release; the supervision migration and the cleanup re-enable were verified live on one node (both apps migrated and healthy, cleanup correctly idle).

## v1.7.128-alpha (2026-08-10)

- **The discovery list stops showing ghosts.** Every reinstall of a node mints a new discovery identity, and the old identity's announcement could never be removed from the public relays — nothing holds its key anymore — so the "Discoverable nodes" list slowly filled with entries that led nowhere. Announcements now expire: your node re-announces itself twice a day, each announcement carries a 48-hour expiry that relays honour, anything older than that is ignored when reading, and switching discovery off — or factory-resetting the node — actively overwrites the announcement before it can become a ghost. Old ghosts from earlier versions stop being shown immediately and age off the relays on their own.
- **You can name your node when you make it discoverable.** Turning discovery on now asks for an optional display name — it travels inside the public announcement, so other nodes' discovery lists show "Dorian's basement node" instead of a bare npub. The name is public by construction, capped at 32 characters, and blank is fine: you list as npub only. Toggling discovery off and on remembers the name; you can clear it the same way you set it.
- **The discoverability panel now shows what the network actually sees: your node's npub.** It previously showed your Tor address — which is precisely the thing the announcement never contains (your address stays private until you approve a peer). The npub, the identity other nodes discover you by and send peering requests to, is now displayed there with a copy button.
- **The seed screen stops flashing while the node starts.** During first boot, the lock icon and "server starting" text blinked in and out every few seconds while the node came up — each silent retry briefly emptied the screen. The waiting state now holds steady, with its elapsed timer, until the node answers.
- **A node that already has an identity now explains itself on the seed screen.** Reaching seed creation on a provisioned node used to surface a developer message about "the authenticated system.factory-reset". It now says what you can actually do: sign in normally, or factory-reset the node from Settings to start it over.
- Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release; the changes were verified by operator UAT on a live node.

## v1.7.127-alpha (2026-08-09)

- **Your node now has its own assistant.** This is the first release to ship AIUI: a conversational screen that can answer from your node's own content — your films, music and files come first, the open web second — and can act on the node itself: install or remove an app, check what's running, or queue up your media, all through a fixed list of vetted actions rather than free rein. It is off-limits to your data until you say otherwise: every data category starts closed, grants are made in Settings → AI Data Access and live on the node itself, and anything that changes the node asks you to confirm in the dashboard's own chrome first — a declined action stays declined. What leaves the node is screened: your API key is stored encrypted and never written in plain text, credential-shaped strings are scrubbed from app logs before the model sees them, your public address and Wi-Fi name are stripped from network answers, web search is gated behind your login session, and cloud-bound text passes a secret scan on the way out. Three model backends are supported — Anthropic's API, a local Ollama, and pay-per-use Routstr with a hard prepaid budget ceiling — and mesh peers can reach the same loop with `!ai`.
- **Tor now tells you the truth, heals itself, and the Restart button really restarts it.** Three nodes ran for days with Tor completely dead while the dashboard said "Connected" — the indicator was reading a leftover address file, not the daemon, and the restart button reported success without checking. The cause was a configuration line Tor can never bind on our systems; a node could re-break itself from a single settings change. The node now refuses to write that line, checks Tor with a real connection instead of a leftover file, repairs its own Tor configuration at every start, and the Restart button only claims success once Tor is actually answering. Onion addresses that had silently never been published (BTCPay's included) come back with it.
- **Inviting another node as Trusted works again — on every node.** Generating a Trusted invite, or promoting a peer from the dropdown, silently failed everywhere: the security prompt that asks for your node password could never appear, because the message requesting it was being scrubbed out of the reply on its way to your browser. The prompt now opens, and if a trust change fails, the error appears inside the window you are looking at instead of hidden behind it.
- **The mempool explorer actually connects now.** The page loaded but sat empty forever. Three separate causes stacked up: the block index had spent days rebuilding without anything saying so, and then two different layers of the node's plumbing were dropping the live-data connection the page depends on — so everything reported healthy while your screen showed nothing. All three are fixed, and the node's own health checks now test the real connection a browser makes, so this cannot pass unnoticed again.
- **Apps no longer vanish after stopping cleanly.** A stopped app's container is deleted by design, but the restart policy meant an app that exited cleanly was never brought back — it simply disappeared until reinstalled. Backends now restart in every case, the node remembers what you have installed so a missing app is recreated rather than forgotten, and this release repairs the incorrect policy on apps installed by earlier versions.
- **Your Bitcoin node will not silently change software versions anymore.** "Latest" previously meant different things in different places — one path installed a newer build that deliberately halts until you make a network-rules decision, which froze one node's sync at a fixed block while it reported itself fully synced. Bitcoin Knots is now pinned to an explicit, known-good version; changing it is a decision you make, never a side effect of an update.
- **Smaller fixes:** the AI data-access settings now say plainly which categories the assistant can see but not act on; the transactions window's tab bar is transparent glass instead of a black block; BTCPay logins no longer fail with a server error when the node is under heavy load right at that moment.

- **You can now replace your Lightning connection keys from Settings, without touching a terminal.** The tokens wallet apps like Zeus use to reach your node are bearer keys: anything that has ever seen one can spend from your node until they are replaced, and there is no way to cancel one individually. Replacing them was previously a script you had to SSH in and run, which in practice meant it never happened. Settings → Lightning credentials now shows when yours were issued, which node they belong to and how many channels must survive, then does the whole job behind your node password — with a step-by-step progress list, and a refusal to call it a success unless it has confirmed your node identity and every channel came back. Your coins and channels are not touched: nothing is closed, and the wallet is never re-created. Afterwards you re-pair Zeus by scanning the Lightning app's QR code again.
- **Replacing those keys no longer silently breaks BTCPay Server.** BTCPay holds its own copy of the key, and that copy cannot repair itself — so a node that replaced its keys ended up with BTCPay running, healthy, and unable to take a single Lightning payment, with nothing anywhere saying why. The dashboard now updates BTCPay's copy as part of the run and restarts it around its existing data, and the Settings screen warns you if it finds a node already stuck in that state. The command-line script fixes the same gap.
- **Lightning stops getting stuck locked on a busy node.** Lightning opens its databases before it will accept the password that unlocks the wallet, and on a loaded node that took nearly three minutes — longer than the node was willing to wait. Giving up restarted Lightning, which started the slow open again, so the wallet stayed locked forever and everything depending on it stayed broken. The node now waits as long as it takes. A genuinely wrong password still fails immediately.
- Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The full 5x real-node lifecycle gate was not run for this release; reboot survival was verified directly on a live node — all installed apps returned after a cold reboot, uninstalled apps stayed gone, and restart policy was confirmed on every managed unit.

## v1.7.126-alpha (2026-08-07)

- **The most important fix in this release: the update button could take you backwards onto a version withdrawn for a security hole.** BTCPay Server published 2.4.2 to close a flaw that was being actively exploited — a way past two-factor authentication. Nodes that had already moved to 2.4.2 were then shown an "Update" button offering 2.3.9, the very release being withdrawn, and taking it would have rolled the node back onto the vulnerable version. The cause was that the node only asked whether the two version numbers differed, never which was newer, so any stale record anywhere could present a rollback as an upgrade. It now refuses to offer a lower version as an update, so a stale record fails safe instead of becoming a trap. BTCPay itself is on 2.4.2, and every place that still named the old version — including the fallback installer, which would have installed it outright — has been corrected.
- **An app now reports its own version, not a helper's.** Where an app is made of several parts, the node could read the version of the wrong part: BTCPay showed as "15.17", which is the version of its database, while offering an update to 2.4.2. That is the number update decisions are made from, so a nonsensical pair was being presented as a legitimate upgrade. When the node cannot identify an app's own container it now says so rather than guessing at a neighbour.
- **Your node issues its own certificate, so apps stop being flagged as insecure.** Each node now has its own certificate authority, with a one-step install from Settings, and app screens are served over the same secure connection as the dashboard rather than dropping back to an unprotected one. Apps answer on both the secure and plain address on the same port, so nothing that worked before stops working.
- **An app that is still starting says "starting".** It previously reported "App not reachable", which reads as a failure when the app is simply warming up.
- **Updates and app downloads now come from a proper domain name.** They previously used a bare numeric address over an unprotected connection. Downloads are now encrypted in transit, and the old address is kept as an automatic fallback for nodes whose clock or name lookup is off — the signature, not the address, is what makes either source safe.
- Also in this release: the tool app developers run to check their app description no longer rejects every valid file (it needed a program most machines do not have, and reported the missing program as a broken file); and the node's own security audit, which had been reporting all-clear, now actually inspects the files where credentials had been sitting.
- Housekeeping, disclosed rather than buried: this release removes Archipelago's own infrastructure details from the published source — machine names, addresses and internal working notes — ahead of the code being opened to the public. No behaviour changes for your node.
- Known gaps, unchanged from the last release: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.

## v1.7.125-alpha (2026-08-06)

- **The Lightning, Bitcoin, Electrum and mesh screens work again behind the login gate.** Since the gate went up, those screens loaded their frame and then showed every number as unreachable. The gate was deliberately hiding your login from the apps it protects — right for third-party apps, wrong for the node's own screens, which need that login to fetch your data. The gate now removes only its own credential, and the node's own screens explicitly receive yours. The same mistake was also quietly signing you out of apps with their own logins — Vaultwarden, Nextcloud, Gitea — on every single request; that stops too.
- **IndeeHub heals itself.** Three separate faults fixed: its database helper was recreated with permissions too tight to read its own files (it had crashed and restarted roughly ten thousand times on one node); on another node two of its seven parts could never be recreated at all because of how the node asked for their storage — it would remove the old part and then fail to build its replacement, leaving the app half-missing forever; and a regenerated password could lock the app out of a database that keeps the original. The storage fault fixes the same trap for every future multi-part app.
- **A missing piece of a running app now gets put back automatically.** If one container of a multi-part app disappears while its siblings are still running, the node treats that as a hole to repair rather than a choice to respect, and rebuilds the missing piece. An app you actually uninstalled stays uninstalled.
- **Send and Receive open clean every time.** Whatever you typed last — an address, an amount, and above all an armed "send all funds" toggle — no longer quietly carries over into the next payment. Choosing "send all funds" also shows the amount being swept instead of a confusing 0.
- **A sweep that cannot happen now says why.** Trying to sweep a balance that is below Bitcoin's dust minimum (about 546 sats) or not yet confirmed used to fail with "check server logs"; it now explains that no transaction can be built from those coins.
- **The camera scanner option no longer vanishes on desktop.** Browsers only allow the live camera on secure (HTTPS) pages, and the scan window silently hid the camera choice on plain connections — which read as "the scanner is gone". The option now stays visible and explains itself, and the photo and paste routes always work. The companion app's built-in scanner is untouched.
- **App data folders can no longer be "repaired" into a state the app cannot use.** When the node fixed a folder's ownership through its fallback path, it wrote the container's raw user number instead of the translated one, so the fix reported success while the app still could not open its own files — one node's BotFights restarted every ten seconds over exactly this. The translation is now applied.
- Also: the app login page uses the Archipelago mark and stays centred on phones with the keyboard open, app icons in the install window are no longer cropped, and when the node fails to build a container it now records the actual reason instead of a one-line stub that hid the cause of the IndeeHub fault for days.
- Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.

## v1.7.124-alpha (2026-08-05)

- **The most important fix in this release: some nodes were left switched off by their own update, and could not switch themselves back on.** The node replaces its program and then exits, expecting the system to start it again — but nodes installed from older images carried a setting that only restarts the program if it *crashes*. A clean, deliberate exit looked like success, so nothing restarted it, and the node sat dead showing "server starting" with nothing able to start it. One of ours was down for over two hours this way, and three of four checked had the same setting waiting to bite. Your node now repairs that setting itself the first time it starts, so it survives every future update.
- **Portainer opens again.** Its screen reported the app as not responding because the app was quietly refusing to start: nodes have been running Portainer 2.39.1, their stored data was written by that version, and the app list pinned a version from two years earlier — so when the container was rebuilt it landed on the old one, which will not read newer data. The correct version is now pinned, older installs upgrade cleanly, and no data was touched.
- **Bitcoin starts reliably again.** A leftover settings file in the Bitcoin folder — one the node itself kept rewriting and Bitcoin no longer reads — is treated as fatal by Bitcoin, so affected nodes restarted every few seconds forever. The node no longer writes that file, removes stale copies, and treats any that remain as harmless.
- **Every app screen opens from My Apps again.** The login gate refused to be displayed inside another page at all, which is exactly how My Apps opens an app, so protected apps appeared broken. It now allows only your own node to display it, and refuses everyone else — a distinction the old setting could not express.
- **The app login screen now looks like the node's own.** Same rotating artwork, the same panel, the Archipelago mark, and the app's real icon shown as a tile the way My Apps shows it, instead of a plain box with a letter.
- **The Mesh screen uses wide displays properly.** On very large screens it stacked all five panels on top of each other, clipping three of the headings to a sliver and squeezing the map into a letterbox — more screen producing a worse view. It now shows one panel at a time, filling the space, with the map running edge to edge.
- **You can choose how long you stay signed in.** Settings → Account now offers an inactivity timeout and a hard limit, plus an option to re-enter your password before sending funds. TV and kiosk screens are never signed out for sitting idle, because there is nobody there to sign them back in.
- Updates now come from `source.archipelago-foundation.org` rather than a bare address, with the old one kept as an automatic fallback for nodes whose clock or name lookup is off. Also included: clearer wallet errors from ecash mints, and mesh peers reconnecting via their last known address before falling back to the wider network.
- Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication; the correct fix puts them on a private network with the assistant. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.

## v1.7.123-alpha (2026-08-05)

- **Five more screens on your node were readable by anyone who could reach it, and the previous release's own check said they were fine.** The Bitcoin, Lightning, Electrum, FIPS mesh and Fedimint Guardian screens each answered on their port with no login. They were missed because they work differently from ordinary apps: they run directly on the node's network rather than behind its container plumbing, so there was no address to pin and their descriptions listed no port at all — and the node builds its list of what to protect from exactly those descriptions. It therefore neither protected them nor listed them as unprotected. A check that reports success while five screens are open is worse than no check, and this was found by scanning the node from another machine rather than asking the node about itself.
- **What was actually readable was the page, not your money.** Every request on those ports that could have returned a credential — the Lightning connection details, the wallet passthrough, container logs, and every node command — already required a login and still refused without one. The Lightning macaroon fix from v1.7.120 was verified directly rather than assumed. What leaked was the screen itself: layout and code, no wallet data, no keys.
- All five now serve only to the node itself, with the login gate in front of them, exactly like the twenty app screens closed in the previous release.
- **Every port on the node now has a stated policy — there are no undecided ones left.** Eleven ports previously had no instruction either way and stayed open by default. The BotFights arena, the router screen and the Pine voice screen now require the node password. The ones that genuinely cannot take a login page stay open with a written reason: Fedimint's guardian and gateway connections (federation members authenticate to the federation), NetBird's management and dashboard ports (your VPN devices carry their own credentials and cannot hold a browser session, and its dashboard needs its own certificate), Pine's secure listener, and the Lightning REST port, which wallets reach with a macaroon exactly as before.
- Fresh installs are covered too, not just existing nodes. The five screens are delivered as prebuilt images, so a newly flashed node would have come up open even after this fix. All five were rebuilt, published, and then pulled back and inspected to confirm the fix is really inside them.
- Two delivery faults fixed alongside, either of which would have silently undone the above: two of the five screens were reaching nodes through no update path at all, so edits to them never arrived; and a fourth copy of the Bitcoin screen's configuration was being rewritten on every health check, which would have re-opened that port after everything else was corrected.
- Known gaps, disclosed rather than buried: non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. Three voice-assistant ports remain open without authentication; the correct fix puts them on a private network with the assistant. The 5x real-node lifecycle gate was not run for this release.

## v1.7.122-alpha (2026-08-04)

- **Your apps now ask for your node password before they open — over your home network, Tailscale, the mesh and Tor alike.** Until now anyone who could reach your node could open Immich, Nextcloud, Vaultwarden, Jellyfin, Grafana and the rest simply by typing the address and port, with no login at all. Twenty app screens now sit behind the same login you use for the node, showing you which app you are opening, and honouring two-factor if you have it switched on. Logging in at an app address logs you into the dashboard too, so it is one password, not one per app. This completes the groundwork disclosed in v1.7.121.
- **The things that must stay open stayed open.** Zeus and other remote wallets still reach your Lightning node directly, Electrum wallets still connect, and Bitcoin still talks to its peers — those connections carry their own proof of identity and a login page would simply break them. Every one of these seventeen exceptions now has to state in writing why it is safe to leave open, so the list is something you can read rather than something you have to discover.
- **A private address on your node was answering the mesh without a password.** One app's port was marked as being for this machine only, and the part of the node that carries mesh traffic did not know that — it forwarded requests from the whole mesh straight to it. Found while verifying the work above on a real node, not in testing. That path now refuses anything marked machine-only, and the app is reachable only from the node itself, as intended.
- **Tor addresses no longer skip the login.** An app published as a .onion address was handed straight to the app, because a Tor visitor carries no session cookie to check. The login gate now takes those addresses first, closing the last of the four routes that went around it.
- Nodes fix themselves after this update. Apps installed before this system used its current container setup kept their old wide-open address even after the signed list told them to move, and each would otherwise have needed hand-holding on every node. Your node now notices the difference and rebuilds those apps itself, keeping their data, within about half a minute of starting. Verified by putting a node back into the old state deliberately and watching it repair.
- The node had been reading two different sets of instructions about its own apps — the signed list it downloads, and older copies on disk — which is how a port meant to stay private was briefly opened on a test node. Both now come from the signed list, and a port withdrawn from the login gate is released without needing a restart.
- **The key that signs these updates has been replaced.** The previous signing key was exposed where it should not have been, so it is treated as compromised and this release installs its replacement. This update is the last one signed with the old key, by necessity — it is the one that teaches your node the new one.
- Known gaps, disclosed rather than buried: eleven app ports still have no stated policy — BotFights, the Fedimint gateway, NetBird, the voice assistant's own screens and the router screen — and remain reachable without a login until each is decided deliberately; the node reports them rather than guessing, because guessing at an unstated setting caused both incidents behind this work. Three voice-assistant ports are still open without authentication; the correct fix puts them on a private network with the assistant. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — will now meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.

## v1.7.121-alpha (2026-08-04)

- **Making another node "Trusted" now asks for your node password.** Trust was being handed out by machines rather than by you: any node able to reach yours could join and mark itself Trusted, because the check proved only that the caller owned the key it had just presented — never that you had approved it. Trust also spread on its own, since every peer a Trusted node advertised was added as Trusted too, so one grant quietly propagated across the whole federation. Uninvited joins are now capped at Observer, advertised peers arrive as Observers, and raising anyone to Trusted — whether by generating an invite or by changing the dropdown on a node — requires your password. Lowering trust deliberately does not, because the safe action must never be the inconvenient one. Existing peers are left exactly as they are rather than silently demoted, and each one now records how its trust was granted so you can review them.
- **Nodes you have peered with can be messaged straight away.** Peering was not enough: you also had to be within LoRa radio range of the other node once before chat would work. The node picked how to send a message based on which radio was plugged in, and only one of those paths knew how to reach a peer over the mesh's internet transports — so on a node with a different radio, or no radio at all, messaging a peer you had just federated with simply failed until a radio contact happened to appear. Peered nodes are reachable without radio by definition, so that choice no longer depends on the hardware. Radio is still preferred when the other node is actually in range and the message fits.
- The dashboard no longer flickers a vertical line across its cards. A rendering seam appeared at random while moving the mouse, because the two large cards used a background-blur effect that this system already disables everywhere else on the dashboard — that browser mis-draws it inside the dashboard's animated container, and these two cards had been missed when the workaround was written. Diagnosed from a single screenshot rather than by trying to reproduce it.
- The Lightning screen will actually update from now on. Its image was set to "latest", and the container system will not re-fetch a label it already holds, so nodes kept the same Lightning screen forever no matter how many updates shipped. A separate copy of the same setting used only by brand-new installs also described the screen incorrectly, so fresh installs got a screen that never answered.
- Apps that provide their own screens stop rebuilding themselves in a loop. On this system's own node one of them rebuilt every thirty-five seconds indefinitely, burning processor time and restarting the app each round. The node decided a rebuild was needed by comparing file dates against the image's creation date, but a rebuild that changes nothing reuses the existing image and leaves that date untouched — so the condition that triggered the rebuild was still true afterwards, forever. Nodes taking this update repair themselves the first time they check.
- Groundwork you can see but that does not change access yet: the node can now tell you which of its app ports answer without a login, and every port that is deliberately open — Bitcoin's peer connections for syncing the chain, Lightning's wallet connections, the Electrum wallet protocol — now has to state in writing why it is safe, so the list of exceptions is something you can read rather than something you have to discover. The login gate that will sit in front of the rest is built and proven working end to end on a real node, but it is not yet closing any ports; that arrives with the signed app catalog that tells each app to hand its address over.
- Releases can no longer ship an unsigned update file. Signing was skippable, and when it was skipped the release was still committed and tagged — producing an update that every node correctly refuses to install. It had been caught by hand every cycle; now the release simply stops.
- Known gaps, disclosed rather than buried: the 5x real-node lifecycle gate was not run for this release. App ports other than the deliberate exceptions above are still reachable without a login — the gate reports them, and closing them needs the next signed catalog. Three voice-assistant ports are open without authentication and should not be; the correct fix puts them on a private network with the assistant instead, which needs testing on a node that runs both. Two nodes on the fleet still share SSH host keys (detection shipped, rotation remains a deliberate operator decision).

## v1.7.120-alpha (2026-08-02)

- **Security, and the reason to take this update: two ports on your node handed anyone who could reach them complete control of your money, with no password.** The Lightning app's port answered a plain web request with the LND admin macaroon, the TLS certificate and the node's onion address — everything needed to drain the wallet remotely, and the onion meant an attacker kept that ability even after losing access to your network. The Bitcoin app's port reached Bitcoin Core's control interface using credentials the node itself supplied on the caller's behalf, with a wallet loaded. Anything on your home network, your Tailscale network or the mesh could use either one. Both now require you to be logged in. If your node has been reachable by anyone you do not fully trust, treat the Lightning macaroon and the Bitcoin RPC password as known to them.
- The Bitcoin and Lightning app screens can no longer be published as public Tor addresses automatically. They were one app-id away from being handed a worldwide, permanent address as a silent side effect of being installed — which would have re-opened the hole above to the entire internet. Turning Tor on for them deliberately still works; it just never happens on its own.
- **Fixes shipped inside the program now actually reach apps that your system keeps running.** A container the node had been told to uninstall, but that the system service manager kept alive anyway, was quietly skipped by the part of the node that applies configuration — so it never received updates that shipped with the program. This was found the hard way: the Bitcoin control-interface fix above appeared to be installed and silently was not, while the Lightning half applied correctly, which is the most misleading way for a security fix to fail. Both halves are now proven to land on a real node.
- The Lightning and Bitcoin node screens have been rebuilt to match what umbrelOS offers. Lightning gains Overview, Channels, Activity, Insights, Connect and Settings tabs with a sats/BTC switch; Bitcoin gains Insights, Peers, Connect and Sharing. Along the way: every copy button on those screens silently did nothing (the browser blocks clipboard access inside an embedded page) and now works; the channels link led to a dead page; and Node ID showed a bare key instead of the full address someone can actually connect to.
- Updates to the Bitcoin screen show up without a hard refresh. The page was being cached by the browser, so a freshly updated screen kept rendering the previous one.
- The AI sidebar loads again. It was asking for its program files at an address that pointed at the main app's files, where they do not exist, so it silently loaded nothing.
- The navigation above the bottom bar no longer follows you between screens. Back buttons and the mesh tab bar stayed pinned over every other page once you had visited the screen that owns them. Keeping tabs loaded in the background — the change that made switching between them instant — means leaving a screen hides it rather than destroying it, and this floating navigation sits outside the screen it belongs to, so it was never being hidden with it. It is now tied to whether its own screen is on display. The speed is unchanged: the screens are still kept loaded, so returning to one is still instant.
- Wallet: Lightning actions are now offered based on whether you actually have a usable channel rather than just a running node, sending is gated the same way, and an invoice you cannot yet receive offers to install a Lightning node instead of simply failing.
- Onboarding and viewing fixes: the "I have written down my recovery words" tickbox is findable on short screens, paid pictures and videos open in the app's own viewer with a visible loading state instead of a blank browser tab, picture-in-picture survives changing tabs, and the FIPS/Tor labels on peer cards stay put instead of wrapping into the card below.
- Key-material hardening across the node: a node that is already set up refuses to have its identity replaced by an unauthenticated request; first-boot secret generation now fails loudly instead of silently continuing with shared keys; the node proves its TLS certificate and key are actually a matching pair; the Bitcoin Core wallet path that kept a second copy of your spending key outside the encrypted store has been removed; and every place the node generates a key, token or nonce now names its source of randomness explicitly, enforced at build time.
- Federation and mesh: a rotated gateway credential now reaches the already-running container instead of leaving the old one in place, sync failures are surfaced to you instead of being swallowed, and nodes can share their Lightning connection details with a chosen peer over the mesh — the groundwork for opening channels with nodes you already talk to.
- Known gaps, disclosed rather than buried: the 5x real-node lifecycle gate was not run for this release. Two nodes on the fleet still share SSH host keys with each other (detection shipped, rotation is a deliberate operator decision and has not been performed). Bitcoin Core can now reach Tor from its container, but is not yet routed through it — the network mode is becoming a setting you choose, and until then Core's peers remain on the clear internet.

## v1.7.119-alpha (2026-07-31)

- Wallet payments now work on nodes whose channels are private/unannounced. Every invoice-creation call site — the wallet's own Receive flow, and the seller-side paid-content/peer-files flow — only ever sent LND the amount and memo, so LND defaulted private to false and returned invoices with no route hints. Any node whose only usable channel is private or unannounced (the common shape for a channel someone opened to you) was silently unpayable through the wallet, and unpayable through paid file/content sales too. Both call sites now set LND's private flag correctly; this was broken in the field and is the main reason for this release.
- Tor and the mesh's Tor fallback are reliable again. The node's background "doctor" health-checker was fighting Tor over the permission bits on its own hidden-service directory: it compared the directory's mode against the literal string "700", but Tor's own setgid hidden-service mode is 2700 — a value the doctor's check never recognized as correct. Every ~5 minutes it "corrected" the mode back to 700 and restarted Tor to apply it, and Tor immediately reasserted 2700 — a permanent restart loop that meant Tor could never hold onto its consensus/HSDir cache long enough to be useful, breaking the mesh's Tor fallback path entirely. The check now compares only the owner/group/other bits that actually matter (both 700 and 2700 pass; genuinely wrong modes like 750 or 2755 are still corrected and restart Tor), plus a 30-minute restart backoff so no future condition can reproduce the storm.
- Wallet balances and your node's own FIPS identity key (npub) are no longer written to the browser's sessionStorage — caught by an audit of the page-caching work below. Every cache call site in the app now makes an explicit, reviewed decision about whether its data is allowed to persist across a reload, and a one-time migration purges any legacy, unaudited snapshot left behind by an older build.
- Server, Home, Mesh, Chat/AI chat, and the secondary screens (app details, marketplace, cloud, federation, monitoring, router/OpenWrt) now load instantly from cache when you revisit them and refresh quietly in the background, instead of blanking and re-fetching everything on every tab switch — this closes out the page-performance work started back in v1.7.116/117.
- App updates (including this one) now apply automatically in the background instead of waiting on a tap-to-update prompt, matching how kiosk/TV installs already behaved — the reload still waits for any in-progress splash/dashboard animation to finish first, so it won't land mid-motion. This was a direct, explicit decision made with the mid-payment-reload risk spelled out in advance; reverting to a confirmation prompt for beta is a one-line change if wanted later.
- Known gap, disclosed rather than buried: the project's 5x production lifecycle gate (install/UI/stop/start/restart/reinstall/reboot-survive/archipelago-restart-survive/uninstall, run on a real node — CLAUDE.md's own definition of done before a release tag) was NOT run for this release, because its target node was unreachable and running it here would have required rebooting a shared, live build machine out from under other active work. This release's own automated gates (release-gate harness, strict catalog-drift check, the full cargo test suite, a mount-level ISO smoke test, and a headless QEMU boot test) all still ran and passed — this is specifically about the separate 5x real-node lifecycle gate, which is still outstanding and should be run as soon as the node is reachable again.

## v1.7.118-alpha (2026-07-29)

- Fixes mesh radios dropping off on nodes that took the v1.7.117 update. Updates only ever replaced the main program, never the packaged radio helpers — so updated nodes were left running an older radio daemon that didn't understand a new option and quietly gave up, showing "device not connected" with a Connect button that did nothing. The node now checks what its radio daemon supports before using new options, and updates finally carry the radio helpers themselves, so every node gets current radio support with the update instead of only from a fresh install.
- The in-app "Flash LoRa" flow works on updated nodes. The RNode flashing tool was only ever included on freshly installed nodes; everywhere else flashing failed with a cryptic "No such file or directory". The tool now ships with updates and is included on new install images, and if it's somehow still missing the error says exactly what to do instead.
- Message notification badges finally remember what you've read. Unread counts were only kept in memory, so every visit re-counted old messages as new — including a phantom badge for chats with nothing new in them. Read-state is now saved on the device, opening a chat marks all its linked conversations read, and history no longer re-badges after a reload.
- Every mesh message now has a visible "⋯" button that opens the route view: watch the path your message took animate — sender and receiver appear, a pulse travels the link, and each relay hop lights up in order — with signal quality for radio links and delivery status. (Tapping the transport pill still works too.)

## v1.7.117-alpha (2026-07-29)

- Flash your LoRa radio from inside the app. The Mesh page now has a "Flash LoRa" button that opens a guided flow: pick the firmware family (MeshCore, Meshtastic, or Reticulum RNode) and your board, and the node downloads the latest release and flashes it with live progress — no external flasher website, no cables to a computer. The same flow appears when a freshly plugged-in radio is detected, and a long list of flashing pitfalls was fixed along the way: radios no longer boot-loop after a flash, failures show the real error instead of silently bouncing back, wedged flash jobs can't get stuck forever, and board auto-detection no longer misidentifies Heltec boards.
- Every Archipelago node now acts as a Reticulum relay. Nodes forward mesh traffic and re-broadcast peer announcements, so two radios that can't hear each other directly can still discover and message each other through any Archipelago node in between — your nodes become infrastructure for the whole neighbourhood mesh, including non-Archipelago apps like Sideband.
- Reticulum (RNode) radios are now first-class mesh citizens. Radios are reliably detected on node startup (a boot-timing race used to leave them unclaimed), settings changes apply live without a restart, your node's name propagates over the Reticulum network so other apps like Sideband see it properly, and a crashed Reticulum daemon is detected and restarted automatically. Photo and file attachments sent over Reticulum now actually arrive — four separate delivery bugs were found and fixed, verified end-to-end over real radio hardware.
- Messages to contacts that exist on both the internet mesh and a LoRa radio now prefer the radio when it's live, and attachments follow the same path — so co-located nodes talk over the air even when the internet path exists.
- Mesh chat polish: each message in the image viewer shows which transport carried it, a new hop-route view shows the path a message took, reactions moved into a tidy dropdown, and read-tracking now reflects what you've actually seen. The Refresh and Broadcast buttons give real feedback, and the radio-setup modal shows honest probe progress instead of freezing.
- The wallet transactions list works properly on phones now: it scrolls (it silently couldn't on touch screens before), and the All / On-chain / Lightning / Ecash filter tabs stay pinned at the top with a subtle blur while the list scrolls underneath.
- Backend services no longer masquerade as launchable apps. Anything without a real web interface — databases, APIs, background workers, including stacks you deploy by hand for testing — now files under Services with no Launch button. Apps declare their interface in their manifest; for everything else the node checks the port itself to see whether a browser page actually lives there.
- Lightning payments that take a while (slow multi-hop routes) are no longer reported as failed while they're still in flight. The wallet now waits properly, shows an honest "pending" state, and reports the true final outcome.
- Server pages feel instant: Server, Federation, Lightning channels, Monitoring, wallet, Cloud, and Credentials screens now render immediately from a shared cache and refresh live in the background (including push updates over the node's websocket), instead of blanking while every panel refetches.
- The node stays responsive under heavy load: the connection handler now sheds excess load instead of stalling everything behind it, and companion-app probes no longer trigger container builds during routine checks.
- FIPS mesh uptime hardening continues: the node's peer port is opened explicitly everywhere, LAN anchors use the right port, direct peering between co-located nodes works again, dials fail fast instead of hanging, and a connectivity watcher re-applies anchors immediately when the network comes back.
- FIPS startup is more reliable on nodes that have the packaged `fips.service` instead of Archipelago's `archipelago-fips.service`. Startup self-heal, onboarding, dashboard Start, and reconnect now use the systemd unit the node actually has, so FIPS no longer looks like it needs to be installed when it only needs to be started.
- App screens over the FIPS mesh now bind their relay only to the node's FIPS address instead of reserving the same host ports Podman needs. This keeps apps such as FileBrowser and Botfights from restart-looping because the backend was already holding their published ports.
- Companion app 0.5.25: a redesigned settings hub (three-finger tap opens it over the dashboard), seamless transport handoff with FIPS mesh settings, the wallet scanner reads dense invoice QR codes, app webviews clear the phone status bar with an HTTPS toggle on add/edit, and off-LAN loads fall back to the mesh URL instead of a dead LAN address.
- Public-source preparation now includes a Nostr Git hosting plan using `ngit`, NIP-34, and GRASP: anyone can clone, fork, review, and propose changes from their Archipelago node, while canonical merge authority stays with a small signed maintainer set in the style of Bitcoin Core.

## v1.7.116-alpha (2026-07-27)

- Nodes no longer get stuck on "server starting up" after an update or reboot. On a node running many apps, the backend used to spend minutes recovering containers before it told the system it was ready, and anything that touched it during that window could leave it down for good. It now reports ready immediately and recovers in the background, and it always restarts itself if it ever does go down.
- Installing apps no longer crashes the node. A change that made app screens reachable over the mesh was accidentally holding onto every app's network port in advance — so installing an app like Grafana, Photoprism, Uptime Kuma, or Jellyfin collided with it and the port-cleanup step took the whole backend down, rolling the install back. Installs are now clean and the backend can never be caught by that cleanup.
- Rolls up everything from v1.7.115: app screens and the dashboard load over the mesh out of the box (firewall openings shipped automatically, IPv6 support end to end), and nodes rejoin the mesh in seconds after their rendezvous point restarts.

## v1.7.115-alpha (2026-07-26)

- The companion app can reach your node's screen from anywhere again. The recent security hardening locked down the node's mesh interface so tightly that the dashboard itself was blocked — the phone would pair and connect, then sit on a blank screen. The node now explicitly opens its own web interface (and only that) through the mesh firewall on every install and upgrade, so the phone's view of your node works out of the box, on any network, and can't silently break in a future update.
- The node's web interface also answers on IPv6 everywhere it answers on IPv4 — the mesh runs entirely on IPv6, and one v4-only listener was enough to make a working connection show nothing.
- Nodes now come back onto the mesh in seconds instead of minutes after their rendezvous anchor restarts: the fast-reconnect tuning proven on the phone this week is now baked into every node's mesh configuration, and it survives upgrades.

## v1.7.114-alpha (2026-07-26)

- Plugging in a mesh radio no longer traps it in an endless reboot loop. The device detector itself was causing it: every scan pulsed the radio's reset line, the same board was probed twice under two names, and retries came so fast the radio never finished booting before the next reset hit. Detection now gives the board real time to boot, probes it once, backs off properly between attempts, and no longer fights the "device detected" popup for the port. Radios that could never connect now come up within a minute of being plugged in.
- The Lightning channels screen now has All / Active / Pending / Closed tabs. Pending gathers everything in motion (opening, closing, force-closing — each with its own status dot and a link to the closing transaction), and Closed is a real history: how each channel ended, what settled back to you, and the closing transaction for each.
- Sending bitcoin on-chain now puts you in charge of the network fee: pick Fast, Standard, or Slow (Standard is the default), or set your own target blocks or sats-per-vByte. The confirmation step shows the estimated fee for your chosen speed before any money moves.
- Type on-chain amounts in whichever unit you think in — a sats/BTC switch on the amount field converts as you type.
- Back up your seed by scanning it. Every recovery-phrase screen (onboarding, Settings, and the Lightning wallet seed) now has Words and QR code tabs — words always shown first. The QR for your node's recovery phrase uses the SeedQR standard, so hardware wallets like Passport Prime, SeedSigner, and Keystone can import it with a single scan (a plain-text option remains for wallets that read the phrase as text). The Lightning seed's QR is plain text with an honest note: it's an LND-format seed that restores into Lightning wallets like Zeus or Blixt, not into hardware wallets.

## v1.7.113-alpha (2026-07-25)

- Fixed a money bug in Cashu ecash sends: the token you handed a recipient could carry your own change proofs along with it, letting the same sats be credited twice. Change now stays in your wallet — only the amount you meant to send leaves it.
- Closing a Lightning channel is no longer a leap of faith. The close used to hang (or time out with an error) even though it had actually gone through; it now comes back within seconds with the closing transaction ID. Channels mid-close appear in the channel list as Closing or Force-closing with their transaction attached, and a new closed-channels history keeps past closes visible instead of letting them vanish from the list.
- The wallet card now leads with your total bitcoin across everything, and the on-chain balance gets its own chain icon so the rows read at a glance.
- The companion phone app (0.5.15) connects dramatically faster away from home: a cold connect over 5G dropped from 40+ seconds to about 5. First connects no longer stall on unreachable mesh dial hints, fresh joins fail fast and retry instead of waiting out long timeouts, and the phone re-announces itself the moment the network around it changes. The node side's mesh-join handling was hardened to match.

## v1.7.112-alpha (2026-07-23)

- Sound works on TVs out of the box. Fresh installs were missing the audio system entirely, and even when present a boot-time race left HDMI silent until the cable was unplugged and replugged. Both are fixed: installer images now ship the full audio stack, and a small background helper detects the silent-HDMI state and heals it automatically.
- Plug in a game controller and drive the whole TV interface with it — navigation, menus, and media playback all respond to the gamepad, and dialogs that pop up are controller-navigable too.
- The companion phone app took a huge leap (0.5.9). Your node and its apps now work from anywhere — on 5G or any internet connection, the phone reaches the node over the encrypted mesh with zero port forwarding or VPN setup. Startup away from home is instant, apps on your node open inside the app, the phone's native camera handles QR scanning, and a branded full-screen loader shows while the mesh connects.
- Mesh Party: two phones scan each other's QR and instantly get a direct encrypted chat and app sharing between them — plus a "Share this app" QR that anyone can scan with a normal camera to install the companion app.
- Pairing a second phone no longer silently logs out the first. Every device now keeps its own named access credential, ending the mystery reconnects when a household paired more than one phone.
- The companion pairing QR is scannable again (it had grown too dense for phone cameras) and now identifies your node by its identity key, so the app recognizes your node even after it moves or gets a new address.
- Every app your node serves on your home network is now also reachable over the mesh — remote access covers the apps themselves, not just the dashboard.
- Selling files: you now choose which payment methods you accept (Lightning, ecash, …) and buyers are only offered those — enforced by the node itself, not just the buttons. Paying twice for the same file is impossible now, purchases file themselves into a new Paid Files tab, purchased music always plays in the bottom-bar player, and videos get picture-in-picture.
- Sending Lightning is invoice-first: paste or scan an invoice and the amount fills in and locks by itself. An expired invoice now tells you plainly to ask for a fresh one instead of failing cryptically, and payment errors always reach your screen.
- Sending to a pasted address gets a confirmation step showing exactly what will happen before any money moves, and buying ecash is an explicit two-step — no more accidental purchases.
- Apps keep running when you change how they're displayed. Switching an app between windowed and fullscreen used to reload it from scratch (stopping any playing media); the app now stays live through the switch, and each app remembers its own preferred display mode.
- A watchdog notices when the Lightning (LND) node wedges and revives it before you do; Fedi ecash gets its own send option with scannable token QR codes.
- Fedimint's Lightning gateway and guardian now follow whichever bitcoin version is actually running instead of pointing at a stale address — switching bitcoin versions no longer strands them.
- If your router starts handing out different addresses, the Pine voice speaker re-links itself automatically instead of staying silent until someone re-configures it.
- Polish: mesh radios never show garbled device names anymore, the TV kiosk uses slim overlay scrollbars instead of fat grey bars, and the AI chat's background artwork shows through again.
- Your mesh messages now survive restarts. Chat history — channels and DMs alike — used to live only in memory, so a reboot or update wiped every conversation; worse, other nodes silently discarded the first messages you sent after a reboot. Everything is now saved on the node and restored on startup, and post-reboot messages deliver reliably.
- Plug in any LoRa radio and the node walks you through it. A setup window appears every time a radio is connected, shows what firmware is already on it (MeshCore, Meshtastic, or Reticulum RNode — with its current name, region, and channels where available), and offers two honest choices: "Set Up with Archipelago Settings" (a preview screen shows exactly what will be written before anything touches the radio) or "Keep As Is" (the radio is used untouched, and you can hot-swap radios freely). Swapping sticks mid-session now just works — including Reticulum RNodes, which fresh installer images now support out of the box.
- Incoming bitcoin appears in your wallet within seconds of being sent — balance and the yellow "unconfirmed" entry update live, no refresh, no waiting for the next poll.
- The speaker now announces the very first mesh message a node ever receives, and DMs announce just like channel messages (a safety guard against announcement storms was quietly swallowing them). Announcements also react about twice as fast.
- Opening a Lightning channel right after the node starts no longer fails with a scary red error. The node quietly retries while Lightning finishes waking up, and if it's still not ready you get a calm "still finishing its startup — try again shortly" notice instead.
- Viewing a transaction works on every node now, including small ones. Nodes with pruned bitcoin storage can't run the Mempool explorer app; transaction links now open your choice of external explorer instead (tx1138.com by default) — after a clear one-time warning that a third-party server will see which transaction you looked up. Set your preferred explorer in Wallet Settings → the new On-chain tab.
- Voice commands respond noticeably faster: speech recognition now transcribes in roughly half the time, with identical accuracy on short commands.
- Scanning a Lightning invoice with your phone's camera is far more reliable — dense invoice QR codes that the photo scanner missed are now read by the phone's native barcode engine.
- The companion app's pairing QR always contains an address your phone can actually reach. If you manage your node over a VPN (Tailscale), the QR used to embed the VPN address, and pairing silently failed; it now advertises the node's home-network address.
- Peer requests sent from Nostr discovery now actually arrive: your node checks for incoming requests every five minutes by itself (previously they sat unseen until someone manually pressed "Poll"), requests publish to all your configured relays instead of two hardcoded ones, and a failed send tells you instead of pretending it worked.
- The Connected Nodes list refreshes instantly. It previously froze for up to 30 seconds per offline peer while checking who's reachable, one peer at a time; the checks now run all at once in the background while the list shows immediately.
- Apps opened from inside a window (like a transaction from the wallet) now animate smoothly on top instead of loading invisibly underneath.
- Settings-style windows keep their tabs pinned at the top and their buttons pinned at the bottom; only the middle scrolls. The wallet's tabs are now Channels / Cashu / Fedi / Ark / On-chain so all five fit.
- On the TV screen, menus no longer flash open and instantly close. And the interface never follows your computer's light/dark preference anymore — dropdowns and other native controls stay dark on every device.
- Error messages tell you what's actually wrong: "Insufficient balance: need 80 sats, have 0 sats" now reaches your screen instead of "Operation failed. Check server logs."
- Installing Mempool no longer refuses to start while ElectrumX is mid-resync (it connects by itself once ElectrumX is ready), and installs no longer fail just because the system was momentarily busy.
- Much quieter logs: the node no longer tries to start containers that are already running (hundreds of harmless-but-alarming errors per day), and a node that's offline stops hammering unreachable servers every 30 seconds with rebuild attempts.
- Phones pairing with the companion app connect over the node's embedded mesh for remote access, with instant QR pairing and per-device access tokens (contributed alongside this release).

## v1.7.111-alpha (2026-07-22)

- Ask your node anything, out loud. Install Pine (the voice assistant app) alongside Home Assistant and everything wires itself automatically: speech recognition, the speaking voice, and a Claude-powered brain. Questions about your node — "what's the block height?", "how many peers am I connected to?", "is bitcoin synced?", "what's my Lightning balance?" — are answered instantly from the node itself without costing anything; anything else goes to Claude for a real conversation. New mesh radio messages are read out on your speaker as they arrive.
- Pine now ships a wake-word listener, so a paired speaker can sit on standby and activate when it hears its wake word instead of needing a button press. (A custom "Yo Archy" wake word is in the works.)
- Pine's launcher page shows your node's live status at a glance: software version, uptime, bitcoin sync progress, and mesh peers.
- Fixed: installing Pine could send Home Assistant into a crash loop on startup (a record the installer wrote was missing a timestamp field Home Assistant requires). Two noisy warnings that repeated in Home Assistant's log every half minute are silenced too.
- The companion phone app opens every app in its fast built-in browser view again, with native back/forward/reload controls, instead of embedding some apps inside the page where they scroll and render worse. This had quietly regressed.
- Turning on federation discovery now shows you exactly what you're about to sign: a panel explains the announcement before your key signs it, you can review the signing details any time from the discoverability strip, and the panel fits and scrolls properly on small phones.
- Fixed a bug on nodes using the newer app-management engine where Bitcoin's access credentials were written out incorrectly (a placeholder leaked through as the literal text "/bin/bash"), which broke the node's Bitcoin status display, Lightning's connection to the chain, and any app that reads Bitcoin data.
- Bitcoin's access credentials also moved out of the process command line into a protected file, so they're no longer visible to other software on the node.
- Desktop app windows have one-click buttons to switch between side panel, overlay, and fullscreen viewing.
- On the phone home screen, the wallet card moved up to sit right under My Apps.
- Home Assistant updated to 2026.7.3, which keeps voice satellites (like Pine's speaker) connected reliably.

## v1.7.110-alpha (2026-07-21)

- Pay by pointing your camera: the wallet has a new Scan button (on the wallet card and inside both the Send and Receive windows) that reads any payment QR code — Lightning invoices, Bitcoin addresses, Cashu tokens, and Fedimint invites — and takes you straight to the right send or redeem screen with everything filled in. It also understands the animated, multi-part QR codes some wallets show for long payloads. If your browser can't open a live camera preview (common when reaching the node over plain http), a "Take photo of QR" button snaps a picture with your phone's camera and reads the code from the photo instead.
- The TV screen got a complete overhaul. A deep bug made the display freeze on the intro artwork on 4K TVs — that's fixed, and along the way: the interface now picks a comfortable, sharp size for big screens (a 4K TV gets a full desktop layout at double sharpness), the artwork behind every page shows again instead of a black void, switching between tabs animates smoothly, the built-in AI assistant stays in its dark theme, and the Cashu and Ark wallet icons no longer render as empty squares.
- You can now choose how big the interface renders on your node's attached screen: Settings → Display offers Auto (recommended), Large UI, Balanced, and Native — changing it applies immediately.
- The companion phone app can steer the TV again. Remote input from the phone was being silently ignored on kiosk displays; the remote-control relay now runs there like everywhere else.
- The companion app is also ready to grant its built-in browser camera access, so the wallet scanner can work inside the app (ships with the next companion app build).
- Zero-amount Lightning invoices can now be paid: the wallet asks you for the amount and sends it along, instead of failing on invoices that leave the amount up to the payer.
- The Lightning setup guidance now reads the same everywhere: "Open a channel with Zeus Olympus node and start sending and receiving Lightning payments. Minimum 150,000 · maximum 1,500,000 on-chain sats required."
- Installer images now bundle a color-emoji font, so emoji anywhere in the interface render properly on the TV screen.

## v1.7.109-alpha (2026-07-21)

- Meet Pine, your node's voice assistant: a new app in the App Store that gives your node ears and a voice — speech-to-text and text-to-speech engines that run entirely on your own hardware, ready to wire into Home Assistant for private, offline voice control. Install it like any other app; nothing you say leaves your node.
- Your node can now program its MeshCore radio's RF settings — frequency, bandwidth, spreading factor, and coding rate — from Mesh → Device settings. Radios that were flashed with mismatched settings could hear that other radios exist but never decode their messages, and until now the only fix was a separate phone app. Set the values once and the node programs the radio automatically (it restarts once to apply); every radio on your mesh must use the same values to talk to each other.
- The Device settings panel is tidier: values you can edit (name, region, channel) are no longer also shown as separate read-only rows.

## v1.7.108-alpha (2026-07-20)

- Your node connects to the private mesh far more reliably. Nodes rely on a public rendezvous point to find each other, and the only one available was unreachable from many home and office networks — leaving some nodes unable to join the mesh at all. There is now a second, always-reachable rendezvous point, and your node tries every one it knows, so it joins the mesh in seconds instead of being stranded.
- Wi-Fi setup now heals itself on older nodes. Some nodes set up before a mid-year fix couldn't connect to a Wi-Fi network from the screen — it failed with a permissions error — because the piece that lets the node manage networking on your behalf was missing. Nodes now put that piece in place automatically on startup, so "scan, pick a network, type the password, connect" works without reinstalling.
- Your node rejoins the mesh within seconds after an update. Applying an update briefly restarts the mesh service, and previously a node could sit disconnected from other nodes for up to five minutes before it retried.
- The TV screen now fits your television. On a large or 4K TV the interface rendered tiny with no way to zoom on a keyboard-less screen; it now sizes itself to a comfortable, readable scale automatically (and small laptop panels are left unchanged).
- More TV-screen polish: the built-in assistant shows its dark theme instead of bright white panels, the on-screen hint for switching between the kiosk and a terminal now points at the right keys, the welcome logo no longer occasionally renders as garbled characters, and an accidental tap of the power button no longer shuts the node down — hold it to power off on purpose.
- Behind the scenes: fixed the installer image build so it no longer stops on a component that was removed from the product, and so it correctly includes the private relay it was meant to bundle.

## v1.7.107-alpha (2026-07-20)

- Wi-Fi setup now heals itself on older nodes. Some nodes set up before a mid-year fix couldn't connect to a Wi-Fi network from the screen — it failed with a permissions error — because the piece that lets the node manage networking on your behalf was missing. Nodes now put that piece in place automatically on startup, so "scan, pick a network, type the password, connect" works without reinstalling.
- Your node rejoins the mesh faster after an update. Applying this update briefly restarts the mesh service, and previously a node could sit disconnected from other nodes for up to five minutes before it retried. It now notices the restart and reconnects within seconds.
- Behind the scenes: fixed the installer image build so it no longer stops on a component that was removed from the product, and so it correctly includes the private relay it was meant to bundle — two separate faults that had been failing the build.

## v1.7.106-alpha (2026-07-20)

- Nodes on the same network now find each other directly. Your node announces itself on your local network and connects straight to other Archipelago nodes nearby, instead of every connection having to be introduced by a public rendezvous server out on the internet. Peers in the same home or office stay connected to each other even when that server is unreachable, and they reach each other faster.
- On a phone, the peer files screen tells you how you're connected again. The badge showing whether a peer's files are arriving over the fast mesh or over Tor was only visible on desktop — on narrow screens it disappeared entirely. It now appears next to the peer name on mobile too.
- Your node's mesh settings can no longer be written in a way that breaks the mesh. The configuration file used to be assembled as free-form text, where one wrong setting would stop the mesh service from starting and quietly drop your node off the network. It's now generated from a checked description of the file, with tests that verify the exact output.
- When your node has trouble reaching another node, the logs now record the real reason instead of a generic summary. A failure to open a peer's files previously logged only "Failed to connect to peer" and threw away the actual cause, which made these problems very hard to diagnose. Nothing changes on screen, and no internal detail is exposed.
- Behind the scenes: the installer image now builds its mesh component at a fixed, known version instead of whatever upstream had published that day, so two images built from the same source are identical.

## v1.7.105-alpha (2026-07-20)

- Fixed a failure loop where a node that lost power or was moved could get stuck on a blank "can't reach your node" screen forever: startup recovery no longer spends minutes retrying containers that no longer exist, and a genuinely large recovery is no longer cut off half-way and forced to start over. The node now reaches its login screen even after the messiest shutdown.
- Phone tunnel setup (WireGuard) is now dependable: the QR screen automatically retries while a fresh install is still settling instead of dead-ending at "failed to fetch", and if your node has moved to a different network the QR and downloadable config now carry the node's current address instead of the old one.
- Fixed the white screen some laptop displays showed right after the intro on v1.7.104.
- The companion phone app no longer suggests installing the companion app from inside itself.
- The Tor page now lists onion addresses only for apps you actually have installed — fresh installs no longer come with six pre-made addresses for apps that were never set up.
- Running `archipelago --version` or `--help` on the command line now prints and exits instead of silently starting a second copy of the node, which could briefly disrupt running apps.
- Behind the scenes: installer image builds now stop loudly if VPN components are missing instead of producing a broken image, and a background file-permission sweep runs far less often, reducing disk churn on busy nodes.

## v1.7.104-alpha (2026-07-19)

- Software updates are now much safer to receive: the node will never install an update that isn't completely downloaded and verified byte-for-byte, closing a rare bug where an interrupted or cancelled download could leave a node unable to start.
- If a freshly installed update does fail to start, the node now notices and automatically restores the previous working version by itself — no manual rescue needed.
- The Electrum server now works with whichever Bitcoin you run: it finds Bitcoin Knots or Bitcoin Core automatically instead of assuming Knots.
- While the Electrum server is first building its index, its waiting screen now shows the ElectrumX app icon and live progress.

## v1.7.103-alpha (2026-07-18)

- Connecting from the phone app no longer replays the intro cinematic on a loop: signing in after scanning the pairing QR could accidentally trigger "Replay Intro" instead of logging you in. The companion app now lands you straight on your dashboard, and the Android app waits for the login screen to be ready before it types your password.
- Pressing Enter in any password box now does what you expect — it signs you in or moves to the next field, and can no longer "click" a nearby button by mistake when using a controller or the companion app.
- The public demo no longer interrupts you with an "Update Available" popup that reset the site back to the intro — demo visitors simply get the newest version on their next visit.

## v1.7.102-alpha (2026-07-17)

- The password you choose during setup is now truly your node's password: it also becomes the system login for console and SSH access, instead of leaving the factory default in place. If you ever renamed your node and the TV screen went black on the next boot, that's fixed too — renaming no longer breaks the kiosk display.
- Setting up Lightning is now a guided journey: a fund-your-wallet step that shows a live countdown while Bitcoin syncs, suggested channels you can open straight into the Zeus mobile wallet with one tap, and a "finish setup" prompt that walks you to the end — goals now complete when you've actually done the steps, not just when apps happen to be running.
- Pair your phone by pointing it at the screen: the companion app now connects by scanning a QR code — scan, and it fills in your node's address and logs you in. The App Store has a banner to grab the Android app, and the pairing flow can now also set up secure remote access so your phone reaches home from anywhere.
- First installs are far more dependable: app downloads that stall now retry instead of hanging forever (the old "first install fails, the second works" pattern), big multi-part apps show their real download progress instead of sitting at "Preparing", Lightning no longer fails its first install over temporary hiccups, and a brand-new node now comes up with its core apps — file cloud and ecash wallet — even with no internet connection.
- The installer image is about 160MB smaller and gets to a working screen faster, because the apps bundled for offline setup are now compressed.
- The first-run experience keeps its magic: the typing intro is back on fresh installs and can no longer be cut short by a mid-play refresh — updates now politely wait for the cinematic to finish — and dark backgrounds stay dark instead of flashing black or white.
- Your backups now include your secrets — including the key that protects your Lightning wallet's recovery seed — and there's a Download button to take a copy off the node; the seed-backup reminder now actually opens the backup flow when you tap it.
- Networking Profits grew into a full dashboard, network cards keep their action buttons in reach on every screen size, "Connect to Mesh" goes to the right page instead of a dead end, and the identity pages got a round of mobile polish.
- Behind the scenes: apps that report their own health are no longer second-guessed by a port probe (fewer false "restarting" states), and pressing arrow keys or a gamepad is once again the only thing that shows the controller focus ring.

## v1.7.101-alpha (2026-07-15)

- The wallet speaks Ark: a new Ark tab shows your Ark balance and history, you can send and receive over the Ark protocol, pay Lightning invoices from your Ark balance, and Ark payments appear in the transactions view with their own filter chip.
- Every app you install now automatically gets its own private .onion address — your apps are reachable over Tor a few seconds after install, with no manual "Add Service" step.
- "Add Service" in the Tor panel now works for every app, not just a fixed list — the node reads the app's actual web port, so apps like Gitea, Jellyfin, Nextcloud, and Uptime Kuma no longer fail with "see server logs".
- Renaming your node now genuinely renames it everywhere: the machine's hostname, its .local network name (re-announced immediately), the local hosts file, and the HTTPS certificate all follow — so http and https links using your node's name keep working right after a rename.
- The node no longer mistakes a VPN tunnel for its own address. On fresh installs with NetBird, apps could launch on an internal 10.x address instead of your LAN IP; the node now reads its address from the actual network route, fixing app launch links, generated app configs, and VPN setup.
- Your cloud got a real layout: Apps-style tabs with categories for Folders, My Files, and Peer Files, readable file rows, a search that also finds files shared by your federated peer nodes — and music now opens in the bottom-bar player instead of a broken preview window.
- The first-login experience flows again: the dashboard entrance animation is back — and you can actually hear it now (its sound was silently swallowed before, including on replays) — "Replay Intro" in Settings actually replays it, opening a direct link to an inner page no longer detours through the splash screen, the login screen keeps the intro video until your first login (switching to rotating backgrounds after), and the intro video streams three times lighter so it starts instantly.
- Changing DNS settings no longer blanks the page, and the DNS and WiFi dialogs now cover the whole app instead of only the right panel.
- The public demo is richer and truer: the intro plays on every fresh visit, Ark wallet flows, working DNS and Tor service management, and a library of peer content with previews that never break.
- Assorted fixes: failed installs clean up after themselves properly, and the transactions view fits mobile screens (capped at 60% of the visible viewport).

## v1.7.100-alpha (2026-07-14)

- Bitcoin now supports multiple versions of both Bitcoin Core and Bitcoin Knots: install the version you want, switch between them, pin a version, or let it auto-update — and switching is designed to be safe, with no surprise resyncs.
- Lightning grew up: your LND wallet's recovery seed is captured at setup and kept as an encrypted backup you can reveal from Settings, there's a new Channels tab with a fee control when opening channels, and on-chain and Lightning balances now show side by side.
- Installing Lightning (and other Bitcoin-dependent apps) on a fresh node no longer fails repeatedly — the node now waits until Bitcoin is genuinely ready to answer before starting them, and Bitcoin sizes its storage to your actual disk and its memory cache to your RAM, so small machines stop swapping and stalling.
- The wallet understands more money: Cashu v4 tokens are supported, you can pay for a peer's files from either your Cashu or Fedimint ecash, and the Transactions view now shows your Lightning, Cashu, and Fedimint activity together — with a payment confirmation screen and an automatic refund if a purchase fails.
- Mesh radios got a major upgrade: Meshtastic direct messages are now true end-to-end-encrypted radio messages that interoperate with off-the-shelf Meshtastic phone apps, your radio's region and a shared channel are provisioned automatically, and a new setup window appears when a radio is plugged in — with board pictures, full radio settings, and signal-strength indicators.
- Reticulum joins as a third mesh radio protocol with RNode LoRa hardware support, including sending images and voice messages over the radio — and every chat message now carries a small pill showing how it travelled (Mesh, FIPS, or Tor).
- Your node can manage an OpenWrt router: set up its internet uplink from the UI with a Wi-Fi network scan, turn it into a TollGate pay-for-Wi-Fi hotspot with a real captive portal, and sweep the router's earnings into your node's wallet. The gateway's status appears on the Home screen's Network tile.
- Peering is now trust-aware: "Invite a Peer" grants view-only Observer access while "Link Your Nodes" grants Trusted access, incoming requests ask for your confirmation with an optional message, Node Visibility is a single clear switch plus a list of discoverable nodes you can peer with, and the Fleet view shows your trusted nodes' health.
- Updates and apps are verified end-to-end: release updates are cryptographically signed and checked against a key baked into your node, app definitions arrive via the signed catalog, and container images are checked against trusted sources before anything installs or runs.
- Dozens of reliability fixes: failed installs no longer leave phantom app cards, uninstalling can't hang forever, apps you stopped stay stopped, crashed apps heal themselves (even "running" containers whose process actually died), the login page no longer refresh-loops, and the mobile layout fits real phone screens instead of hiding the last row behind the browser bar.

### Also in this release

- Ask your node things over the radio: send "!archy" for node status with no AI involved, or "!ai <your question>" in a direct message for an AI answer that comes back on the same path it arrived — with a model dropdown (Haiku, Sonnet, or Opus) and an "always allow" list in the Mesh AI Assistant panel.
- The off-grid mesh radio no longer posts cryptic identity codes ("ARCHY:") to the shared public channel every minute.
- Mesh contacts take care of themselves: new radios you hear are added automatically, "Clear All" really removes contacts (they return when in range), each contact shows a reachability dot, and the Peers list has a search box.
- You can message standard meshcore phone apps and they can message you — readable text both ways, private replies instead of public-channel broadcasts.
- Federated Archipelago nodes now appear on the Mesh Map.
- Apps open as an overlay on top of whatever page you're on, in every display mode, instead of yanking you to a different screen; the Services tab groups apps by category with proper icons.
- BTCPay Server keeps its plugins across restarts, connects to your node's own LND out of the box, and its invoices stay payable over private Lightning channels.
- Fedimint federations show up in Wallet Settings again (the client app's configuration error is fixed), and Wallet Settings has tabbed sections for Cashu and Fedimint.
- The phone companion app can upload and download files, edit saved server entries, opens non-embeddable apps in an in-app browser, and got a proper round launcher icon.
- Six placeholder "apps" that were just web bookmarks (484.kitchen, arch-presentation, call-the-operator, nwnn, syntropy-institute, t-zero) are gone from the store.
- The Bitcoin dashboard works fully offline (no more loading its styling from the internet), Gitea opens on the right port, and mempool, strfry, and Electrum stopped their restart/health-check loops.
- Kiosk displays: HDMI audio no longer stutters, and a bad display-clone state no longer sticks after reboot.
- Consistent dropdowns, toggles, tabs, and modal styling across Settings, Federation, and the rest of the UI; in Mesh chat, scrolling the conversation no longer also scrolls the contact list; "App Updates" and "App Registry" sit directly under Account in Settings.
- A fresh node no longer reinstalls apps just because their definition file exists on disk — only apps you actually installed come back.

## v1.7.99-alpha (2026-06-17)

- Your node can now hold Fedimint ecash as well as Cashu. Wallet Settings now has tabbed sections for each: keep your list of trusted Cashu mints, or paste a Fedimint invite code to join a federation, and the home wallet card shows both your Cashu and Fedimint balances side by side. A new "Fedimint Client" app in the catalog powers the federation side.
- You can now buy files shared by another node, right from their cloud. When you open a peer's paid file you get a simple "Buy this file" picker with several ways to pay — instantly from this node's ecash balance, from your node's own Lightning wallet, on-chain from your node, or by scanning a Lightning QR code with any outside wallet. Once payment settles, the file downloads automatically.
- Your node can now act as an AI assistant on the off-grid mesh radio network. If your node has a local AI model available (via Ollama), other people on the mesh can ask it a question by starting their message with "!ai" and get an answer back over the radio — handy where there's no internet. A new Mesh assistant panel lets you turn this on or off and shows whether a local AI model was detected.
- You can now view your node's 24-word recovery phrase whenever you need it. Settings has a new "Recovery phrase" option that, after you confirm your password (and 2FA code if you use one), reveals the words behind a tap-to-show blur with a copy button — so you can write them down and store them safely offline.
- Setting up a brand-new node is smoother and less alarming. If the node is still starting up while you generate or confirm your recovery phrase, it now quietly waits and retries instead of flashing a scary error, and offers a clear "Try again" button only when something genuinely goes wrong. The final setup screen also shows a gentle "securing your private connection…" status that turns to "ready" on its own, so you can tell the encrypted transport is coming up rather than stuck.
- The NetBird VPN app now actually logs in. It was failing to reach its sign-in screen because the dashboard needs a secure (HTTPS) connection that wasn't being provided; the node now serves it over HTTPS and opens it in a browser tab, so the login flow completes.
- When you use your phone to remote-control a node's attached screen, two-finger scrolling now works inside apps and panels, not just the main page. And tapping an app that's meant to open in an external browser now hands the link to your phone to open there, instead of trying to open it on the (often unattended) attached display.
- You can now choose whether your node shares Bitcoin block headers over the mesh. The Mesh Bitcoin panel has new switches to announce headers to peers and to accept headers from them, and your choices are remembered.
- Version numbers now display cleanly everywhere. In a few places the interface was showing a doubled "v" (like "vv1.7.98"); it now always shows a single, tidy version label.
- The "Back" buttons throughout the cloud and other detail screens now look and behave consistently on both desktop and mobile, including when browsing another node's files.
- For advanced testing, Settings now includes an optional "update & app source" choice between the usual trusted origin and an experimental peer-to-peer (DHT swarm) mode that pulls updates and app content from other nodes first, falling back to the origin automatically. The trusted origin remains the default.

## v1.7.98-alpha (2026-06-16)

- Apps that crash now recover on their own. Multi-part apps like Immich and IndeedHub could have one of their pieces stop and stay stopped until the whole node was rebooted; the node now checks every couple of minutes and restarts any crashed piece automatically (while still leaving apps you deliberately stopped alone).
- The on-screen kiosk display can no longer slow the whole node down. On machines without a graphics chip the kiosk browser could spin a CPU core at full tilt, starving everything else (including the wallet, which then timed out); it's now capped and uses lighter rendering on those machines.
- If an update download fails, you're taken back to the Download button to retry, instead of being stranded on an Install button for an update that didn't actually finish downloading.
- Your node's identity is clearer and always visible: Settings now shows your Node DID on every node (it previously only appeared if your browser had cached it) plus your node's npub, both with copy buttons. There's also a terminal tool to cryptographically prove all your node's keys come from your one seed phrase.
- The "all nodes over Tor" group chat sends quickly now — the "sending" spinner clears as soon as the reachable nodes have the message, instead of hanging on a slow or offline node.
- Message notifications now have a close button and open the relevant chat when tapped.
- The encrypted mesh transport (FIPS) turns itself on automatically after setup — no button to press — and connects to peers more reliably (it retries and keeps connections warm), so node-to-node features use the fast path more often instead of falling back to Tor.
- Your chat history with other nodes is saved reliably and now encrypted on disk, so it survives restarts and updates and can't be read from a stolen drive (only clearing chat removes it).
- Peer media shows a "connecting" loader before a video or audio file plays, and audio errors are accurate instead of blaming File Browser.
- The Fedimint app now displays with its proper styling, and the Connected Nodes screen stays compact — it shows a few nodes and scrolls, you can tap a node to jump to it in Federation, or tap Message to open its chat.
- App updates can now arrive on their own without waiting for a full system release, so individual apps can be improved and shipped faster.

## v1.7.97-alpha (2026-06-16)

- The Bitcoin sync status on the home screen no longer disappears for a moment when it refreshes. If the node was briefly busy, the panel used to vanish and pop back; it now stays put and simply shows "Updating…" until the next reading arrives, while a genuinely stopped node still correctly shows as not running.
- Bitcoin sync progress on the home screen now updates more promptly, so the percentage and block height keep pace with the node instead of lagging behind.
- The Lightning wallet "connect your wallet" screen loads its details and QR code again across all nodes, instead of failing to fetch them.
- Your list of trusted nodes is now clean: the same node no longer appears several times under different names, and removed nodes stay removed. In chat, a node that previously showed up as two separate contacts now appears just once.
- Browsing another node's cloud is smoother: music and video files from a peer now preview and play properly (including seeking partway through), and the connection now shows a small badge telling you whether it's using the fast encrypted mesh or the slower Tor network.
- Opening "My Folders" in the cloud now shows a clear, friendly message when the file app isn't running, instead of a confusing error.
- The Electrum server app opens on its own once it's ready, instead of sometimes leaving a loading spinner stuck on top of the screen.
- The Fedimint app now displays with its proper styling and icons, instead of appearing unstyled with a missing image.
- The Mempool app now connects to your Bitcoin node whether the node is Bitcoin Core or Bitcoin Knots, instead of only working with one of them.
- Nodes start up cleanly after a reboot. On some boots the node's main service was trying to start before its data drive had finished mounting, so it failed and retried about twenty times over roughly five minutes — showing a wall of "Failed to start" messages — before finally coming up. It now waits for the data drive to be ready first, so it starts on the first try.
- The background images throughout the interface now load faster — they've been made significantly smaller with no loss of quality.

## v1.7.96-alpha (2026-06-15)

- The screen attached to your node now shows the normal Archipelago interface and your dashboard after you sign in, instead of a separate, stripped-down grid of app icons that could appear in its place. That extra screen has been removed so the attached display matches what you see everywhere else.
- On a brand-new node, the attached screen now walks through the same welcome and setup steps you'd see on a phone or laptop, and shows the normal sign-in screen once the node is set up — so the on-device display always matches the rest of the interface.
- When adding a FIPS network anchor, you can now choose whether it connects over TCP (for a public anchor reached across the internet) or UDP (for one on your local network), instead of it always assuming the local-network option.
- Behind the scenes, a new automated two-node test now exercises real node-to-node features — browsing another node's shared files and handling a removed node — against live nodes before each release, so node-to-node problems are caught earlier.

## v1.7.95-alpha (2026-06-15)

- Browsing another node's shared files now works over the fast encrypted mesh. Opening a peer's cloud could fail with a generic "Operation failed" message because the request for their file list wasn't permitted over the mesh and came back as "not found" — and it never retried over Tor. The mesh now serves the file list directly, and if a peer can't answer over the mesh the node automatically falls back to Tor instead of giving up.
- Nodes you remove from your federation now stay removed. Previously a deleted node could quietly come back the next time you synced with another node that still listed it. Removed nodes are now remembered as removed and won't reappear on their own — only if you add them back yourself.
- The app credentials pop-up now appears as a normal centred box with a dimmed background over the whole screen, instead of stretching to fill the entire screen.

## v1.7.94-alpha (2026-06-15)

- Your node now joins the private encrypted mesh network on its own. A wrong built-in setting meant nodes were quietly never reaching the shared mesh meeting point, so everything between nodes fell back to the slower Tor network. Every node now connects to the mesh automatically on startup, so node-to-node features like file sharing use the faster encrypted mesh first and only fall back to Tor when a peer is genuinely offline. (Confirmed live: a node with its mesh setting wiped re-connected to the mesh by itself within a second of starting.)
- You can now bring the mesh networking software up to the latest stable version straight from the node, with one action — it fetches the new version, checks it's genuine before installing, and restarts the mesh on its own. (Confirmed live end to end: a node on an older build was upgraded to the current stable release and rejoined the mesh automatically.)
- The Lightning wallet screen connects again on nodes where it was showing a "failed to fetch" error instead of your balance and channels. The wallet app and the node now talk to each other correctly, and the connection quietly repairs itself if its details drift after a restart.

## v1.7.93-alpha (2026-06-14)

- Receiving Bitcoin and Lightning works again on nodes where the Lightning wallet was stuck locked. After some updates the wallet could come back locked with a password the node no longer had, so "generate a receive address" kept failing with a "wallet is locked" message that nothing could clear. The node now detects this and repairs itself automatically.
- Each node now secures its Lightning wallet with its own unique, randomly generated password instead of a shared built-in one, and remembers it safely so the wallet unlocks on its own after every restart or update — no more getting stuck locked.
- If a wallet is found locked with an unrecoverable password, the node rebuilds it cleanly so Bitcoin and Lightning start working again. (On these early-access nodes the wallet holds no funds, so nothing is lost — a wallet locked with an unknown password was already inaccessible.)
- The self-repair was validated end to end on live nodes: a stuck, locked wallet was detected, rebuilt, and came back unlocked on its own, and stayed unlocked across restarts.

## v1.7.92-alpha (2026-06-14)

- The Electrum server app no longer flashes a "can't connect, try again" error over its loading screen while it's still catching up. If ElectrumX is building its index or waiting on the Bitcoin node, you now just see the sync progress, and the app opens on its own once it's ready.
- Behind the scenes, the reboot-survival test now confirms the whole system is genuinely healthy after a restart — every app reachable, updates not stuck, core services answering — instead of only checking that containers came back, so update-related problems are caught before shipping.
- Settings → What's New now lists the notes for every recent release again. The screen had quietly fallen several versions behind, so the last eight releases of changes weren't showing up there — they're all back now, and a release check keeps it from drifting again.

## v1.7.91-alpha (2026-06-14)

- Apps you've installed now reliably show their "Open" button again. Some apps — including Jellyfin, BTCPay Server, Fedimint, Gitea and Portainer — were running fine but their launch link sometimes went missing, so there was no way to open them from the home screen. They now open correctly.
- Receiving Bitcoin is more dependable: if the wallet's internal connection details drift after a restart, it now repairs them on its own, and any error it does hit is reported clearly instead of as a generic failure or a misleading "wallet locked" message.
- Installing Bitcoin now sets itself up correctly without manual help — a security credential that could previously be missing and stop Bitcoin from starting is created automatically before it launches.
- The Electrum server app is back on the home screen and can be launched again.
- Behind the scenes, the release now runs an expanded automated test suite before shipping, so these kinds of issues are caught earlier.

## v1.7.90-alpha (2026-06-13)

- Generating a Bitcoin receive address works again — the wallet now requests the correct address type, fixing the "400 Bad Request" error when creating an address.
- In the companion app, the on-screen pointer can now click into apps and type — including the app store search box — instead of clicks and keystrokes not reaching app content.
- "Open in a new tab" from the companion app now opens the app in your phone's browser, instead of doing nothing. The normal mobile browser keeps working as before.
- The login/credentials pop-up on phones is once again a centered, properly sized window rather than stretching the full height of the screen.
- The Electrum server now recovers on its own if its index ever gets corrupted, and shows a clear progress screen (with percent complete and block height) while it builds its index, instead of a blank or broken page.
- Software updates are more reliable on slow internet connections — downloads are given much more time to finish before giving up.

## v1.7.89-alpha (2026-06-12)

- The AI assistant looks the way it always did again: no extra back button or close button on phones, and the desktop view fills the whole screen without a gap at the bottom.
- System updates are much more reliable: updates that previously got stuck partway or failed to install now complete cleanly, and a failed update can no longer block all future updates.
- After an update, the system now checks itself correctly on every node type, so working updates are no longer mistakenly undone.
- Generating a Bitcoin receive address works again on nodes where a network proxy previously got in the way.
- The Lightning wallet now recovers and unlocks itself properly after restarts.

## v1.7.88-alpha (2026-06-12)

- AIUI now loads immediately again instead of waiting on a production availability probe and cache-busted iframe URL, restoring the lighter launch behavior from before the regression.
- Bitcoin receive now uses LND's GET-based newaddress flow with the native SegWit address type, fixing the `501 Method Not Allowed` response from the previous POST attempt.
- Validation pending on the AIUI rollback; the rest of the release train remains unchanged.

## v1.7.87-alpha (2026-06-12)

- Bitcoin receive now calls LND's on-chain address endpoint with the correct REST method, and backend failures keep the specific address-generation error instead of collapsing into the generic operation-failed message.
- App launch credential interstitials now render as true full-screen overlays, and the launcher loading indicator uses the neutral brand palette instead of a blue spinner.
- Validation passed with `git diff --check`, `npm run type-check`, and the focused frontend tests for `bitcoinReceive` and `AppIconGrid`.

## v1.7.86-alpha (2026-06-12)

- Fleet now preserves the last known node list, alerts, and selection locally while telemetry refreshes in the background, so the dashboard no longer blanks on tab switches or update scans.
- Connected nodes and identities now reuse their last loaded data instead of reloading the visible list every time the user revisits the tab.
- The Fleet matrix and detail views now show actual node names and host information instead of raw node id prefixes.
- The network map only redraws when its graph data actually changes, which stops the D3 scene from visually resetting on every refresh tick.
- Mobile federation and system-update actions now stack full width, and the ElectrumX app health check allows a long startup window so slow sync nodes do not restart mid-index.
- Validation passed with `git diff --check`, focused frontend tests, and `npm run type-check`.

## v1.7.85-alpha (2026-06-12)

- ElectrumX now runs with less cache pressure and more memory headroom, reducing the restart loop seen during sync catch-up.
- Portainer is pinned to `2.19.4` instead of `latest`, avoiding schema-drift restarts from surprise image updates.
- LND receive-address creation now asks for a native SegWit address and returns clearer wallet/readiness failures when an address is not available.
- Fleet telemetry now carries server name, hostname, and server URL, and the Fleet dashboard shows those names instead of hashed node ids.
- Trusted federation peers are still auto-added transitively, but the local node no longer imports itself back into the fleet list.
- Validation passed locally for the touched frontend helpers, `git diff --check`, and Rust formatting.

## v1.7.84-alpha (2026-06-11)

- Bitcoin trusted-node relay approvals now generate restricted `txrelay` RPC credentials when needed and restart the active Bitcoin backend so bitcoind loads the new `rpcauth` whitelist.
- Kiosk mode now includes a browser safe-area path for HDMI displays that crop edges, and self-update refreshes kiosk launcher/systemd files so display fixes ship to existing nodes. The experimental X11 scaling safe-area is opt-in to avoid stretching TV output.
- Wi-Fi setup now reports scan errors instead of showing an empty network list, supports retrying scans from the modal, parses escaped `nmcli` SSIDs correctly, and can join open networks without forcing a WPA password.
- Bitcoin Core now matches Bitcoin Knots for restricted relay RPC support, including the txrelay secret injection and transaction broadcast whitelist.
- The restricted Bitcoin relay whitelist now includes `submitpackage` and `gettxout`, covering newer wallet/package-relay broadcast flows without opening wallet/admin RPC.
- The Bitcoin UI companion image is pinned to `1.7.84-alpha` across release metadata and the Quadlet fallback path, avoiding stale `latest` detection during OTA updates.
- Container scanning now uses an RAII in-flight guard so timeout and error paths cannot leave the scanner stuck in a permanently busy state.
- Validation passed with `cargo fmt`, `cargo check -p archipelago`, `git diff --check`, and focused source review of the relay message/approval path.

## v1.7.83-alpha (2026-06-11)

- App launch metadata now derives more consistently from app manifests, with typed launch interfaces and catalog generation updates that keep packaged apps aligned with their runtime ports and launch surfaces.
- Revoked or unsupported app surfaces were removed from the catalog and release path, including OnlyOffice and the unvalidated Saleor surface, so the Marketplace no longer exposes apps that cannot be safely supported in this release.
- The frontend production build now passes strict TypeScript checks after tightening app details, Web5, cloud refresh, and credential test typing.
- Mobile and desktop app surfaces received release polish: improved mobile app layout, safer mesh desktop/tablet scrolling, and the Home system card now routes directly to monitoring.
- Bitcoin UI status rendering now avoids false stale/reconnecting states when fresh block snapshots advance, and guards optional DOM updates so the standalone Bitcoin UI is more resilient.
- Deploy tooling now excludes local Codex scratch output, archived image-build artifacts, and upload screenshots from target syncs, and bounded optional IndeedHub fixups so a stuck Podman helper cannot hold the deploy.
- Validation passed with `npm run type-check`, production `npm run build`, backend `cargo build --release`, catalog/release manifest checks, focused frontend tests, and live `.198` deploy verification through the frontend/service restart phase.

## v1.7.82-alpha (2026-05-22)

- Saleor storefront proxying now forwards `X-Forwarded-Host`, fixing Next.js Server Actions requests that compared the browser origin with the internal `storefront-app:3000` upstream host.
- Saleor storefront media now routes `/thumbnail/` and `/media/` through the same `9011` proxy to the Saleor API, fixing product image optimizer failures caused by `localhost:8000` media URLs.
- The Saleor storefront container receives an explicit internal media origin so rewritten media URLs resolve inside the Podman network without exposing private API ports to browsers.
- Validation passed with `cargo fmt --all --check --manifest-path core/Cargo.toml`, `cargo check -p archipelago --manifest-path core/Cargo.toml`, and live checks on the staging node for storefront HTML, static assets, GraphQL, media redirects, and optimized product images.

## v1.7.81-alpha (2026-05-21)

- Saleor storefront installs now use the prebuilt registry image instead of building the Next.js app on-device, avoiding Podman build failures during stack installation.
- Existing Saleor stacks are repaired on adoption by recreating missing storefront containers, forcing the storefront app to bind `0.0.0.0:3000`, and resolving nginx upstreams dynamically after container restarts.
- The shipped Saleor storefront image now includes public assets and omits Vercel-only Speed Insights injection, fixing broken static asset responses and the local `/_vercel/speed-insights/script.js` browser warning.
- Validation passed with `cargo fmt --all --check --manifest-path core/Cargo.toml`, `cargo check -p archipelago --manifest-path core/Cargo.toml`, and live checks on the staging node for `9011` storefront, static assets, and proxied GraphQL.

## v1.7.80-alpha (2026-05-21)

- Saleor storefront proxying now falls back to the direct request scheme when no forwarded protocol header is present, fixing direct `http://node:9011` launches that could generate an invalid same-origin GraphQL URL.
- The Saleor storefront release path keeps public proxy support intact by still honoring forwarded HTTPS headers for Nginx Proxy Manager domains while repairing local/direct port launches.
- Validation passed with `cargo fmt --check` and `cargo check` for the Archipelago backend before release staging.

## v1.7.79-alpha (2026-05-20)

- Saleor now installs the official Saleor Storefront as part of the stack, built from the pinned `saleor/storefront` source and served as the customer-facing shop on port `9011`.
- Saleor app launches now open the storefront while the admin dashboard remains available on port `9010` with the generated `admin@example.com` credentials shown in Archipelago.
- Public Nginx Proxy Manager hosts forwarding to the Saleor storefront also expose same-origin `/graphql/`, so public storefront domains can talk to the local Saleor API without mixed-content or private-LAN reachability failures.
- Saleor stack metadata, marketplace descriptions, catalog ports, scanner exclusions, and app-session routing now describe the storefront/dashboard/API split explicitly.

## v1.7.78-alpha (2026-05-20)

- Public Nginx Proxy Manager hosts for Saleor now keep browser GraphQL calls same-origin at `/graphql/` and proxy them to the local API on `8000`, fixing `Failed to fetch` when a public domain such as `noderunner.shop` was loaded from devices that cannot reach the node's private LAN/tailnet API address.
- Saleor's validated stack changes are now release-ready: dashboard origins on port `9010` are explicitly allowed for dashboard/API calls, preserving the working test-node install path for production nodes.
- NetBird launches now stay pinned to the unified dashboard/proxy origin on port `8087` instead of following stale runtime-discovered server URLs on `8086`.
- NetBird's local nginx proxy now routes browser API, OAuth, relay, and WebSocket traffic through `host.containers.internal:8086` instead of a hard-coded rootless Podman gateway IP, and includes the upstream `management.ProxyService` gRPC path.
- The mobile credentials interstitial now keeps credential lists scrollable and action buttons reachable in both My Apps and the mobile app icon grid.
- Android WebView popup windows now hand external popup URLs to the system browser, covering app login/signup flows that open secondary windows.
- Validation passed with `git diff --check`, `cargo check -p archipelago`, and the focused `npm test -- src/views/appSession/__tests__/appSessionConfig.test.ts` suite.

## v1.7.77-alpha (2026-05-20)

- Saleor first-use now exposes generated credentials through Archipelago instead of leaving users at an unexplained dashboard login: App Details shows copyable `admin@example.com` credentials, and My Apps/mobile icon launches show a pre-launch credentials modal.
- Saleor installs now create or repair the `admin@example.com` staff account idempotently after sample data loads, use the correct dashboard mount path, and re-check stack containers after startup so stopped containers are caught.
- NetBird embedded login now uses the upstream-compatible IdP signing-key behavior and sends ID tokens from the dashboard to the management API, fixing the post-signup `Unauthenticated` state while preserving the unified local proxy/logout routes.
- Transient unnamed Podman helper containers created during app install tasks are hidden from My Apps, so generated names like `eager_keldysh` no longer appear as user applications.
- Validation passed with catalog/release JSON checks, `npm run type-check`, and `cargo fmt --all --check --manifest-path core/Cargo.toml`; live checks on the staging node confirmed Saleor dashboard/API availability, generated Saleor admin login, NetBird OAuth availability, and NetBird logout redirects.

## v1.7.76-alpha (2026-05-20)

- Saleor installs now use dashboard port `9010`, avoiding the existing Portainer `9000` binding on the test node while keeping API `8000`, Mailpit `8025`, and Jaeger `16686` unchanged.
- Saleor's Valkey cache no longer bind-mounts `/var/lib/archipelago/saleor-cache`, and the dashboard container has the minimal rootless nginx capabilities it needs to chown cache files, bind port 80 inside the container, and drop workers to the nginx user.
- NetBird's browser proxy now sends API, OAuth, relay, WebSocket, and management traffic through the stable host-published server port at `169.254.1.2:8086`, avoiding stale rootless Podman DNS/IPs after `netbird-server` restarts.
- Mobile App Store category chips now stay visible above the tab bar, Discover is available on mobile, and category selection updates the page route/query so the selected category is actually shown.
- Apps that require a real browser tab now open directly from the app icon tap instead of first entering an in-shell app-session route, including BTCPay, Grafana, Home Assistant, Vaultwarden, Nextcloud, Portainer, OnlyOffice, Tailscale, Uptime Kuma, Gitea, and Nginx Proxy Manager.
- Validation passed with catalog JSON checks, `npm run type-check`, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml`; live checks on a fleet node confirmed Saleor dashboard `9010`/API `8000` and NetBird API/OAuth routes survive `netbird-server` restart.

## v1.7.75-alpha (2026-05-19)

- Saleor is now published as a recommended commerce app with catalog metadata, icon, direct app-session launch on port `9000`, scanner metadata, image pins, and a full stack installer for dashboard, API, worker, PostgreSQL, Valkey, Mailpit, and Jaeger.
- Existing NetBird installs are repaired more aggressively by rewriting unified-origin config, recreating the dashboard/proxy containers, restarting the server, preserving data, and handling exact `/api` and `/oauth2` routes plus dashboard logout redirects through the local proxy.
- Desktop dashboard scrolling now hands focus back from the sidebar to the main content when the pointer or wheel moves over the main pane, preventing the sidebar scroll area from trapping wheel input on short screens.
- Validation passed with catalog JSON checks, `npm run type-check`, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml` before release.

## v1.7.74-alpha (2026-05-19)

- App-session right panels now re-focus the iframe after load and when the frame area is activated, so wheel/touch scrolling works immediately after switching tabs or selecting an app on shorter screens.
- NetBird now launches through a unified local origin on port `8087` that proxies the dashboard plus `/oauth2`, `/api`, relay, WebSocket, and gRPC routes to `netbird-server`, fixing the embedded login flow that previously ended in `Unauthenticated` or `404 page not found` after logout.
- Existing NetBird installs are repaired on adopt/start by rewriting `config.yaml`, `dashboard.env`, and the local nginx proxy config, then creating the missing `netbird-dashboard` and `netbird` proxy containers when needed while preserving NetBird data.
- Saleor is still pending and is not included in this release; its registry/installer work remains local until it can be validated separately.
- Validation passed with catalog JSON checks, `npm run type-check`, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml`.

## v1.7.73-alpha (2026-05-19)

- Mobile app launches for iframe-blocked apps now open the direct app URL in a new browser tab immediately instead of landing in a broken in-shell webview that requires a second tap.
- Mobile My Apps/Websites tabs now react to route query changes, App Store pages label the mobile view as Discover, mobile filters have safe bottom spacing, and App Store search ignores the current category so searches cover all available apps.
- My Apps search now surfaces matching App Store entries when the app is not installed, making it possible to jump directly from a failed My Apps search to the installable app details.
- NetBird self-host installs now prefer a `100.x` tailnet/CGNAT address for dashboard, management, relay, STUN, and auth redirect origins when one is present; live repair on a fleet node updated the existing stack from LAN origins to its tailnet address and restored `netbird-server`.
- App-session iframe frames now focus automatically and wrap the iframe in a scroll host so wheel/touch scrolling works in the active right frame without requiring an initial click.

## v1.7.72-alpha (2026-05-19)

- Settings What's New now includes the missing release notes for `v1.7.68-alpha` through `v1.7.71-alpha`, so the modal reflects the current OTA history instead of stopping at `v1.7.67-alpha`.
- The follow-up release carries the NetBird install fix, Gitea icon polish, mobile app-session fallback updates, and rounder app icon masks from `v1.7.71-alpha` with the Settings modal notes included.
- The local Cargo lockfile version metadata is kept in sync with the release bump after the previous release build updated it.

## v1.7.71-alpha (2026-05-19)

- NetBird stack installs now pre-create `/var/lib/archipelago/netbird/data` before binding it into `netbird-server`, fixing the failed install/start path seen on a fleet node where Podman rejected the missing host directory.
- NetBird start/restart ordering now starts `netbird-server` before the dashboard container so lifecycle actions bring the control plane up before the UI.
- App-session invalid IDs and panel-mode fallbacks now return to `/dashboard/apps`, avoiding the stale `/apps` route that could render a 404.
- Mobile launches for apps that block iframes now stay inside the Archipelago app-session fallback instead of automatically opening an external browser tab.
- Installed Gitea containers now report the packaged Gitea icon, and app icon masks use a rounder radius on mobile grids, app cards, and detail headers.
- Validation passed with `npm run type-check`, focused Vitest app-session/app-grid tests, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml`.

## v1.7.70-alpha (2026-05-19)

- NetBird is being corrected from the peer/client daemon image to the self-hosted NetBird control-plane stack with a launchable dashboard on port `8087`, a combined management/signal/relay server on `8086`, and STUN on UDP `3478`.
- App sessions now always launch local apps through direct host ports and carry an explicit dashboard return target, so closing an iframe returns to the launching dashboard screen instead of falling through to browser history or a 404.
- Mobile app launches ignore stale desktop panel state and route into the full app-session webview consistently.
- The desktop sidebar now pins the logo/version at the top and controller/online/mode controls at the bottom, with only the navigation section scrolling on shorter screens.
- Validation passed with catalog JSON checks, `scripts/image-versions.sh` syntax check, `npm run type-check`, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml`.

## v1.7.69-alpha (2026-05-19)

- App installs now allow up to 10 minutes for the initial `package.install` RPC to return, matching slow container image pulls and preventing apps from disappearing from My Apps while the backend is still pulling or retrying mirrors.
- Live diagnostics on a fleet node confirmed the Gitea install did not fail; the primary registry pull timed out after 300 seconds, the fallback mirror succeeded, and Gitea came up healthy on `3001` while the frontend had already timed out at 15 seconds.
- Gitea and other Docker-image app installs now stay visible during slow registry pulls instead of being marked as failed by the browser before backend install progress can complete.
- Gitea is now categorized as a known Data app in My Apps, so a running Gitea container appears with installed apps instead of being filtered into the Websites/Services split.
- NetBird `0.71.2` is now available in the app catalog and fallback marketplace data as a recommended networking app using the official `docker.io/netbirdio/netbird:0.71.2` image.
- NetBird installs get persistent state under `/var/lib/archipelago/netbird`, `NET_ADMIN`/`NET_RAW`, `/dev/net/tun`, `slirp4netns`, image-version pinning, backend metadata, and health checks through `netbird status`.
- The Archipelago terminal now includes `nano` on new disk installs and ISO builds, and self-update installs it on existing nodes if it is missing.
- Validation passed with catalog JSON checks, shell syntax checks, `npm run type-check`, `cargo fmt --all --check --manifest-path core/Cargo.toml`, and `cargo check -p archipelago --manifest-path core/Cargo.toml`.

## v1.7.68-alpha (2026-05-19)

- BTCPay Server now ships on the official `docker.io/btcpayserver/btcpayserver:2.3.9` image, fixing the plugin catalog crash caused by newer plugin dependency version metadata while preserving existing datadirs and Postgres databases.
- BTCPay release and first-boot health checks no longer depend on `curl` inside the container; they use a bash TCP probe that works with the official image out of the box.
- Host nginx now serves Nginx Proxy Manager HTTP-01 challenge files before the Archipelago SPA fallback and is marked as the default HTTP/HTTPS virtual host, so public proxy hosts can issue certificates without hijacking local API traffic.
- Nginx Proxy Manager first-boot, runtime repair, and container-doctor paths now pre-create the ACME webroot, keep bind mounts owned by the rootless Archipelago user, and sync issued public proxy hosts into host nginx vhosts.
- The Nginx Proxy Manager host-nginx sync now skips proxy hosts with missing certificate files and rolls back the generated nginx include if validation fails, preventing a bad certificate path from poisoning later nginx reloads.
- App session close buttons now return to the previous dashboard screen when possible and otherwise fall back to My Apps, avoiding the 404 page after closing an app launched from an invalid or stale history entry.
- System Update confirmation and mirror modals now teleport to the document body with a full-screen overlay, so they cover the whole app instead of only the right-hand dashboard panel.
- Mobile app launches stay inside Archipelago's app-session webview and hide desktop-only new-tab launch affordances, including apps such as Home Assistant that previously looked like they would leave the mobile shell.
- Live recovery on a fleet node upgraded only the `btcpay-server` container to `docker.io/btcpayserver/btcpayserver:2.3.9`, preserved the existing datadir and Postgres database, and confirmed the container is healthy after a pre-upgrade backup.
- Public validation confirmed ``the BTCPay host`/`www` redirect to BTCPay login over HTTPS and `the L484 host`/`www` serve the L484 page over HTTPS using the issued Let's Encrypt certificates.

## v1.7.67-alpha (2026-05-18)

- Home dashboard status cards now keep the last known good system, VPN, Bitcoin, and FIPS values while route changes or transient RPC failures are in flight, avoiding false "not configured" or "not running" flashes.
- Home, Web5 Monitoring, and the Monitoring page headline cards now share the same live system-stat snapshot for CPU, memory, disk, uptime, and load so the visible numbers agree across the UI.
- Settings What's New is filled through `v1.7.67-alpha`, including the missing historical `v1.7.44-alpha` through `v1.7.66-alpha` entries.
- Bitcoin/Knots/Core shell lifecycle specs now match the Rust app config memory policy: 8 GiB on normal hosts, 4 GiB on low-memory hosts, and pruned Knots uses a larger dbcache on hosts with enough RAM to improve IBD throughput.
- ElectrumX/electrs shell lifecycle specs now match the 4 GiB memory policy used by the Rust app config, reducing drift between first boot, reconcile, and app lifecycle paths.
- Live assessment of a fleet node identified the current IBD bottlenecks as CPU/thermal/I/O pressure rather than RAM exhaustion, with follow-up work planned for existing-node swap repair, kiosk Chromium CPU reduction, and reconcile failure cleanup.

## v1.7.66-alpha (2026-05-18)

- Nginx Proxy Manager stale-port repair now detects stopped or `Created` Podman records by inspecting `podman ps -a` port metadata, covering records where `podman port nginx-proxy-manager` returns no mapping until start.
- Live recovery on a fleet node removed only the stale Nginx Proxy Manager container record and recreated it with `8081:81`, `8084:80`, and `8444:443`, preserving `/var/lib/archipelago/nginx-proxy-manager` data.
- Validation confirmed Nginx Proxy Manager recovered as healthy and responds through direct admin port `8081`, host compatibility port `81`, and `/app/nginx-proxy-manager/`.

## v1.7.65-alpha (2026-05-18)

- Orchestrator-backed app starts now run the same pre-start repairs as the legacy Podman path, so Nginx Proxy Manager stale `81:81` container metadata is removed and recreated before the orchestrator tries to start it.
- Live diagnostics on a fleet node confirmed host nginx is healthy while Nginx Proxy Manager has no listeners on `8081`, `8084`, or `8444`, causing host nginx `502` responses for NPM proxy paths.
- The gap this closes: apps launched through the orchestrator previously skipped the legacy start-time repair path entirely, so the same stale metadata the old flow cleaned up silently broke the new one. Both paths now converge on the same repairs.

## v1.7.64-alpha (2026-05-18)

- Update apply rate limiting is relaxed for authenticated admins from 2 attempts per 10 minutes to 10 attempts per minute, preventing the System Update page from getting stuck behind `429 Too Many Requests` during legitimate OTA retry/troubleshooting flows.
- The corrected backend artifact rebuild protection from `v1.7.63-alpha` remains in place, so this release is built from a fresh Rust backend binary before publishing.
- For operators mid-incident this changes the recovery loop: a failed apply can now be retried immediately from the System Update page instead of waiting out a throttle window while a node sits half-updated.

## v1.7.63-alpha (2026-05-18)

- Release automation now rebuilds the Rust backend after bumping the version and before hashing release artifacts, preventing OTA manifests from pointing at a stale backend binary.
- This corrected release carries the Nginx Proxy Manager stale-port repair in an updated backend binary, so nodes running `1.7.61-alpha` can actually receive and execute the fix.
- Validation confirmed the previously published `v1.7.62-alpha` backend artifact still contained `1.7.61-alpha`, explaining why nodes did not advance after applying that update.

## v1.7.62-alpha (2026-05-18)

- Nginx Proxy Manager start and restart now repair stale Podman containers that still publish the admin UI on host port `81`, which conflicts with host nginx on updated nodes.
- The repair recreates only the stale Nginx Proxy Manager container metadata while preserving `/var/lib/archipelago/nginx-proxy-manager` data and using the current `8081:81`, `8084:80`, and `8444:443` mappings.
- Runtime stale-listener cleanup for Nginx Proxy Manager is shared across start and restart paths so rootless port helper leftovers are still cleared before lifecycle retries.
- Validation passed with `cargo fmt --all --check --manifest-path core/Cargo.toml` and `cargo check -p archipelago --manifest-path core/Cargo.toml`.

## v1.7.61-alpha (2026-05-18)

- Multi-container stack installs now keep their app card in the `Installing` state for up to 20 minutes while dependency containers are being pulled and prepared.
- BTCPay Server installs no longer appear to vanish or fail after two minutes while Postgres and NBXplorer are still being created before the primary `btcpay-server` container exists.
- The stale-transition escape hatch remains short for start, stop, restart, update, and removal operations, so genuinely wedged lifecycle actions still recover quickly.
- Live validation on a fleet node confirmed BTCPay Server completed installation and responds on port `23000` with the expected HTTP redirect.

## v1.7.60-alpha (2026-05-18)

- Meshtastic serial detection now rejects malformed or incomplete handshakes instead of accepting unrelated serial devices as a fallback Meshtastic radio.
- Mesh radio auto-detection now skips known non-mesh serial devices such as Sierra Wireless LTE modems and Zooz/Z-Wave sticks, avoiding interference with production peripherals.
- Meshtastic config sync now sends `want_config_id` with the correct protobuf wire type, fixing radio-side `ignore malformed toradio` errors and allowing node-info/contact ingestion.
- The stable `/dev/mesh-radio` udev rule no longer claims every `ttyACM*` device; it only matches known mesh USB serial adapters and known USB CDC ACM radio vendors.
- Live validation on a fleet node confirmed Archipelago selects `/dev/ttyUSB0`, identifies the Meshtastic node, and refreshes 103 mesh contacts.

## v1.7.59-alpha (2026-05-17)

- Mobile app launching now keeps known container apps inside Archipelago's app-session flow instead of forcing desktop-only new-tab behavior on phones.
- App sessions on mobile now respect the status-bar safe area so foreground iframe content starts below the device chrome while the fullscreen backdrop remains edge-to-edge.
- Prepackaged website launch buttons now resolve their curated website URLs before website-container fallback logic, restoring launches for the L484 sites and adding the Arch Presentation bookmark.
- Meshtastic contact discovery now drains the radio config stream through completion and retries config sync when the contact cache is empty, so nearby nodes already known by the radio are more likely to appear in Archipelago.
- The Apps page now includes a compact sideload button and modal for installing trusted Docker images with optional title, description, and port mapping metadata.
- Sideloaded app title and description metadata now persist through the backend app-config file so refreshed package scans do not collapse custom apps back to generic IDs.
- Validation passed with `npm test -- appLauncher`, `npm run build`, `cargo check -p archipelago`, and `cargo fmt --all --check`.

## v1.7.58-alpha (2026-05-17)

- Mesh networking now supports Meshtastic radios over the Meshtastic serial API in addition to existing MeshCore Companion USB radios.
- The mesh listener now probes preferred and auto-detected serial paths for both MeshCore and Meshtastic firmware, preserving the existing reconnect loop so unplug/replug and firmware hot-swap behavior stays consistent.
- Meshtastic text packets are translated into the existing Archipelago mesh frame pipeline, so current RPC handlers, transport routing, message storage, typed-message decoding, and UI state continue to work without a separate frontend path.
- Meshtastic node information is surfaced as normal mesh contacts using stable synthetic public keys derived from Meshtastic node numbers, allowing peer refresh and message attribution to reuse existing MeshCore contact handling.
- Outbound Archipelago mesh messages can now be sent through Meshtastic as channel text packets using the same command path used by MeshCore channel broadcasts.
- Device status now reports the detected firmware family as `meshcore` or `meshtastic` from the shared listener abstraction.
- Radio udev rules now include USB CDC ACM serial devices (`ttyACM*`) alongside CP2102, CH340, and FTDI adapters so Meshtastic boards are more likely to appear through the stable `/dev/mesh-radio` symlink.
- Host nginx now serves `/assets/*` hashed frontend chunks as immutable static files with a hard 404 on misses instead of falling back to `index.html`, preventing strict MIME errors when a browser has a stale pre-update HTML shell.
- The SPA HTML shell and service-worker files now revalidate on every load, reducing stale frontend references after OTA updates.
- OTA runtime promotion now installs the bundled `nginx-archipelago.conf` into `/etc/nginx/sites-available/archipelago` and reloads nginx after a successful config test, so frontend cache/fallback fixes reach existing nodes without a manual deploy.
- Local validation passed with `cargo check -p archipelago`; live SSH testing against a fleet node was not completed because temporary public-key authentication was rejected on the target.

## v1.7.57-alpha (2026-05-17)

- Nginx Proxy Manager now avoids privileged rootless Podman host port `81`, preferring `8081:81` while host nginx keeps a compatibility proxy on `:81` for stale cached launch buttons.
- App installs now allocate ports by checking live host bind availability, falling back to a free high port when preferred ports are already occupied.
- Portainer-created launchable containers are separated into a `Websites` tab and launch through their discovered published host port instead of hard-coded app URLs.
- Internal BuildKit helper containers such as `buildx_buildkit_default` are hidden from the Apps UI.
- Portainer works out of the box on Debian 13/Podman installs by including `catatonit` and by preserving the Podman socket mount as a socket rather than creating it as a directory.

## v1.7.56-alpha (2026-05-15)

- Health notifications now clear when an app is no longer unhealthy, including stale alerts for removed containers such as Portainer.
- Fresh installs now include the full Wi-Fi userspace stack (`wpasupplicant`, `wireless-regdb`, `iw`, `rfkill`, `polkitd`, `pciutils`, and `usbutils`) so NetworkManager can scan and connect with Intel Wi-Fi cards out of the box.
- The installed system now grants the `archipelago` service user explicit NetworkManager PolicyKit access for web-triggered Wi-Fi scans and connection changes.
- Wi-Fi connect now replaces stale/partial NetworkManager profiles and creates an explicit WPA-PSK profile with the supplied password, avoiding no-secret retry failures after a failed attempt.
- Settings password changes now update the Linux/SSH password through non-interactive sudo, so the web password and SSH password stay in sync when the checkbox is enabled.
- Quadlet environment values with spaces or shell metacharacters are quoted consistently, preventing env drift recreate loops for apps like nostr-rs-relay and Grafana.
- Boot/bootstrap reconcile avoids restarting running Bitcoin containers while repairing RPC config, preserving IBD progress on active nodes.
- Exit code 137 is labeled as SIGKILL instead of assuming OOM, avoiding false OOM alerts for orchestrator-managed recreates.
- Container reconcile force-recreates Podman records stuck in `Stopping`, preserving bind-mounted app data while recovering wedged containers automatically.
- Container health reporting is honest for running containers: Archipelago surfaces Podman's actual health state instead of marking every running container healthy.
- Quadlet reconciliation restarts services when stale health gates, port bindings, network aliases, exec commands, or healthchecks drift from the current manifest.
- Bitcoin Knots sync performance improves on fresh installs and updates with 8Gi container memory, a 4Gi dbcache, and full CPU parallelism.
- ElectrumX initial indexing gets more headroom: CPU caps are removed, memory is raised to 4Gi, cache is raised to 3Gi, and oversized sends are allowed for heavier wallet/indexing workloads.
- Mempool/ElectrumX lifecycle qualification respects pruned/non-archival Bitcoin nodes instead of installing a half-running stack with unhealthy dependencies.
- LND wallet/RPC helpers are more tolerant of container-owned files and updated REST port metadata, improving LND lifecycle and wallet-connect flows.
- Marketplace/catalog metadata carries richer container config so remote lifecycle tests install apps using the same settings users get from the UI.
- The app screensaver no longer activates during media-heavy app sessions such as IndeeHub, Jellyfin, Immich, PhotoPrism, and File Browser; apps can also pause/resume it with media playback messages.
- A fresh `1.7.56-alpha` unbundled installer ISO is built from the same primary VPS2 release line for easy download and USB flashing.

## v1.7.55-alpha (2026-05-13)

- Container reconcile now force-recreates Podman records stuck in `Stopping`, preserving bind-mounted app data while recovering wedged containers automatically.
- `.198` is green after the container-layer hardening pass: focused and broad non-destructive lifecycle audits pass, raw Podman health/state sweep is clean, and direct app probes return healthy responses.
- Release-candidate artifacts are staged separately from live update publishing while Gitea artifact hosting is repaired.

## v1.7.54-alpha (2026-05-06)

- Existing installs now self-repair nginx backend proxy locations for `/bitcoin-status` and `/api/app-catalog`, including hosts where `sites-enabled/archipelago` is a copied active file instead of a symlink.
- LND UI is consistently served on `18083` across first boot, Tor config, companion Quadlet reconciliation, OTA runtime payloads, and ISO scripts; stale companion units/images are rewritten instead of only checking service active state.
- OTA frontend tarballs now carry a clean runtime payload with updated scripts, docker UI sources, and canonical nginx config, preventing startup promotion from reintroducing stale host assets.
- Release ISO builds now support the primary HTTP app registry when bundling core images, so unbundled media includes File Browser/Cloud support instead of requiring a post-install Marketplace download.
- `.116` was live-updated with the new backend and runtime scripts; focused non-destructive lifecycle audit passes for Bitcoin Knots, LND, BTCPay, Mempool, and Grafana.

## v1.7.53-alpha (2026-05-05)

- Bitcoin Knots/Core config generation no longer duplicates RPC bind and port settings between `bitcoin.conf` and container command args, fixing `Unable to bind all endpoints for RPC server` startup failures.
- Legacy Bitcoin container healthchecks no longer depend on `bitcoin-cli`, which is absent from current Knots images and can wedge Podman healthcheck runners.
- Update checks now prefer manifest OTA releases over stale git remotes unless `ARCHIPELAGO_GIT_UPDATES` is explicitly enabled, so installed nodes can see published releases from the VPS mirror.

## v1.7.52-alpha (2026-05-05)

- Tailscale now launches the local installed web UI on port `8240` and starts `tailscaled` before `tailscale web`, fixing unreachable installs after container creation.
- Grafana install/start/restart now repairs missing rootless host listeners on port `3000`, matching the existing SearXNG, Uptime Kuma, and Gitea recovery path.
- Debian 13/Trixie ISO and disk-install paths now force security updates from `trixie-security` during image/install creation so rebuilt release media includes patched base packages.
- Broad `.198` lifecycle audit passes with the current qualified app set; known absent blockers remain `electrumx`, `photoprism`, `dwn`, and `ollama`.

## v1.7.51-alpha (2026-04-30)

- Stack installs now adopt containers that already exist instead of failing on them — a repair or reinstall over leftover containers completes, and the adopted container's readiness is waited on like any fresh start.
- Failed installs come with evidence: the install path waits for its containers, and when one doesn't become healthy it captures that container's logs, so the error on screen names the real culprit instead of a bare timeout.
- Bitcoin RPC bindings are ensured as part of install, and the startup self-heal path gained additional ground for already-deployed nodes.

## v1.7.50-alpha (2026-04-30)

- The OTA bridge older nodes needed: deployed binaries only knew how to apply two artifacts (the backend binary and the frontend archive), so the scripts, app specs and docker assets newer releases carry never reached them. This release packs those payloads inside the frontend tarball — the one channel old binaries do apply — and the new backend promotes them into /opt once it starts.
- Runtime payloads are staged into timestamped directories and promoted atomically; a failed extraction cleans up its staging area instead of leaving half-written state for the next update to trip over.
- This is the release that un-sticks the fleet's update pipeline: from here on, an OTA can carry more than the two artifacts, and app installs on updated nodes use the specs that match their backend.

## v1.7.49-alpha (2026-04-30)

- Bitcoin Knots/Core UI now reports connection, reconnecting, syncing, and error states from a backend status bridge instead of showing a stale "Unable to connect" message while the node is warming up.
- ElectrumX UI now exposes indexed height, local Bitcoin height, known headers, status, and progress source so indexing/waiting states are readable during long initial sync.
- Added container doctor timer and smoke/lifecycle test coverage for Bitcoin Knots/Core, ElectrumX, Mempool, BTCPay/NBXplorer, and UI surface availability.
- Bitcoin Core and Bitcoin Knots are mutually exclusive variants, with a real Bitcoin Core manifest and corrected install conflict handling.
- IndeeHub now launches only on direct web UI port `7778`; the broken `/app/indeedhub/` path proxy was removed, and port `7777` remains the Nostr relay.
- BTCPay/NBXplorer Postgres environment formatting fixed so installs do not carry malformed connection strings.

## v1.7.48-alpha (2026-04-29)

- archipelago.service no longer fails to start with "Failed to set up mount namespacing: /run/containers: No such file or directory" on nodes where that runtime directory wasn't pre-created — the failure surfaced in systemd's mount-namespace setup before the service itself ever ran.
- ExecStartPre now creates /run/containers before the service starts, so the node's service manager finds the directory it needs on every boot; ISO installs from this version forward have the fix baked in.
- Existing nodes pick the fix up with a one-time `systemctl edit archipelago` adding the mkdir — after which the boot failure does not recur.

## v1.7.47-alpha (2026-04-29)

- Bitcoin Knots/Core sync is now significantly faster. The container now uses every available core for script verification (was capped at 2) and has 8GB of memory instead of 4GB so its 4GB UTXO cache has headroom for the mempool and peer connections. Existing nodes pick up the new limits on next install/update; freshly-installed nodes start at full speed.
- ElectrumX initial indexing is faster too. Its CPU cap is removed, container memory is 4GB, and its internal cache is now 3GB (default was 1.2GB).
- The result: a fresh node's first hours are measurably shorter — initial block download and ElectrumX indexing were the two longest post-install waits, and both now run at the hardware's limit.

## v1.7.46-alpha (2026-04-29)

- Health monitor no longer pages "Auto-restart failed" for orphaned containers. After a variant switch (bitcoin-core ↔ bitcoin-knots) the previous variant's container could survive uninstall and the health monitor would try restarting it forever. Now skipped silently with a debug log.
- Apps no longer disappear from My Apps when an install fails. The card stays visible with state=Stopped so the user can retry or uninstall, with the failure reason surfaced via the new install_progress.message field.
- "Downloading…" progress now actually advances during multi-image stack pulls. Was sticking at 20% until all pulls finished; now interpolates 20%→70% based on which image of N has landed.
- Pulled four docker.io images (bitcoin, gitea, nextcloud, valkey) into the lfg2025 registries on the registry mirrors. Removes a docker.io dependency from first-boot installs.
- Resilience harness improvements: install-fail entries no longer vanish, install/uninstall/probe cells are timing-tolerant (60s retry on ui_probe and auth_probe), dep snapshots no longer leak companion containers into the dependent app's "new containers" set.

## v1.7.45-alpha (2026-04-29)

- Bitcoin RPC auth is durable. The dashboard reliably connects across container restart, image update, and reboot. Was failing on registry-pulled images that shipped a stale baked-in password.
- Multi-container apps show real install progress. IndeedHub (7), BTCPay (4), Mempool (3), Immich (3) — bar advances through Preparing → Pulling → Creating → Done instead of sitting at 0% until the very end.
- Apps no longer disappear from the dashboard mid-install. The container scanner now respects in-flight installs and updates instead of evicting an entry while its containers are still being created.
- IndeedHub installs cleanly on a fresh node. Five missing environment variables fixed; Nostr sign-in works on first install.
- Tailscale install no longer fails with "executable not found". Container command was a malformed shell string; now a proper command array.
- Removed three catalog entries that hung installs for ten minutes (dwn, endurain, ollama — no source images in our registries). Restored Nextcloud, sourced from docker.io.
- Bitcoin Core update path uses the correct image name (was pulling from a non-existent path).
- New ISO installs now allocate swap (sized to RAM, capped at 8GB, on the encrypted data partition). Without swap, container image builds and memory spikes were hitting OOM under load.

## v1.7.44-alpha (2026-04-28)

- Container orchestration migration completed, with release hardening across the app lifecycle — installs, updates and removals now run through one orchestrator path instead of the split legacy/Podman flows.
- OTA updates now rebuild and sync the app UI containers they carry, so an updated app serves the UI image that matches its backend instead of whatever happened to be on disk.
- LND UI port handling is aligned across all runtime specs, and release packaging moved to tarball-only payloads with the ISO build recipes archived — update payloads now carry only the files existing nodes need.


All notable changes to Archipelago will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.3.1] - 2026-03-25

### Security
- All crypto dependencies pinned to exact versions from Cargo.lock (supply chain hardening)
  - ed25519-dalek 2.1 → 2.2.0, sha2 → 0.10.9, hmac → 0.12.1, argon2 → 0.5.3, chacha20poly1305 → 0.10.1, zeroize → 1.8.2, hkdf → 0.12.4, aes-gcm → 0.10.3
- All container images pinned to exact patch versions (no more floating tags)
  - postgres:15 → 15.17, redis:7 → 7.4.8, nginx:alpine → 1.29.6-alpine, uptime-kuma:1 → 1.23.17, nextcloud:29 → 29.0.16, valkey:8 → 8.1.6, mariadb:11.4 → 11.4.10, and 7 more
  - DWN server pinned by SHA256 digest (only has `:main` branch tag)

### Reliability
- Nostr relay connections now have 10s timeout — prevents indefinite hangs blocking RPC calls
  - identity_manager.rs: publish_profile()
  - nostr_discovery.rs: publish_node_revocation(), verify_revocation(), discover_archipelago_nodes()
  - marketplace.rs: discover(), publish()

### Infrastructure
- CI pipeline added (.github/workflows/ci.yml) — cargo fmt, clippy, tests + frontend type-check, build
- Update system now fetches from the release Gitea instance (configurable via ARCHIPELAGO_UPDATE_URL)
- Cleaned up stale git branches (app-store, overnight/2026-03-12, overnight/2026-03-13)

## [1.3.0] - 2026-03-19

### Security

#### Pentest Remediation (33 findings, all addressed)
- **Critical**: Backend now binds to 127.0.0.1 only — no more direct LAN access to port 5678
- **Critical**: Fixed path traversal in Tor service management that could allow `sudo rm -rf` on arbitrary directories
- **Critical**: Fixed unauthenticated file read/delete via DWN recordId path traversal
- **High**: Federation peers now require cryptographic signature — unsigned peers rejected
- **High**: Login redirect XSS vulnerability fixed with proper URL validation
- **High**: Viewer role restricted to read-only node methods (was granting sign/export access)
- **High**: Backup restore/verify now validates IDs against path traversal
- **High**: Tar archive extraction validates every entry path (prevents tar slip attacks)
- **High**: S3 backup endpoints require HTTPS and reject private IP ranges
- **Medium**: Remember-me token secret now uses cryptographic random (not machine-id)
- **Medium**: Destructive operations (factory reset, onboarding reset) now require password re-verification
- **Medium**: Session token rotated after TOTP verification (prevents interception reuse)
- **Medium**: Webhook URL validation hardened against IPv6 bypass, DNS rebinding, redirect chains
- **Low**: CORS localhost:8100 only included in dev mode
- **Low**: CSP `unsafe-inline` removed from `script-src`
- **Low**: Content filenames validated against path separators and hidden file prefixes
- **Low**: Nostr relay URLs restricted to `wss://` with private IP rejection
- **Low**: Onion address validation enforces v3 format (56 base32 chars)
- **Low**: Router detection restricted to private IP ranges only

#### Nginx Authentication
- Fixed session cookie name mismatch (`session_id` → `session`) across all nginx auth checks
- LND Connect info endpoint now properly authenticated

### Container Reliability

#### Memory Limits (prevents OOM crashes)
- All 37 containers in `first-boot-containers.sh` now have `--memory=` limits
- Automatic RAM tier detection — reduced limits on 8GB machines
- Prevents a single runaway container from crashing the entire system

#### Smart Container States
- New `exited` state distinguishes crashed containers from intentionally stopped ones
- Crashed containers show red "crashed" badge with restart button
- Health-aware status: "healthy" (green), "starting up" (yellow spinner), "unhealthy" (orange pulse)
- Restart button added next to Stop on running containers

#### Crash Recovery Improvements
- Boot recovery and health monitor now coordinate via shared flag (no more restart cascade)
- User-stopped containers tracked in `user-stopped.json` — survive reboots without auto-restart
- Boot recovery uses tiered ordering: databases → core → services → apps → UIs
- Health monitor waits for boot recovery to complete before starting checks

### UI Improvements

#### Home Dashboard
- Wallet card now matches Web5 wallet display
- New Transactions modal with full history (incoming/outgoing, amounts, confirmations)
- Transactions button in header — switches to "Incoming" badge when pending transactions exist
- Dev faucet button (dev mode only) with mutable wallet state
- Fixed system stats crash (`cpu_usage_percent` field name mismatch)

#### Apps & App Details
- Container restart button (icon) next to Stop on all running apps
- Exited/crashed containers show "Restart" instead of "Start" with red styling
- Removed broken sticky header from Apps page
- Health-aware status badges throughout

#### Mesh, Cloud, Settings & More
- Mesh view overhaul with improved layout
- Glass button styling updates across components
- New BaseModal and ToggleSwitch components
- Updated translations (English + Spanish)
- Spotlight search improvements

### Infrastructure

#### LND Connect
- Tor hidden service now exposes LND REST port (8080) for remote wallet connections
- Fixed in ISO build script, deploy script, and live servers

#### Dev Environment
- Mock backend has mutable wallet state (faucet/send/receive actually change balances)
- Testnet stack option auto-starts Podman machine on macOS
- Boot mode simulation for testing startup screens

## [1.2.0] - 2026-03-14

### Fixed

#### Crash Loop Resolution
- Identified and fixed UFW blocking Podman subnet DNS resolution on .228
- Fixed archy-nbxplorer, btcpay-server, mempool-web, immich crash loops (3500+ restarts)
- All 32 containers stable with zero crash loops after fix

#### DWN Sync Performance
- Made `dwn.sync` endpoint non-blocking (background task with polling)
- Added 90-second overall sync timeout to prevent indefinite blocking
- Deduplicated peer onion addresses before syncing
- Batched message pushes (50/batch) instead of one-at-a-time over Tor
- Fixed HTTP handler to process all messages in batch (was only first)

#### Backup Reliability
- Increased backup.create rate limit from 3/600 to 10/600 for testing
- Increased backup.restore rate limit from 2/600 to 5/600

#### Deploy Script
- Added `set -eo pipefail` for pipe error detection
- Fixed duplicate variable initialization
- Fail on missing binary in --both path (was silently ignored)
- Added post-deploy health check on .198

### Added

#### Cross-Node Test Suite
- US-08: DWN sync tests — 50/50 pass (register, write, sync, query bidirectional)
- US-10: Backup/restore tests — 80/80 pass (create, list, verify, delete × 10 × 2 nodes)
- US-15: Boot recovery tests — .228 9/9 pass (32/32 containers survive 3 reboots)
- `trigger_sync_and_wait()` helper for polling async DWN sync

#### did:dht Integration Planning
- Architecture document: `docs/did-dht-integration.md`
- BEP-44 mutable DHT items, DNS packet encoding, z-base-32 identifiers
- Publication/resolution flows, `mainline` crate selection, security notes

#### DWN Protocol Definitions
- 4 Archipelago DWN protocols documented in `docs/dwn-protocols.md`
- Node Identity Announcements (public)
- File Sharing Catalog (public)
- Federation State (private)
- App Deployment Requests (private)
- Auto-registration of all 4 protocols on backend startup

#### Deploy Script Improvements
- `--dry-run` flag shows what would be deployed without executing
- Works with all other flags (--live, --both, --frontend-only)

#### ISO/First-Boot Improvements
- Auto-create swap file on first boot (50% RAM, min 2GB, max 8GB)
- Tiered container startup ordering in first-boot script
- Tier 1: Databases, Tier 2: Core Services (5s delay), Tier 3: Applications (5s delay)

### Security

#### Backend Hardening
- Rate limiting on federation endpoints (join 5/60s, invite 10/300s)
- DWN message data size limit (10MB max)
- Container security: cap-drop ALL, no-new-privileges, per-app memory limits
- Input validation: path traversal protection on identity/DID endpoints
- Error sanitization: internal paths stripped from error messages

## [1.1.0] - 2026-03-13

### Added

#### Nostr Identity in Onboarding
- Auto-generate secp256k1 Nostr keypair during identity creation
- Onboarding shows both DID (`did:key:z...`) and Nostr ID (`npub1...`) with copy buttons
- Real Ed25519 signature verification in onboarding verify step
- Real encrypted backup creation in onboarding backup step

#### NIP-07 Iframe Signing
- `nostr-provider.js` injected into all proxied iframe apps via nginx `sub_filter`
- `window.nostr` interface: `getPublicKey()`, `signEvent()`, `getRelays()`
- Signing consent modal with "Remember for this app" option
- `node.nostr-sign` RPC endpoint — signs events with node-level Nostr key
- NIP-04 and NIP-44 encrypt/decrypt RPC endpoints for iframe apps
- noStrudel Nostr client added to marketplace as iframe app

#### File Sharing Across Nodes
- Content catalog with add/remove/browse over Tor
- Three access modes: `free`, `peers_only` (DID-authenticated), `paid` (cashu tokens)
- Availability controls: `AllPeers`, `Nobody`, `Specific` (DID allowlist)
- Peer Files view in Cloud page for browsing federated peers' shared content
- Content download from peers via Tor SOCKS proxy

#### DWN Multi-Node Sync
- Bidirectional DWN message replication over Tor between federated nodes
- Protocol and message sync via `/dwn` HTTP endpoint
- DWN sync status in Federation dashboard with "Sync Now" button
- DWN management section in Web5 page (protocols, messages, sync targets)

#### Node Visualization Map
- D3.js force-directed network topology graph
- Nodes colored by trust level (green/amber/red), opacity by online status
- Self node centered, draggable peer nodes with tooltips
- List/Map tab switcher in Federation page with localStorage persistence

#### Tor Address Rotation
- `tor.rotate-service` RPC: generates new .onion address with 24h transition
- Automatic propagation to Nostr relays and federation peers
- `tor.cleanup-rotated` for expired transition directories
- Per-app Tor toggle (`tor.toggle-app`) to enable/disable Tor per service
- Tor management UI in Settings with rotate button and per-app toggles

#### Boot Container Recovery
- All stopped containers automatically started on backend boot
- Fixes clean reboot scenario where PID marker was removed by systemd

#### Monitoring & Testing
- Federation health check script (cron every 5min, CSV + JSON output)
- Uptime monitor with authenticated RPC access
- `test-first-install.sh` — 8-check post-install verification
- `test-nip07.sh` — 11-check NIP-07 signing validation
- `test-tor-rotation.sh` — 10-check Tor rotation lifecycle
- `test-integration-full.sh` — 23-check full integration test
- `test-failure-recovery.sh` — 5-scenario failure injection + recovery

### Fixed
- Health monitor webhook gate no longer blocks auto-restart and notifications
- Monitoring alerts now trigger webhook delivery (DiskWarning, ContainerCrash)
- Tor hostname reading with `tor-hostnames` readable cache (0700 system Tor dirs)
- Tor rotation clears hostname cache before reading new address
- Rotation restarts system Tor (not just archy-tor container)
- NIP-07 signing uses node-level key (matches `getPublicKey()`)
- DWN sync URL uses port 80 (nginx/Tor) instead of 5678
- DWN `/dwn` POST endpoint allows unauthenticated peer sync
- DWN message handler supports both single and batch message formats

## [0.8.0-rc1] - 2026-03-11

### Added

#### W3C Identity & Credentials
- W3C DID Core v1.0 compliant DID Document generation (`did:key` method)
- DID Document verification and cross-node resolution over Tor
- JSON-LD Verifiable Credentials (VC Data Model 2.0, Ed25519Signature2020 proofs)
- Verifiable Presentation creation with selective disclosure
- Credentials management UI at `/dashboard/web5/credentials`

#### Decentralized Web Node (DWN)
- DWN message store with CRUD, protocol registration, and query interface
- DWN HTTP API (`POST /dwn`, `GET /dwn/health`)
- Bidirectional peer sync over Tor via SOCKS proxy
- DWN management UI in Web5 page with protocol browser

#### Multi-Node Federation
- Node federation protocol with invite codes (`fed1:` prefix), trust levels, state sync
- Federation dashboard at `/dashboard/server/federation`
- Federated app deployment to trusted peers over Tor
- Architecture documented in `docs/multi-node-architecture.md`

#### Decentralized Marketplace
- NIP-78 Nostr-based app manifest discovery across relays
- Trust scoring (0-100) based on DID verification, relay consensus, federation trust
- App manifest publishing with Nostr secp256k1 signing
- Community marketplace tab in App Store with trust score badges

#### Networking
- VPN integration (Tailscale + WireGuard) with keypair generation and status display
- Mesh networking via Meshtastic LoRa devices with node discovery
- DNS-over-HTTPS configuration (Cloudflare, Google, Quad9, Mullvad, Custom)
- WiFi/Ethernet configuration via `nmcli` with scan-and-connect modal
- Network interfaces display in Server page

#### Hardware Wallet Support
- PSBT signing flow (create, QR display, finalize, broadcast)
- USB hardware wallet detection (ColdCard, Trezor, Ledger)
- Hardware wallet signing UI in LND views

#### System Management
- System monitoring (CPU, RAM, disk gauges on Dashboard)
- Automatic update system with download, apply, rollback, and scheduling
- Disk space management with auto-cleanup at 90% usage
- Container health monitoring with auto-recovery (max 3 restart attempts)
- Crash recovery via PID-file detection and container snapshot restoration
- Graceful shutdown with in-flight request draining (5s timeout)

#### Backup & Restore
- Full backup with tar.gz + ChaCha20-Poly1305 encryption
- Backup create, list, verify, restore, delete via RPC
- USB drive detection and backup-to-USB
- Backup UI in Settings page

#### Kiosk Mode
- Chromium kiosk with auto-restart and watchdog service
- Recovery page at `/recovery` (no auth required)
- Kiosk keyboard shortcuts (Ctrl+Shift+R/H/Q)
- Systemd services for kiosk and watchdog

#### ARM64 Support
- Cross-compilation for aarch64 with rustls-tls
- All 6 core apps verified with multi-arch images
- Parameterized ISO build script (`ARCH=arm64`)
- RPi 5 testing guide

#### Testing
- 236 frontend tests across 17 test files (Vitest)
- 124+ backend tests (cargo test)
- Playwright visual regression suite (12 pages)
- Chaos testing (SIGKILL recovery, concurrent RPC, rapid restarts)
- App lifecycle testing and dependency chain verification
- 1-week continuous uptime monitoring

#### Documentation
- Developer guide, API reference (100+ endpoints), app developer SDK guide
- 5 Architecture Decision Records (Podman, DID:key, Nostr, Tor, ChaCha20)
- Release process, canary deploy, quality baseline documentation

### Changed
- Settings sections use `glass-card` instead of `path-option-card`
- Web3 card shows "Coming Soon" badges instead of fake data
- Network diagnostics moved from Settings to Server page
- Removed `core/startos/` (2MB of dead code, zero dependencies)

### Fixed
- CSRF protection on all state-changing RPC calls
- CORS restricted to same-origin (removed `Access-Control-Allow-Origin: *`)
- Nginx security headers (X-Frame-Options, CSP, X-Content-Type-Options)
- All 24 silent catch blocks now log in dev mode
- Zero `console.log` outside dev gate, zero `any` types

### Security
- CSRF token validation on all state-changing endpoints
- Same-origin CORS policy
- Nginx security headers (SAMEORIGIN, nosniff, CSP, Referrer-Policy)
- Container security hardened (readonly root, dropped caps, non-root, no-new-privileges)
- Secrets rotation with AES-256-GCM and automatic scheduling

## [0.5.0-beta] - 2026-03-11

### Added

#### Security Hardening
- Session inactivity expiry (24h), max 5 concurrent sessions with oldest eviction
- Session rotation on password change (invalidates all other sessions)
- Container security: `--cap-drop=ALL`, `--security-opt=no-new-privileges:true`, read-only root
- Secrets rotation with AES-256-GCM encryption and metadata tracking
- Path traversal prevention (nginx regex blocks + client-side sanitizePath)
- Cookie-based auth for File Browser (removed token from URLs)
- Login rate limiting (5 failures per 60s per IP)
- TOTP two-factor authentication with backup codes

#### Performance
- Backend startup: ~100ms
- Frontend bundle: ~105 KB gzipped initial load
- WebSocket heartbeat (30s ping/pong) with exponential backoff reconnection
- Server-side 5-minute inactivity timeout for stale WebSocket connections
- Real-time install progress reporting via WebSocket during container pulls
- Connection state machine (connecting/connected/disconnecting/disconnected)

#### Apps & Integrations
- Pinned all container images to specific versions (no `:latest` tags)
- Fedimint and Fedimint Gateway with auto-LND detection
- IndeedHub virtual app integration
- Expanded read-only root filesystem support (electrs, nostr-relay, ollama)
- Dependency chain validation (Bitcoin → Electrs → Mempool, Bitcoin → LND)

#### Documentation
- Comprehensive user guide (docs/user-guide.md)
- Beta release checklist (docs/BETA-RELEASE-CHECKLIST.md)
- 72-hour stability test script

### Fixed
- Penpot hardcoded secret key replaced with SHA256-derived key
- WebSocket reconnection reliability after network interruption

## [0.1.0] - 2026-01-28

### 🎉 Initial Release

The first production release of Archipelago - a next-generation Bitcoin Node OS for macOS.

### Added

#### Core Features
- **Native Rust Backend** - High-performance async server using Tokio and Hyper
- **Modern Vue.js Frontend** - Beautiful glassmorphism UI with Tailwind CSS
- **Docker Integration** - Seamless container orchestration via Docker Desktop
- **Real-time WebSocket** - Live updates for container status and system events
- **Authentication System** - Secure user login and session management

#### Bitcoin & Lightning
- **Bitcoin Core** - Full node in regtest mode with custom UI
- **LND** - Lightning Network Daemon with dedicated interface
- **BTCPay Server** - Bitcoin payment processing
- **Mempool Explorer** - Blockchain visualization and analytics

#### Applications
- **Penpot** - Open-source design and prototyping platform
- **Endurain** - Self-hosted fitness tracking
- **Morphos** - File conversion utility
- **Nextcloud** - Cloud storage and file management
- **Home Assistant** - Home automation hub
- **Grafana** - Metrics and monitoring dashboards
- **OnlyOffice** - Document editing suite
- **SearXNG** - Privacy-respecting search engine
- **Fedimint** - Federated e-cash system

#### User Interface
- **Onboarding Flow** - Guided setup for new users
- **Dashboard** - Real-time system overview
- **My Apps** - Alphabetically sorted app management
- **Cloud Interface** - File management by type (Documents, Photos, Videos, Music)
- **Web5 Explorer** - Decentralized identity and data management
- **Settings** - System configuration and preferences
- **Custom Launch Pages** - Dedicated UIs for Bitcoin Core and LND

#### Technical Features
- **Container Runtime Abstraction** - Support for Docker and Podman
- **Dynamic Package Discovery** - Automatic detection of running containers
- **Health Monitoring** - Container status and health checks
- **Data Persistence** - Docker volumes for app data
- **Network Isolation** - Secure container networking
- **Resource Management** - CPU and memory allocation

### Architecture

- **Backend**: Rust + Tokio + Hyper + WebSocket
- **Frontend**: Vue 3 + TypeScript + Vite + Pinia
- **Styling**: Tailwind CSS + Custom Glassmorphism
- **Containers**: Docker Compose + Dockerode API
- **Build System**: Cargo + npm + macOS App Bundle

### Known Limitations

- Requires Docker Desktop (23.0+)
- macOS only (Intel and Apple Silicon)
- Single-user mode
- No auto-updates (manual download required)
- Ollama excluded due to image size
- Manual Docker container management

### System Requirements

- macOS 10.15 (Catalina) or later
- 8GB RAM minimum (16GB recommended)
- 20GB free disk space (50GB+ for blockchain data)
- Docker Desktop 23.0 or later
- Internet connection for initial container downloads

### Installation

1. Download `Archipelago-0.1.0-macOS.dmg`
2. Open the DMG and drag Archipelago to Applications
3. Install Docker Desktop if not already installed
4. Launch Archipelago from Applications
5. Access the UI at http://localhost:8100

### Security

- **Code Signed**: Yes (Developer ID)
- **Notarized**: Yes (Apple notarization)
- **Sandboxed**: No (requires full disk access for Docker)
- **Hardened Runtime**: Yes
- **Gatekeeper**: Compatible

### Documentation

- README.md - Project overview
- BUILD_MACOS.md - Build instructions
- DEPLOYMENT_CHECKLIST.md - Release process
- docs/ - Detailed documentation

### Credits

Built with:
- Rust (backend)
- Vue.js (frontend)
- Docker (containers)
- Alpine Linux (inspiration)
- Parmanode (Bitcoin scripts)
- And many open-source dependencies

### License

[Specify your license here]

---

## Version History

### 0.1.0 - 2026-01-28
Initial public release

---

## Future Roadmap

See GitHub Issues for planned features:
- [ ] Auto-update system
- [ ] Multi-user support
- [ ] Native container runtime (no Docker Desktop)
- [ ] iOS companion app
- [ ] Hardware wallet integration
- [ ] Tor integration
- [ ] VPN/Tailscale support
- [ ] Backup/restore functionality
- [ ] Mac App Store distribution
- [ ] Windows and Linux builds

## Contributing

See CONTRIBUTING.md for development setup and guidelines.

## Support

- GitHub Issues: Report bugs and request features
- Documentation: See `/docs` directory
- Community: [Discord/Telegram/Forum link]
