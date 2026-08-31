# SESSION — companion 0.5.28: shipped, published, playbook (2026-08-31)

**For: the companion agent (next session) + anyone shipping a companion
release.** Session that closed the 2026-08-30 companion-agent queue (#61
residual, #128, #139) and shipped 0.5.28 end-to-end.

## Release state at session end — ALL LIVE

Companion **0.5.28 / versionCode 48**, main @ PR #149 (`9f1a289d`), deploy
handoff merged as PR #150 (`91374392`). Every public surface verified
byte-identical (`shasum -a 256` = `fc786b46c704c5752f04fe603371365524c749734f17bd8858cf02fa2dbc34ca`):

| Surface | URL | State |
|---|---|---|
| Gitea raw-on-main | `http://146.59.87.168:3000/lfg2025/archy/raw/branch/main/neode-ui/public/packages/archipelago-companion.apk` | ✅ 0.5.28, v1+v2+v3 verified on download |
| Foundation static `/packages/` (real-node QR URL) | `https://source.archipelago-foundation.org/packages/archipelago-companion.apk` | ✅ 0.5.28 |
| Foundation Gitea-raw proxy | `…/lfg2025/archy/raw/branch/main/…` | ✅ 0.5.28 (6h cache — may lag after pushes) |
| Demo `:2100` | `http://146.59.87.168:2100/packages/archipelago-companion.apk` | ✅ 0.5.28 (auto: CI + Portainer webhook) |

Only remaining live-surface step: **node web-bundle redeploys** so each
node's own served copy is 0.5.28 — archi-dev-box's standard step, written up
in `docs/HANDOFF-2026-08-31-companion-0.5.28-deploy.md` (its §1/§3 were
already done by the time of this doc — only §2 outstanding).

Tracker: #128 and #139 closed with what-shipped comments; #61 (already
closed) got a residual-fix follow-up. Signing cert unchanged (`d622e07e…`),
so phones update in place.

## What shipped in 0.5.28 (map)

- **#61 residual (web)**: `isCompanionApp()` gates on `CompanionBanner.vue`
  render, `openCompanionIntro()` (useCompanionIntro.ts), and the overlay's
  manual-open watcher; overlay moved to the canonical helper. Vitest suite
  green (1013 tests).
- **#128 Backup & Restore**: `Android/rust/archy-fips-core/src/backup.rs`
  (ADR-005 envelope, node-compatible), `BackupManager.kt`, hub sub-page
  `ui/components/BackupSection.kt`. Doc: `companion-backup-restore.md`.
- **#139 Remote Signer**: `src/nostr.rs` (NIP-44 v2 + NIP-04 + BIP-340,
  official vectors), `nostr/BunkerManager.kt` + `NostrSignerPreferences.kt`,
  hub sub-page `ui/components/SignerSection.kt`, `nostrconnect://` deep link
  via `SignerLaunch`. Harness: `Android/tools/nip46-test-client.py`.
  Doc: `companion-nip46-remote-signer.md`.
- **Hub modal redesign** (field feedback): both features are sub-pages like
  Nodes/FIPS; panel height cap 70%; scanner hosted by NESMenu outside the
  panel; back-arrow → hub.
- **Extras**: node mesh ULA shown/copyable in the Nodes list (`MenuItem`
  subtitle); `Android/tools/fipssh` (npub→ULA is pure: `fd ‖ sha256(pubkey)[0..15]`,
  pinned by `npub_derives_the_same_mesh_ula_as_the_fips_identity` test).
- **Node-side handoffs written**: `HANDOFF-2026-08-31-ssh-over-mesh.md`
  (SSH-over-mesh toggle) and the 0.5.28 deploy handoff.

## The deployment playbook (learned the hard way this session)

### Networking — everything goes through the Tor SOCKS proxy

Direct connections to `146.59.87.168` fail from this box ("Bad file
descriptor"); git works because `~/.gitconfig` sets
`proxy = socks5h://127.0.0.1:9050`. **For curl/Gitea API you must pass it
explicitly:**

```bash
curl -s --socks5-hostname 127.0.0.1:9050 ...   # works
curl -s ...                                    # HTTP 000, "unreachable"
```

This is why earlier sessions concluded "Gitea API unreachable" — wrong; it
just needs the proxy flag.

### Gitea API + auth

- Base: `http://146.59.87.168:3000/api/v1` (v1.27.1), via the proxy.
- The keychain git credential (`security find-internet-password -s
  146.59.87.168`, acct `v4v`) is a **`write:repository`-only token** — fine
  for git, CANNOT read/write issues.
- Issue ops need `write:issue`. This session the user pasted a broad token
  (activitypub+misc+notification+organization+package+issue+repository) —
  **revocation still pending** (it's in chat scrollback). Ask the user for a
  scoped `write:issue` token next time.

### main is PROTECTED — ship via -ship branch + PR + API merge

`git push origin main` is rejected by pre-receive. The working sequence:

```bash
git fetch origin
git checkout main && git reset --hard origin/main        # local main is STALE (see below)
git merge --no-ff companion/<ver> -m "Companion <ver> — …"
./Android/ship-companion.sh                               # builds, signs v1+v2+v3, stages APK+meta, commits
# its `git push` FAILS on protected main — expected. Push the branch instead:
git push origin main:companion/<ver>-ship
# then create + merge the PR via API:
curl ... POST repos/lfg2025/archy/pulls        {"head":"companion/<ver>-ship","base":"main",...}
curl ... POST repos/lfg2025/archy/pulls/<n>/merge  -d '{"Do":"merge"}'
```

(Refinement for next time: run `ship-companion.sh` ON the `-ship` branch
from the start — it pushes the current branch, which for a `-ship` branch
succeeds directly.)

- **Local `main` is the pre-open-source-import lineage** (1115 stale
  commits, unrelated history). Always `reset --hard origin/main` before
  using it; never merge into it without the reset.
- A **stale tag ref** (`v1.7.115-alpha`) can make `git fetch` fail
  ("did not send all necessary objects") — `rm .git/refs/tags/v1.7.115-alpha`.
- Last release's `-ship` branch for reference: `origin/companion/0.5.27-ship`.

### Build + verify (per release)

- Version lives in `Android/app/build.gradle.kts` (`versionCode` must
  strictly increase; meta json is auto-generated by the publish script from
  it). 0.5.28 → next is **0.5.29/vc49**.
- APK package is `com.archipelago.app.debug` (the served artifact IS the
  debug build, committed repo keystore, cert SHA-256 `d622e07e…ec2664d`).
  Local `Android/app/debug.keystore` is untracked but produces that cert —
  verify per release: `apksigner verify --print-certs` on old vs new.
- Build: `cd Android && JAVA_HOME=/opt/homebrew/opt/openjdk@17
  ANDROID_HOME=$HOME/Library/Android/sdk ./gradlew :app:assembleDebug`
  (builds the Rust via cargo-ndk, NDK under `~/Library/Android/sdk/ndk/`).
  Test build for the user: copy to `~/Desktop/archipelago-companion-<ver>.apk`.
- Rust: `cd Android/rust/archy-fips-core && cargo test --lib` (24 tests at
  session end) + clippy. neode-ui: `npm ci` first (node_modules not kept),
  `npx vitest run`, `npm run type-check`.
- Post-ship verify block: aapt2 badging, shasum vs Gitea raw, apksigner
  v1/v2/v3, the three public URLs' meta json (table above), foundation
  raw-proxy may serve up to 6h stale (cache-control: max-age=21600).

### Infrastructure facts

- `source.archipelago-foundation.org` = openresty on vps2 with **two
  surfaces**: static `/packages/` (manual mirror; the real-node QR URL) and
  a Gitea-raw proxy (6h cache, auto). Demo `:2100` redeploys automatically:
  `.gitea/workflows/demo-images.yml` fires on `main` pushes touching
  `neode-ui/**`, then calls the Portainer webhook.
- **No SSH to vps2 from this box**: `archy_146_release` key declined for
  root/archipelago/dorian/lfg2025/deploy/git. Server-side work needs the
  archi-dev-box agent or the user.

## Open items for next session

1. **Confirm node web-bundle redeploys** happened (archi-dev-box; deploy
   handoff §2) — a paired node's own `/packages/` should serve vc48.
2. **Token revocation** (user) + request a `write:issue`-scoped one.
3. **Node-side roadmap** fed by this release: SSH-over-mesh toggle
   (`HANDOFF-2026-08-31-ssh-over-mesh.md`), node NIP-46 client (login flow B),
   node-side storage for companion backup envelopes.
4. **On-device follow-ups**: the user's full 0.5.28 pass — signer e2e via
   the harness (`/tmp/nip46env/bin/python Android/tools/nip46-test-client.py`),
   backup round-trip on a wipe, and the zxing-cpp decision trigger
   (move-to-the-code; sketch is verified online:
   `io.github.zxing-cpp:android:3.1.1`, still NOT-actioned by design).
5. Untracked on this box, deliberately left: `Android/app/debug.keystore`,
   `docs/1.8-alpha-improvements-tracker.md`,
   `docs/SESSION-1.8.0-OTA-PROGRESS.md`, `image-recipe/branding/source-logos/`
   (other workstreams' files).
