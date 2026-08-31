# HANDOFF — deploy companion 0.5.28 (vc48) to the live surfaces

**For: the agent on archi-dev-box.** Companion 0.5.28 shipped to `main`
today (PR #149, merge `9f1a289d` — backup & restore #128, NIP-46 remote
signer #139, companion-gated install pitch #61 residual, hub sub-pages).
The dev box verified everything it can reach; three live surfaces remain,
same shape as the 2026-07-23 deploy handoff
([`HANDOFF-2026-07-23-companion-apk-deploy.md`](HANDOFF-2026-07-23-companion-apk-deploy.md)).

## Already done and verified (do not redo)

- `neode-ui/public/packages/archipelago-companion.apk` on `main` is
  **0.5.28 / versionCode 48**, clean build via `Android/ship-companion.sh`,
  **v1+v2+v3 signatures verified**, meta json refreshed beside it.
- Gitea raw-on-main serves it byte-identical:
  `shasum -a 256` = `fc786b46c704c5752f04fe603371365524c749734f17bd8858cf02fa2dbc34ca`
  (2 bytes: 28,206,999… file size ≈ 28.2 MB).
- The foundation server's **raw-proxy** path already serves 0.5.28 (verified
  via `https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/neode-ui/public/packages/archipelago-companion.json`).
- Demo CI (`demo-images.yml`) fired on the push and redeploys the stack via
  the Portainer webhook — should flip on its own; confirm only.
- Signing key unchanged (cert SHA-256 `d622e07e…ec2664d`), so phones update
  **in place** over any 0.5.27 install.

## 1. Foundation server static `/packages/` mirror — the real-node QR URL

`https://source.archipelago-foundation.org/packages/archipelago-companion.apk`
is a **static dir** on the release server (openresty; still 0.5.27,
last-modified 2026-08-17). This is the exact URL real nodes' companion QR
downloads (`DEFAULT_DOWNLOAD_URL` in `CompanionIntroOverlay.vue`) — it must
flip before the release is done.

```bash
# Find the webroot once:
grep -rl "packages" /etc/openresty /etc/nginx 2>/dev/null
find / -name archipelago-companion.apk -not -path '/proc/*' 2>/dev/null

# Mirror the exact bytes from Gitea raw-on-main (no rebuild, no re-sign):
cd <that webroot>
curl -fsS -o archipelago-companion.apk  http://146.59.87.168:3000/lfg2025/archy/raw/branch/main/neode-ui/public/packages/archipelago-companion.apk
curl -fsS -o archipelago-companion.json http://146.59.87.168:3000/lfg2025/archy/raw/branch/main/neode-ui/public/packages/archipelago-companion.json
shasum -a 256 archipelago-companion.apk
# MUST print: fc786b46c704c5752f04fe603371365524c749734f17bd8858cf02fa2dbc34ca
```

## 2. Node web-bundle redeploys

Same as 2026-07-23: redeploy the web-ui bundle from current `main` to the
active nodes — web root `/opt/archipelago/web-ui/` (NOT a `neode-ui/`
subfolder), at minimum every node the user pairs against. The APK rides in
the bundle's `packages/` dir, so this is also what makes each node's own
served QR download 0.5.28.

## 3. Confirm the demo flipped

`curl -s http://146.59.87.168:2100/packages/archipelago-companion.json`
should read 0.5.28/48 once CI's Portainer webhook redeploy lands; trigger a
stack redeploy if it lags.

## Final verify (all three must show 0.5.28 / 48)

```bash
aapt2 dump badging <downloaded apk> | head -1   # versionCode='48' versionName='0.5.28-debug'
apksigner verify -v --min-sdk-version 21 <downloaded apk> | grep scheme   # v1/v2/v3 true
curl -s https://source.archipelago-foundation.org/packages/archipelago-companion.json
curl -s http://146.59.87.168:2100/packages/archipelago-companion.json
```

Then the user's on-device end-to-end: scan the node's companion QR →
installs vc48 in place → hub → Backup & Restore / Remote Signer.
Testing notes for the new features live in the closed tracker issues
(#61/#128/#139) and `docs/companion-backup-restore.md` /
`docs/companion-nip46-remote-signer.md` (the signer's e2e harness:
`Android/tools/nip46-test-client.py`).
