"""Additive Business authorization; never an identity provider or key store."""
import hashlib
import ipaddress
import os
import re
import secrets
import time
from http.cookies import SimpleCookie, CookieError
from collections import OrderedDict

COOKIE = "jwb_session"
PRIVILEGED_GET = {"/api/dashboard", "/api/pos"}
PRIVILEGED_POST = {"/api/config", "/api/business-config", "/api/business-profile"}
RATE_BUCKETS = OrderedDict()


def canonical_npub(value):
    """Validate NIP-19 bech32 checksum and the 32-byte public key length."""
    alphabet = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
    if not isinstance(value, str) or len(value) != 63 or not value.startswith("npub1"):
        raise ValueError("invalid npub")
    try:
        data = [alphabet.index(char) for char in value[5:]]
    except ValueError:
        raise ValueError("invalid npub") from None
    checksum = 1
    expanded = [ord(c) >> 5 for c in "npub"] + [0] + [ord(c) & 31 for c in "npub"]
    for item in expanded + data:
        high = checksum >> 25
        checksum = ((checksum & 0x1ffffff) << 5) ^ item
        for bit, generator in enumerate((0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3)):
            if (high >> bit) & 1: checksum ^= generator
    if checksum != 1 or data[-7] & 15:
        raise ValueError("invalid npub")
    return value


def client_ip(remote, headers):
    """Trust one Nginx-overwritten X-Real-IP only from explicitly enabled loopback."""
    def normalize(value):
        if not isinstance(value, str) or '%' in value:
            raise ValueError("invalid address")
        address = ipaddress.ip_address(value.strip())
        if isinstance(address, ipaddress.IPv6Address) and address.ipv4_mapped:
            return address.ipv4_mapped
        return address

    try:
        peer = normalize(remote)
    except ValueError:
        return "unknown"
    if os.environ.get("JWB_TRUST_PROXY", "0") != "1" or not peer.is_loopback:
        return str(peer)
    # HTTPMessage permits duplicate headers; a normal dict is used by unit tests.
    values = headers.get_all("X-Real-IP", []) if hasattr(headers, "get_all") else [headers.get("X-Real-IP", "")]
    if len(values) == 1:
        try:
            return str(normalize(values[0]))
        except ValueError:
            pass
    return str(peer)


def rate_scope(path, values):
    # Table flows ignore caller-supplied npub; opaque tokens never enter bucket keys.
    if path in {"/api/orders", "/api/payments"}:
        for field in ("token", "order_id", "payment_id"):
            value = values.get(field)
            if isinstance(value, str) and value:
                return field + ":" + token_hash(value)
        return "unscoped"
    try:
        return "merchant:" + canonical_npub(values.get("npub", ""))
    except ValueError:
        return "unscoped"


def rate_allowed(remote, path, values=None, headers=None):
    # Caller holds the process lock. Keep only a digest of the normalized peer.
    timestamp = int(time.time()) // 60
    peer = client_ip(remote, headers or {})
    key = (token_hash(peer), path, rate_scope(path, values or {}))
    prior_window, count = RATE_BUCKETS.pop(key, (timestamp, 0))
    count = count + 1 if prior_window == timestamp else 1
    RATE_BUCKETS[key] = (timestamp, count)
    while len(RATE_BUCKETS) > 2048: RATE_BUCKETS.popitem(last=False)
    return count <= (20 if path == "/api/business-owner-login" else 120)


def mode():
    value = os.environ.get("JWB_AUTH_MODE", "shadow")
    if value not in {"off", "shadow", "enforce"}:
        raise RuntimeError("JWB_AUTH_MODE must be off, shadow or enforce")
    return value


def migrate(conn):
    conn.executescript("""
        CREATE TABLE IF NOT EXISTS business_sessions (
          token_hash TEXT PRIMARY KEY, npub TEXT NOT NULL, created_at INTEGER NOT NULL,
          expires_at INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS business_sessions_expiry ON business_sessions(expires_at);
        CREATE TABLE IF NOT EXISTS business_audit (
          id INTEGER PRIMARY KEY AUTOINCREMENT, occurred_at INTEGER NOT NULL,
          event TEXT NOT NULL, route TEXT NOT NULL, merchant TEXT NOT NULL,
          actor TEXT NOT NULL, reason TEXT NOT NULL);
    """)
    import business_rollout
    business_rollout.migrate(conn)


def audit(conn, event, route="", merchant="", actor="", reason=""):
    # Only fixed event/reason codes and public identities; never bodies, cookies or exceptions.
    timestamp = int(time.time())
    route = re.sub(r"/api/(orders|payments|bookings)/[^/]+/", r"/api/\1/:id/", route)
    merchant = merchant if re.fullmatch(r"npub1[023456789acdefghjklmnpqrstuvwxyz]{58}|localpub-demo", merchant) else ""
    actor = actor if re.fullmatch(r"npub1[023456789acdefghjklmnpqrstuvwxyz]{58}", actor) else ""
    if event == "would_deny" and conn.execute(
        "SELECT 1 FROM business_audit WHERE event=? AND route=? AND merchant=? AND reason=? AND occurred_at>? LIMIT 1",
        (event, route, merchant, reason, timestamp - 60)).fetchone():
        return
    conn.execute("INSERT INTO business_audit(occurred_at,event,route,merchant,actor,reason) VALUES (?,?,?,?,?,?)",
                 (timestamp, event, route, merchant, actor, reason))
    conn.execute("DELETE FROM business_audit WHERE id NOT IN (SELECT id FROM business_audit ORDER BY id DESC LIMIT 10000)")
    conn.execute("DELETE FROM business_sessions WHERE expires_at<=?", (timestamp,))
    conn.execute("DELETE FROM business_session_audiences WHERE token_hash NOT IN (SELECT token_hash FROM business_sessions)")
    conn.commit()


def token_hash(token):
    return hashlib.sha256(token.encode()).hexdigest()


def session(conn, headers):
    try:
        cookie = SimpleCookie()
        cookie.load(headers.get("Cookie", ""))
        token = cookie[COOKIE].value
        if not re.fullmatch(r"[A-Za-z0-9_-]{43}", token):
            return None
        row = conn.execute("SELECT s.npub,s.expires_at FROM business_sessions s LEFT JOIN business_session_audiences a ON a.token_hash=s.token_hash WHERE s.token_hash=? AND s.expires_at>? AND (a.audience=? OR a.audience IS NULL)",
                           (token_hash(token), int(time.time()), headers.get("Host", "").lower())).fetchone()
        return dict(row) if row else None
    except (KeyError, ValueError, CookieError):
        return None


def revoke(conn, headers, all_sessions=False):
    current = session(conn, headers)
    if all_sessions and current:
        conn.execute("DELETE FROM business_sessions WHERE npub=?", (current["npub"],))
    else:
        cookie = SimpleCookie()
        try:
            cookie.load(headers.get("Cookie", ""))
            conn.execute("DELETE FROM business_sessions WHERE token_hash=?", (token_hash(cookie[COOKIE].value),))
        except (KeyError, ValueError, CookieError):
            pass
    conn.execute("DELETE FROM business_session_audiences WHERE token_hash NOT IN (SELECT token_hash FROM business_sessions)")
    conn.commit()
    return current


def cookie_header(token="", max_age=0):
    # Local HTTP opt-out is operator configuration, never inferred from attacker-controlled Host.
    secure = "" if os.environ.get("JWB_COOKIE_SECURE", "1") == "0" else "; Secure"
    return f"{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"


def issue(conn, npub, headers):
    npub = canonical_npub(npub)
    revoke(conn, headers)
    token = secrets.token_urlsafe(32)
    ttl = max(60, min(86400, int(os.environ.get("JWB_SESSION_TTL", "28800"))))
    expires = int(time.time()) + ttl
    conn.execute("INSERT INTO business_sessions(token_hash,npub,created_at,expires_at) VALUES (?,?,?,?)", (token_hash(token), npub, int(time.time()), expires))
    conn.execute("INSERT INTO business_session_audiences VALUES (?,?)", (token_hash(token), headers.get("Host", "").lower()))
    conn.execute("DELETE FROM business_sessions WHERE npub=? AND token_hash NOT IN (SELECT token_hash FROM business_sessions WHERE npub=? ORDER BY created_at DESC,rowid DESC LIMIT 10)", (npub, npub))
    conn.execute("DELETE FROM business_session_audiences WHERE token_hash NOT IN (SELECT token_hash FROM business_sessions)")
    conn.commit()
    return cookie_header(token, ttl), expires


def target(conn, method, path, values):
    from business_rollout import legacy_merchant
    if method == "GET" and path in PRIVILEGED_GET:
        return legacy_merchant(conn) if path == "/api/dashboard" else str(values.get("npub", "")).strip()
    if method == "POST" and path in PRIVILEGED_POST:
        return legacy_merchant(conn) if path == "/api/config" else str(values.get("npub", "")).strip()
    match = re.fullmatch(r"/api/(orders|payments|bookings)/([^/]+)/(status|acknowledge)", path)
    if method == "POST" and match:
        table, identity, _ = match.groups()
        row = conn.execute(f"SELECT merchant_id FROM {table} WHERE id=?", (identity,)).fetchone()
        return row[0] if row else ""
    return None


def same_origin(headers):
    from urllib.parse import urlparse
    origin = headers.get("Origin")
    if headers.get("Sec-Fetch-Site") == "cross-site":
        return False
    if not origin:
        return True  # non-browser clients; browser JSON requests require CORS preflight
    parsed = urlparse(origin)
    return parsed.scheme in {"http", "https"} and parsed.netloc.lower() == headers.get("Host", "").lower()


def authorize(conn, method, path, values, headers):
    merchant = target(conn, method, path, values)
    if merchant is None:
        return True, None
    current = session(conn, headers)
    reason = ""
    if not current:
        reason = "missing_session"
    elif current["npub"] != merchant:
        reason = "tenant_mismatch"
    elif method != "GET" and not same_origin(headers):
        reason = "cross_origin"
    from business_rollout import effective_mode
    selected = effective_mode(conn, method, path, merchant)
    if reason and selected != "off":
        audit(conn, "denied" if selected == "enforce" else "would_deny", path, merchant,
              current["npub"] if current else "", reason)
    return not reason or selected != "enforce", (merchant, current["npub"] if current else "")
