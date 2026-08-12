# Companion App — Build, Ship & "App Not Installed" Runbook

Canonical procedure for releasing the Archipelago Companion Android app and for
debugging install failures. Read this before touching the companion release flow.
Hard lessons from 2026-06-26 are baked in below — don't relearn them.

## Ship the companion (the only sanctioned way)

```bash
./Android/ship-companion.sh
```

This calls `scripts/publish-companion-apk.sh` (the single source of truth, also
used by the `.githooks/pre-push` hook), which:

1. **Removes/rejects resource dirs whose names contain spaces.** Empty stray
   `mipmap-* NNN` dirs (left by icon-export tools) break a *clean* build with
   `Invalid resource directory name`. Incremental builds hide them — clean builds
   don't.
2. **Always does a CLEAN build** (`:app:clean :app:assembleDebug`).
3. **Forces v1 + v2 + v3 signing** via `zipalign` + `apksigner`.
4. **Verifies all three schemes** (`apksigner verify --min-sdk-version 21`) and
   **aborts** if any is missing.
5. Stages the signed APK at `neode-ui/public/packages/archipelago-companion.apk`,
   commits, and pushes with `SHIP_COMPANION=1` (the sanctioned pre-push bypass).
6. The first-launch companion modal and Android "Share this app" QR point at
   `http://146.59.87.168:2100/packages/archipelago-companion.apk`. After the
   repo artifact is built, mirror that exact APK to the VPS2-served path before
   calling the release done.

**Never** hand-roll `gradlew assembleDebug` + `cp` to the served path. That path
skips the clean build and the signature enforcement and is exactly how a broken
APK shipped.

### Bump the version first
Edit `Android/app/build.gradle.kts` — `versionCode` (must strictly increase) and
`versionName`. The committed value can drift AHEAD of what's actually built into
the served APK, so verify the served APK's real version after shipping:
`aapt2 dump badging neode-ui/public/packages/archipelago-companion.apk | grep version`.

## Signing facts (important)

- Debug builds are signed with the **committed** `Android/app/debug.keystore`
  (store/key pass `android`, alias `androiddebugkey`) so every machine and the
  served download share ONE signing key. Cert SHA-256: `D6:22:E0:7E:…:66:4D`.
- **AGP silently ignores `enableV1Signing = true` for `minSdk ≥ 24`**, so a plain
  gradle build produces a **v2-only** APK. The `apksigner` step in the publish
  script is what actually guarantees v1+v2+v3 — do not remove it.
- **Changing the signing key forces every existing install to be uninstalled
  once.** Android blocks in-place upgrades across different signatures. Treat the
  keystore as permanent; never regenerate it casually.

## Debugging "App Not Installed" — DIAGNOSE FIRST

Do **not** theorize about signing schemes / OEM quirks. Get the real reason:

```bash
adb install ~/Desktop/archipelago-companion-<ver>.apk
# -> Failure [INSTALL_FAILED_<REASON>: ...]
```

Map the reason:

| `INSTALL_FAILED_*` | Cause | Fix |
|---|---|---|
| `UPDATE_INCOMPATIBLE … signatures do not match` | Old install signed with a **different key** (e.g. pre-shared-keystore per-machine key `58:31:12…`). | Uninstall the old package, then install. **One-time** per device after a key change. |
| `INVALID_APK` / parse error | Corrupt/incomplete download or bad signing. | Re-download; re-run the publish script. |
| `INSUFFICIENT_STORAGE` | Storage. | Free space. |
| `OLDER_SDK` | Device below `minSdk` (26 = Android 8.0). | Unsupported device. |

> A manual uninstall on the phone may NOT clear `UPDATE_INCOMPATIBLE` if the
> package is registered under another user/profile — `pm path <pkg>` under user 0
> can show nothing while the conflict persists. `adb uninstall <pkg>` clears it
> across all users.

## Phone / adb safety (non-negotiable)

When acting on the user's physical phone, be surgical — the user once had all
home-screen app layouts wiped by an over-broad action.

- Default to **read-only** adb (`devices`, `getprop`, `pm path/list`, `dumpsys`).
- Mutations (`adb install`, `adb uninstall com.archipelago.app.debug`) only with
  explicit go-ahead and **scoped to our exact package** — echo it first.
- **Never** run launcher/system resets: no `pm clear` on launchers, no
  `reset-permissions`, no factory wipe, no uninstalling apps you didn't build.

## Verify the published download after shipping

The checked-in artifact is Gitea raw-on-main. The QR/App Store download served
to users is the VPS2 `:2100` URL. Confirm both live byte streams match what you
built and signed:

```bash
SERVED=neode-ui/public/packages/archipelago-companion.apk
GITEA_URL=https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/$SERVED
QR_URL=http://146.59.87.168:2100/packages/archipelago-companion.apk
curl -sS -o /tmp/live-gitea.apk "$GITEA_URL"
curl -sS -o /tmp/live-qr.apk "$QR_URL"
shasum -a 256 "$SERVED" /tmp/live-gitea.apk /tmp/live-qr.apk  # all must match
apksigner verify -v --min-sdk-version 21 /tmp/live-qr.apk | grep -i "scheme"  # v1/v2/v3 = true
```
