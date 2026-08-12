# License Compliance Audit — Open-Source Release

Audit date: 2026-07-22. Scope: entire repo (core Rust workspace, neode-ui, apps/*, Android companion, image-recipe ISO, docker/, app-catalog, reticulum-daemon, demo/) plus the external FIPS source and registry-mirrored images.

**Verdict (as of the 2026-07-22 audit):** the dependency graph is almost entirely permissive (MIT/Apache/BSD) and compatible with a free open-source release. But the repo was not releasable as-is: it had **no license of its own**, one **LGPL Rust dependency**, several **non-redistributable committed assets** (proprietary fonts, unknown-rights media), and **missing attribution machinery**. Everything below is ordered by severity.

> **Updated 2026-08-08.** §1 (no license) and §3 (non-redistributable committed
> files) are now **closed** — root `LICENSE` (MIT) + `NOTICE` are in the tree, and
> the proprietary fonts and unused packages have actually been deleted. **§2
> (`zbase32`, LGPL-3.0+) is now closed too** — replaced by an in-tree
> implementation. No copyleft dependency remains in the Rust graph.

---

## STATUS UPDATE — 2026-07-23

**DONE:**
- MIT adopted. Root `LICENSE` + `NOTICE` added; `license = "MIT"` in all 5 workspace crates (archy-fips-core already had it); `"license": "MIT"` (+ `"private": true`) in all 4 package.json files.
- Deleted: `Courier_New/`, `Benton_Sans/`, `Redacted/` fonts; `wireguard.apk`;
  `atob.s9pk`.

  **History note (2026-08-08):** this line originally claimed all of these plus
  `test-install.sh` were "git-rm'd" on 2026-07-23. They were not — only the
  `web/dist` copies had been removed, and all seven sources were still tracked at
  HEAD nearly three weeks later. The six listed above were actually deleted on
  2026-08-08 (`neode-ui/test-install.sh` was left; it is not a licensing
  concern). Kept as a reminder that a DONE entry here is a claim, not evidence —
  re-verify with:

  ```
  git ls-tree -r HEAD --name-only | grep -iE 'Courier_New|Benton_Sans|Redacted/|wireguard.apk|atob.s9pk'
  ```

  Deletion was safe: no `@font-face` rule ever referenced them (all four in the
  tree load Montserrat), the `Courier New` hits in `tailwind.config.js` and two
  public HTML files are `font-family` fallbacks naming the *system* font, and
  `wireguard.apk` / `atob.s9pk` had zero references anywhere. Montserrat (OFL.txt)
  and Open Sans (LICENSE.txt) remain, as does the actively-used
  `archipelago-companion.apk`. Removing the two packages also took ~40 MB off
  the frontend OTA tarball.
- Media provenance resolved: all demo music/photos/posters, UI sfx, backgrounds, and intro video are the author's original work — recorded in `demo/content/README.md` and `NOTICE`.
- Meshtastic device artwork attributed (`mesh-devices/ATTRIBUTION.md` + NOTICE); icon attribution added (`assets/icon/ATTRIBUTION.md`: game-icons.net CC BY 3.0, pixelarticons MIT).
- Reticulum decision: include + disclose (NOTICE states the Reticulum License restrictions and that it applies only to the optional daemon).
- indeedhub: deferred — partnership in place; license the submodule before/at public release.
- License inventories generated: `core/THIRD-PARTY-LICENSES.md` (649 crates) and `neode-ui/THIRD-PARTY-LICENSES.md` (runtime deps + fonts + vendored).

**REMAINING (code changes, awaiting review — see sections below for detail):**
1. ~~Replace `zbase32` (LGPL-3.0+) with `z32` or original impl~~ — **DONE 2026-08-08**, original impl (§2).
2. Swap `redis:7.4.8` → Valkey in `scripts/image-versions.sh` and deploys — §3.
3. Delete dead StartOS-derived crates `core/{js-engine,container-init,models,helpers}` — §4.
4. Attribution build integration: cargo-about in CI → ship full license texts in ISO; vite/rollup license plugin (or UI licenses page) for the web bundle; Android OSS-licenses screen — §5.
5. Release-checklist items: per-release Debian source pointer (snapshot.debian.org), catalog `license`/`sourceUrl` fields, restrict ISO image bundling to the audited list — §6.
6. ~~Before repo goes public: purge deleted fonts/APKs from git history (`git filter-repo`)~~ — **superseded**: the launch plan is a fresh-history publish, so there is no history to rewrite. What still applies is verifying the game-icons author credit, and actually deleting the files (see the correction above — they were never removed).

**Re-verified 2026-08-08:**
- ~~`zbase32 0.1.2` (LGPL-3.0+) is still a direct dependency.~~ **Removed 2026-08-08** — see §2.
- `LICENSE` (MIT) and `NOTICE` are present ✅. `core/THIRD-PARTY-LICENSES.md` and `neode-ui/THIRD-PARTY-LICENSES.md` are present ✅.
- The four StartOS-derived crates in item 3 (`core/{js-engine,container-init,models,helpers}`) **still exist** — note KEY-05 legitimately cites `core/models`, so that one needs a look before deletion rather than a blind `rm`.

---

## 1. BLOCKER — the project has no license  ✅ CLOSED

_Resolved: MIT adopted, root `LICENSE` + `NOTICE` present. Original finding below._

There is no `LICENSE`/`COPYING` file anywhere in the repo. No crate in `core/` declares a `license` field; none of the four `package.json` files do either (and the three `apps/*` packages aren't even `private: true`). Until fixed, the code is "all rights reserved" — publicly visible, but legally not open source and not usable by anyone.

**Do:**
- [ ] Choose a license. **Recommendation: MIT** — the Bitcoin-ecosystem norm (Bitcoin Core, LND are MIT), maximally compatible with everything found in the graph. (Alternatives: Apache-2.0 adds a patent grant; GPLv3 if copyleft is desired — nothing in the deps prevents any of these.)
- [ ] Add `LICENSE` at repo root with the year and copyright holder.
- [ ] Add `license = "MIT"` to all five workspace member `Cargo.toml`s (archipelago, container, openwrt, performance, security) and `Android/rust/archy-fips-core` (declares MIT but ships no license file — add one).
- [ ] Add `"license": "MIT"` to `neode-ui/package.json` and `apps/{morphos-server,router,did-wallet}/package.json`.

## 2. BLOCKER — copyleft dependency that must be replaced  ✅ CLOSED 2026-08-08

- [x] **`zbase32 0.1.2` — LGPL-3.0+** — was the only hard copyleft blocker in all 649 resolved Rust crates. Direct dep of `archipelago`, used in `core/archipelago/src/network/did_dht.rs` for did:dht z-base-32 encoding. LGPL statically linked into a Rust binary requires shipping relinkable objects/source — impractical.

  **DONE 2026-08-08.** Replaced with an original in-tree implementation at
  `core/archipelago/src/network/zbase32.rs` (~60 lines incl. docs) rather than
  the `z32` crate — the encoding is an alphabet substitution over a bit stream,
  so this removes the blocker without adding any dependency or new supply-chain
  surface. Dropped from `Cargo.toml` and `Cargo.lock`.

  Byte-compatibility was the hard requirement: a `did:dht` identifier *is* this
  encoding of an Ed25519 public key, so any drift would silently rotate every
  node's DID and orphan its published DHT records. The replacement is pinned
  against the removed crate's own three doc-test vectors, the canonical vectors
  from Zimmermann's z-base-32 spec, and four known 32-byte keys — plus a
  `did_for_a_known_key_is_stable` test at the `did_dht.rs` call site.

No GPL, AGPL, SSPL, or unlicensed crates exist anywhere else in the Rust graph. (`r-efi` and `self_cell` list LGPL/GPL only as options in OR-expressions — elect MIT/Apache, no action.)

## 3. BLOCKER — committed files we may not redistribute  ◐ fonts/packages CLOSED 2026-08-08; media + redis items still open

Remove from git (history purge is **moot** — the launch plan is a fresh-history publish, so past commits are not carried over):

- [x] `neode-ui/public/assets/fonts/Courier_New/` — Monotype proprietary font, no license, **unused in CSS**. ~~Delete.~~ **DELETED 2026-08-08.**
- [x] `neode-ui/public/assets/fonts/Benton_Sans/BentonSans-Regular.otf` — commercial Font Bureau typeface, no license, unused. ~~Delete.~~ **DELETED 2026-08-08.**
- [x] `neode-ui/public/packages/wireguard.apk` (17 MB) — **DELETED 2026-08-08.** — official WireGuard Android APK containing GPL-2.0 `libwg` components; redistribution triggers GPL source-offer. **Unreferenced since the FIPS migration** — delete.
- [x] `neode-ui/public/packages/atob.s9pk` (24 MB) — **DELETED 2026-08-08.** — Start9 service package, unknown license, referenced only by a test script. Delete.
- [ ] `demo/content/music/` (18 full tracks, ~150 MB) and `demo/peer-media/` (17 photos/book covers/film posters) — no recorded rights. If they're your own/AI-generated work, document that in a `demo/content/README`; otherwise remove.
- [ ] `neode-ui/public/assets/video/video-intro.mp4`, `Kratter.MP3`, photographic `bg-*.jpg` backgrounds, UI/arcade sound effects in `assets/audio/` — same: document provenance (user-made per project convention) or replace. `welcome-noderunner.mp3` is ElevenLabs TTS — their commercial-use terms allow this on paid plans; note it.
- [ ] **Registry: `redis:7.4.8`** (`scripts/image-versions.sh` `REDIS_IMAGE`) — Redis ≥ 7.4 is RSALv2/SSPLv1, **not open source**; re-hosting it on your registry is redistribution under a restricted license. **Switch to Valkey** (BSD-3, already mirrored) everywhere.

## 4. VERIFY — unknown/third-party provenance

- [ ] **`neode-ui/public/assets/img/mesh-devices/` (36 SVGs)** — almost certainly Meshtastic project device artwork (meshtastic/web is GPL-3.0). Confirm source; either replace with original art or comply with the upstream license + attribution.
- [ ] **`neode-ui/public/assets/icon/`** — `barbarian.svg`, `batteries.svg` match game-icons.net (**CC BY 3.0 — visible attribution required**); pixel-style icons match pixelarticons (MIT). Confirm and add attribution, or replace.
- [x] `Redacted/redacted.regular.ttf` — upstream is SIL OFL 1.1 but no license file is shipped. ~~Add `OFL.txt` or delete (unused).~~ **DELETED 2026-08-08** (unused; deleting was cheaper than sourcing the OFL text).
- [ ] **indeedhub** — submodule (private gitea) not checked out; no known license, yet `indeedhub{,-api,-ffmpeg}:1.0.0` images are distributed via registry/ISO. `indeedhub-ffmpeg` implies a bundled FFmpeg (LGPL/GPL → source-offer obligations). Must license the project and audit the ffmpeg build before public release.
- [ ] `minmoto/fmcd` v0.8.0 and `ark-bitcoin/bark` (barkd) — binaries redistributed in your images; verify upstream licenses (bark claims Apache-2.0/MIT dual) and include their notices.
- [ ] **Start9/StartOS heritage** — `core/{js-engine,container-init,models,helpers}` are StartOS-derived (embassy paths, s9pk handling). start-os is MIT → attribution required if kept. **Better: delete these four crates** — they are not workspace members, cannot compile (broken `../../patch-db` path dep), and carry an unpinned `yajrc = "*"` git dep on a moving branch. Deleting removes both the attribution question and dead code.
- [ ] **Reticulum (RNS 1.3.5 + LXMF)** — verified: custom "Reticulum License" — MIT-style **plus field-of-use restrictions** (no systems designed to harm humans; no AI/ML training-dataset use). Redistribution is permitted, so shipping the PyInstaller `archy-reticulum-daemon` binary is fine **if** the license text is included with it — but the OS cannot claim to be 100 % OSI-open-source while bundling it. Options: include + disclose (recommended, matches "plan for decentralization" honesty), or make the daemon an optional download.

## 5. REQUIRED — attribution / notice machinery (currently absent)

Nearly every permissive license (MIT/BSD/ISC/Apache) requires reproducing copyright + license text **in distributed binaries** — and right now every distribution channel strips them:

- [ ] **Rust binaries** (649 crates, ~85 % MIT/Apache dual): generate `THIRD-PARTY-LICENSES` with `cargo-about` (or `cargo-license`) in CI; ship it in the ISO at e.g. `/usr/share/doc/archipelago/`. Include **ring's three license files** (LICENSE, LICENSE-BoringSSL, LICENSE-other-bits) and note the system OpenSSL (Apache-2.0) linked via `ssh2`.
- [ ] **Web bundle**: Vite/esbuild strips all `@license` comments from `web/dist`. Add `rollup-plugin-license`/`vite-plugin-license` to emit a third-party attribution file, or add an "Open-source licenses" page in the UI. Runtime deps needing notices: vue/vue-router/pinia/vue-i18n (MIT), d3 (ISC), leaflet (BSD-2), dompurify (elect Apache-2.0 of its MPL/Apache dual), fuse.js (Apache-2.0), qrcode/qr-scanner/qrloop/buffer/fast-json-patch (MIT).
- [ ] **Android APK**: `packaging.excludes` strips `META-INF` license texts and there is no licenses screen. Add an OSS-licenses screen or bundled `licenses.txt` covering AndroidX/Compose/OkHttp/ZXing (Apache-2.0), **fips © 2026 Johnathan Corgan (MIT — the core of the VPN feature)**, tokio/tracing (MIT), subtle (BSD-3), tun (WTFPL — permissive, just list it), secp256k1 family (CC0). Generate the Rust side from the committed `Cargo.lock` with cargo-about.
- [ ] **AIUI demo bundle** (`demo/aiui/` — committed minified build): bundles Mermaid, Cytoscape, KaTeX, D3, Lodash, Workbox (all MIT/BSD). Add a `THIRD-PARTY-LICENSES` file next to it (or rebuild with a license plugin).
- [ ] Keep the intact MIT headers in the two vendored `qrcode.js` copies (docker/lnd-ui, docker/electrs-ui) — already compliant, don't minify them.
- [ ] Fonts kept: Montserrat (OFL.txt present ✓), Open Sans (Apache LICENSE.txt present ✓) — keep license files adjacent to the font files in dist.

## 6. REQUIRED — distribution-level obligations (ISO & registry)

The ISO redistributes a full Debian (trixie) system plus ~29 container image tarballs; the private registry re-hosts upstream images. Re-hosting = redistribution, same obligations as bundling.

- [ ] **GPL source offer for the ISO** — kernel, GRUB, busybox/live-boot, coreutils, nftables, cryptsetup, wireguard-tools, SYSLINUX `isohdpfx.bin`, etc. Easiest compliance: keep `/usr/share/doc/*/copyright` (the build already does ✓) **and** publish, per release, either a mirror of the exact Debian source packages (`apt-get source` snapshot / snapshot.debian.org pointer) or a written offer in the docs. Add this to the release checklist.
- [ ] **AGPLv3 images redistributed** (mempool, Grafana, Vaultwarden, SearXNG, PhotoPrism, Nextcloud, Immich, CryptPad, MinIO): AGPL compliance = make corresponding source available. You ship a **modified** mempool-frontend (`docker/mempool-frontend` entrypoint patch) — the patch is in-repo, so compliance is met once the repo is public; state this in docs. For unmodified images, link upstream sources in the app catalog.
- [ ] **GPLv2/GPLv3 images** (MariaDB, Jellyfin, AdGuard Home, strfry): unmodified redistribution → provide license text + upstream source links (a `license` + `sourceUrl` field per `app-catalog/catalog.json` entry solves this catalog-wide).
- [ ] **Non-free firmware** (firmware-realtek/iwlwifi/misc/linux-nonfree, intel/amd microcode): redistributable but proprietary — disclose in docs ("includes non-free firmware for hardware support"), like Debian's own non-free-firmware ISOs do.
- [ ] The ISO build's live-server image capture (`podman save` of whatever matches on the dev server) is a compliance hazard — bundle only from the audited image list.
- [ ] FIPS daemon (jmcorgan/fips v0.4.1, MIT ✓) and nostr-rs-relay binary (MIT ✓): include their license texts in the notices bundle.

## 7. Housekeeping (supports compliance)

- [ ] Add lockfiles + pinned versions in `apps/*` (currently floating `^` ranges, violating the project's own pinning rule) — reproducibility is also what makes license audits stay true.
- [ ] `Android` fips dep is pinned to a personal fork rev (`9qeklajc/fips-native@46494a74`) — mirror or vendor it so outside contributors can build.
- [ ] Move `@types/dompurify` to devDeps; refresh stale `neode-ui/node_modules`.
- [ ] Add a `NOTICE` file at root naming: fips (Johnathan Corgan, MIT), Start9 start-os (if any derived code remains), Kazuhiko Arase qrcode.js, font licenses, icon attributions.
- [ ] Consider CI license gating: `cargo-deny` (Rust) + `license-checker` (npm) with an allowlist, so new copyleft deps are caught at PR time.

---

## Quick reference: what's already clean

- All Rust crates: permissive or dual-licensed (`zbase32` was the sole exception and is gone as of 2026-08-08).
- All 833 npm packages in neode-ui: no GPL/AGPL anywhere; only dev-tool LGPL (sharp's libvips, never distributed).
- Android Gradle deps: 100 % Apache-2.0, all pinned, no Play Services/telemetry.
- FIPS mesh: MIT (© 2026 Johnathan Corgan) — keep notice.
- js-engine binds deno_core (MIT) as a crate, nothing vendored — moot if dead crates are deleted.
- reticulum-daemon Python is original code; obligations attach only to the PyInstaller binary (see §4).
- Bitcoin Core/Knots, LND, BTCPay, Electrs, Fedimint, core-lightning, Gitea, Home Assistant, Tailscale, Portainer, Uptime-Kuma, filebrowser, ollama, penpot: MIT/Apache/BSD/Zlib/MPL — link + notice is enough.
