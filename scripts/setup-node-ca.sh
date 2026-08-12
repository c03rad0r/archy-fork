#!/usr/bin/env bash
# Per-node certificate authority.
#
# WHY THIS EXISTS
#
# The node used to serve a bare self-signed leaf (setup-https-dev.sh). A browser
# can be told to trust that, but the exception is granted per ORIGIN — scheme +
# host + PORT. The dashboard on :443 and an app on :8334 are different origins,
# so each app port needed its own click-through, and a cert interstitial CANNOT
# be accepted inside an iframe: the embedded app just fails.
#
# A CA fixes that structurally. The user installs ONE certificate; every leaf it
# signs is then trusted, on every port, with no further prompts. Ports are not
# part of a certificate's identity — one leaf with the right SANs covers every
# port on the host — so this is what makes gated apps embeddable over HTTPS.
#
# The CA private key never leaves the node and signs nothing but this node's own
# leaf. Installing it means trusting THIS node, not a third party.
#
# Idempotent: re-running reuses an existing CA and only reissues the leaf (which
# is what you want when the node gains an address). Pass --force-ca to start over
# — that invalidates every copy users have already installed.

set -euo pipefail

SSL_DIR="${ARCHY_SSL_DIR:-/etc/archipelago/ssl}"
CA_CRT="$SSL_DIR/ca.crt"
CA_KEY="$SSL_DIR/ca.key"
CA_SRL="$SSL_DIR/ca.srl"
LEAF_CRT="$SSL_DIR/archipelago.crt"
LEAF_KEY="$SSL_DIR/archipelago.key"

CA_DAYS="${ARCHY_CA_DAYS:-3650}"
# Public CAs cap leaves at 398 days and browsers enforce it. That limit applies
# to publicly-trusted roots, not a privately-installed one, but a shorter leaf
# still bounds the damage from a key leak — and reissuing costs nothing here
# because this script is re-run on address changes anyway.
LEAF_DAYS="${ARCHY_LEAF_DAYS:-397}"

FORCE_CA=false
[ "${1:-}" = "--force-ca" ] && FORCE_CA=true

NODE_NAME="$(hostname -s 2>/dev/null || echo archipelago)"

log() { echo "  $*"; }

mkdir -p "$SSL_DIR"
chmod 755 "$SSL_DIR"

# --- Subject alternative names -----------------------------------------------
# Every name/address the node can be reached by must be in the leaf, because a
# certificate is scoped to names, not ports. Missing one here means that access
# path still throws a warning even after the CA is installed.
collect_sans() {
  local -a dns=() ips=()

  dns+=("archipelago.local" "$NODE_NAME" "$NODE_NAME.local" "localhost")

  # Tailscale gives a stable MagicDNS name; include it so tailnet access is clean.
  if command -v tailscale >/dev/null 2>&1; then
    local ts_name
    ts_name="$(tailscale status --json 2>/dev/null \
      | python3 -c 'import json,sys; d=json.load(sys.stdin); print((d.get("Self") or {}).get("DNSName","").rstrip("."))' 2>/dev/null || true)"
    [ -n "$ts_name" ] && dns+=("$ts_name")
  fi

  # Every non-loopback address the host currently holds, plus loopback itself.
  ips+=("127.0.0.1" "::1")
  while read -r addr; do
    [ -n "$addr" ] && ips+=("$addr")
  done < <(ip -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | sort -u)

  local out="" i=1 j=1
  for d in $(printf '%s\n' "${dns[@]}" | awk 'NF' | sort -u); do
    out="${out}DNS.$i:$d,"; i=$((i+1))
  done
  for a in $(printf '%s\n' "${ips[@]}" | awk 'NF' | sort -u); do
    out="${out}IP.$j:$a,"; j=$((j+1))
  done
  echo "${out%,}"
}

SAN="$(collect_sans)"
[ -z "$SAN" ] && { echo "ERROR: no SANs resolved — refusing to issue a useless cert" >&2; exit 1; }

# --- CA ----------------------------------------------------------------------
if [ "$FORCE_CA" = true ] && [ -f "$CA_CRT" ]; then
  log "--force-ca: replacing the existing CA (previously installed copies stop working)"
  rm -f "$CA_CRT" "$CA_KEY" "$CA_SRL"
fi

if [ -f "$CA_CRT" ] && [ -f "$CA_KEY" ]; then
  log "Reusing the existing node CA (installed copies keep working)"
else
  log "Creating this node's certificate authority…"
  openssl req -x509 -nodes -newkey rsa:4096 -sha256 -days "$CA_DAYS" \
    -keyout "$CA_KEY" -out "$CA_CRT" \
    -subj "/CN=Archipelago Node CA ($NODE_NAME)/O=Archipelago/OU=Node CA" \
    -addext "basicConstraints=critical,CA:TRUE,pathlen:0" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
  chmod 600 "$CA_KEY"
  chmod 644 "$CA_CRT"
fi

# --- Leaf --------------------------------------------------------------------
log "Issuing the server certificate for: $SAN"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

openssl req -nodes -newkey rsa:2048 -sha256 \
  -keyout "$TMP/leaf.key" -out "$TMP/leaf.csr" \
  -subj "/CN=$NODE_NAME/O=Archipelago" 2>/dev/null

cat >"$TMP/leaf.ext" <<EOF
basicConstraints=CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=$SAN
EOF

openssl x509 -req -in "$TMP/leaf.csr" -CA "$CA_CRT" -CAkey "$CA_KEY" \
  -CAcreateserial -CAserial "$CA_SRL" \
  -out "$TMP/leaf.crt" -days "$LEAF_DAYS" -sha256 -extfile "$TMP/leaf.ext" 2>/dev/null

# Swap in place only once both halves exist, so a failure mid-run cannot leave
# nginx pointing at a cert whose key is gone.
install -m 644 "$TMP/leaf.crt" "$LEAF_CRT"
install -m 600 "$TMP/leaf.key" "$LEAF_KEY"

# The leaf key has TWO readers with different privileges: nginx's master
# process (root) and the archipelago daemon (User=archipelago), which needs it
# to terminate TLS on gated app ports. Root-only 0600 silently costs the daemon
# its TLS — it logs "Permission denied" and every app port quietly stays plain
# HTTP, which is exactly the fail-open shape the gate is built to avoid. So the
# key is group-readable by the service user and nothing wider.
SERVICE_USER="${ARCHY_SERVICE_USER:-archipelago}"
if getent group "$SERVICE_USER" >/dev/null 2>&1; then
  chgrp "$SERVICE_USER" "$LEAF_KEY" && chmod 640 "$LEAF_KEY"
  log "Key readable by group $SERVICE_USER (0640) — the daemon needs it for app-port TLS"
elif getent passwd "$SERVICE_USER" >/dev/null 2>&1; then
  # User exists without an eponymous group — fall back to its primary group.
  PRIMARY="$(id -gn "$SERVICE_USER" 2>/dev/null || true)"
  if [ -n "$PRIMARY" ]; then
    chgrp "$PRIMARY" "$LEAF_KEY" && chmod 640 "$LEAF_KEY"
    log "Key readable by group $PRIMARY (0640)"
  fi
else
  log "No '$SERVICE_USER' user on this host — key left root-only (0600)"
fi

# The dashboard serves this for download; it is a public certificate, never the key.
install -m 644 "$CA_CRT" "$SSL_DIR/ca-download.crt"

FP="$(openssl x509 -in "$CA_CRT" -noout -fingerprint -sha256 | cut -d= -f2)"
log "CA fingerprint (SHA-256): $FP"

# --- nginx HTTPS listener -----------------------------------------------------
# The CA is only useful if something actually serves TLS. Bind the dashboard's
# HTTPS on this host's LAN addresses ONLY: tailscaled already owns :443 on the
# tailnet addresses (with its own Let's Encrypt cert), so a plain
# `listen 443 default_server` binds 0.0.0.0 and fails with EADDRINUSE — nginx
# then keeps running the OLD config and the reload looks like it worked.
# Observed exactly that on archi-dev-box.
#
# Port 80 keeps serving: nodes are reached by IP on LANs where forcing a
# redirect would strand anyone who has not installed the CA yet.
ensure_nginx_https() {
  local site="${ARCHY_NGINX_SITE:-/etc/nginx/sites-enabled/archipelago}"
  [ -f "$site" ] || { log "No nginx site at $site — skipping HTTPS listener"; return; }

  local addrs
  addrs="$(ip -o -4 addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 \
    | grep -vE '^100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.' | sort -u)"
  [ -z "$addrs" ] && { log "No LAN address — skipping HTTPS listener"; return; }

  if grep -q 'listen .*:443 ssl' "$site"; then
    log "nginx HTTPS listener already present"
    return
  fi
  log "Adding nginx HTTPS listener on: $(echo "$addrs" | tr '\n' ' ')"
  python3 - "$site" "$addrs" <<'PYEOF'
import sys, re, pathlib
site, addrs = pathlib.Path(sys.argv[1]), sys.argv[2].split()
s = site.read_text(); lines = s.split('\n')
start = next(i for i,l in enumerate(lines) if l.strip() == 'server {')
depth = 0; end = None
for i in range(start, len(lines)):
    depth += lines[i].count('{') - lines[i].count('}')
    if depth == 0 and i > start:
        end = i; break
block = lines[start:end+1]
https = []
for l in block:
    if re.match(r'\s*listen 80 default_server;', l):
        https += [f'    listen {a}:443 ssl;' for a in addrs]
        https += ['    ssl_certificate /etc/archipelago/ssl/archipelago.crt;',
                  '    ssl_certificate_key /etc/archipelago/ssl/archipelago.key;',
                  '    ssl_protocols TLSv1.2 TLSv1.3;']
        continue
    if re.match(r'\s*listen \[::\]:80 default_server;', l):
        continue
    https.append(l)
lines = lines[:end+1] + [''] + https + lines[end+1:]
site.write_text('\n'.join(lines))
PYEOF
}

ensure_nginx_https

if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet nginx; then
  if nginx -t >/dev/null 2>&1; then
    systemctl reload nginx && log "nginx reloaded"
  else
    echo "WARNING: nginx config test failed — NOT reloading. Certs are in place; fix nginx and reload." >&2
  fi
fi

cat <<EOF

Done. Install $CA_CRT on each device that should reach this node without warnings.
The dashboard serves it at /ca.crt (Settings → Node certificate).
Verify the fingerprint above matches what the dashboard shows before trusting it.
EOF
