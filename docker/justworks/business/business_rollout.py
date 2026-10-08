"""Operational rollout and browser-bound Business SSO, not a new identity provider."""
import hashlib
import json
import os
import re
import secrets
import time
from http.cookies import SimpleCookie
from urllib.parse import urlsplit, urlencode

import business_security as security

GROUPS = ("merchant_config", "staff_reads", "staff_mutations")
DEFAULT_ORIGINS = ("https://business.justworks.cash", "https://pos.justworks.cash")
SSO_COOKIE = "__Host-jwb_exchange"
TABLES = ("products", "tables", "orders", "payments", "quick_payments", "bookings", "merchant_modules")


def migrate(conn):
    conn.executescript("""
      CREATE TABLE IF NOT EXISTS business_session_audiences (
        token_hash TEXT PRIMARY KEY, audience TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS business_identity_verifications (
        npub TEXT PRIMARY KEY, slug TEXT NOT NULL, verified_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS business_merchant_aliases (
        alias TEXT PRIMARY KEY, npub TEXT NOT NULL UNIQUE, verified_slug TEXT NOT NULL,
        migrated_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS business_rollout_approvals (
        route_group TEXT NOT NULL, npub TEXT NOT NULL, approved_at INTEGER NOT NULL,
        PRIMARY KEY(route_group,npub));
      CREATE TABLE IF NOT EXISTS business_exchanges (
        request_hash TEXT PRIMARY KEY, binder_hash TEXT NOT NULL, npub TEXT NOT NULL,
        audience TEXT NOT NULL, return_path TEXT NOT NULL, expires_at INTEGER NOT NULL,
        code_hash TEXT UNIQUE, source_session TEXT, consumed INTEGER NOT NULL DEFAULT 0);
      CREATE INDEX IF NOT EXISTS business_exchanges_expiry ON business_exchanges(expires_at);
    """)


def legacy_merchant(conn):
    row = conn.execute("SELECT npub FROM business_merchant_aliases WHERE alias='localpub-demo'").fetchone()
    return row[0] if row else "localpub-demo"


def group_for(method, path):
    if method == "GET" and path in security.PRIVILEGED_GET | {"/api/business"}:
        return "staff_reads"
    if method == "POST" and path in security.PRIVILEGED_POST:
        return "merchant_config"
    if method == "POST" and re.fullmatch(r"/api/(orders|payments|bookings)/[^/]+/(status|acknowledge)", path):
        return "staff_mutations"
    return None


def policy():
    groups = json.loads(os.environ.get("JWB_AUTH_GROUPS", "{}"))
    if not isinstance(groups, dict) or any(k not in GROUPS or v not in ("shadow", "enforce") for k, v in groups.items()):
        raise ValueError("invalid JWB_AUTH_GROUPS")
    canaries = set(filter(None, (x.strip() for x in os.environ.get("JWB_AUTH_CANARIES", "").split(","))))
    for merchant in canaries:
        security.canonical_npub(merchant)
    return groups, canaries


def effective_mode(conn, method, path, merchant):
    global_mode = security.mode()
    # Existing global enforce is retained for regression/local tests only.
    # Operators use shadow + groups; force-shadow overrides all group settings.
    if global_mode == "off" or os.environ.get("JWB_AUTH_FORCE_SHADOW", "0") == "1":
        return "off" if global_mode == "off" else "shadow"
    if global_mode == "enforce":
        return "enforce"
    groups, canaries = policy()
    group = group_for(method, path)
    if groups.get(group) != "enforce" or merchant not in canaries:
        return "shadow"
    approved = conn.execute("SELECT 1 FROM business_rollout_approvals WHERE route_group=? AND npub=?", (group, merchant)).fetchone()
    return "enforce" if approved else "shadow"


def verification(conn, npub, slug):
    conn.execute("INSERT OR REPLACE INTO business_identity_verifications VALUES (?,?,?)", (security.canonical_npub(npub), slug, int(time.time())))
    conn.commit()


def migration_plan(conn, npub, slug):
    npub = security.canonical_npub(npub)
    existing = conn.execute("SELECT * FROM business_merchant_aliases WHERE alias='localpub-demo'").fetchone()
    if existing:
        if existing["npub"] != npub or existing["verified_slug"] != slug:
            raise ValueError("legacy merchant already mapped to a different identity")
        return {"already_applied": True, "source": "localpub-demo", "target": npub, "counts": {}}
    verified = conn.execute("SELECT slug,verified_at FROM business_identity_verifications WHERE npub=?", (npub,)).fetchone()
    if not verified or verified[0] != slug or verified[1] < int(time.time()) - 86400:
        raise ValueError("recent Core-verified owner login and matching slug required")
    if not conn.execute("SELECT 1 FROM merchant WHERE id='localpub-demo'").fetchone():
        raise ValueError("legacy merchant missing")
    counts = {table: conn.execute(f"SELECT COUNT(*) FROM {table} WHERE merchant_id='localpub-demo'").fetchone()[0] for table in TABLES}
    # Never merge unrelated tenant records or choose which configuration wins.
    if conn.execute("SELECT 1 FROM merchant WHERE id=?", (npub,)).fetchone() or any(
        conn.execute(f"SELECT 1 FROM {table} WHERE merchant_id=? LIMIT 1", (npub,)).fetchone() for table in TABLES
    ):
        raise ValueError("target has existing data; explicit reconciliation required before migration")
    return {"already_applied": False, "source": "localpub-demo", "target": npub, "counts": counts}


def migrate_legacy(conn, npub, slug, apply=False):
    conn.execute("BEGIN IMMEDIATE")
    try:
        report = migration_plan(conn, npub, slug)
        if apply and not report["already_applied"]:
            conn.execute("UPDATE merchant SET id=? WHERE id='localpub-demo'", (npub,))
            for table in TABLES:
                conn.execute(f"UPDATE {table} SET merchant_id=? WHERE merchant_id='localpub-demo'", (npub,))
            conn.execute("INSERT INTO business_merchant_aliases VALUES ('localpub-demo',?,?,?)", (npub, slug, int(time.time())))
            conn.execute("INSERT INTO business_audit(occurred_at,event,route,merchant,actor,reason) VALUES (?,'merchant_migrated','operator',?,?,'explicit_verified_mapping')", (int(time.time()), npub, npub))
            conn.commit()
        else:
            conn.rollback()
        return {**report, "applied": apply}
    except Exception:
        conn.rollback()
        raise


def origins():
    values = tuple(x.strip() for x in os.environ.get("JWB_SSO_ORIGINS", ",".join(DEFAULT_ORIGINS)).split(",") if x.strip())
    for value in values:
        p = urlsplit(value)
        if p.scheme != "https" or not p.hostname or p.username or p.password or p.port or p.path or p.query or p.fragment or value != f"https://{p.hostname}" or not p.hostname.endswith(".justworks.cash"):
            raise ValueError("SSO origins must be exact HTTPS JustWorks subdomains")
    if DEFAULT_ORIGINS[0] not in values:
        raise ValueError("business.justworks.cash must remain the Business session hub")
    return values


def request_origin(headers):
    host = headers.get("Host", "").lower()
    origin = "https://" + host
    if origin not in origins():
        raise ValueError("SSO host not allowed")
    if headers.get("Origin") != origin or headers.get("Sec-Fetch-Site") == "cross-site":
        raise ValueError("explicit same-origin request required")
    return origin


def enabled():
    return os.environ.get("JWB_SSO_ENABLED", "0") == "1"


def safe_return(value):
    if not isinstance(value, str) or len(value) > 2000 or not value.startswith("/") or value.startswith("//") or "\\" in value or any(ord(c) < 32 for c in value):
        raise ValueError("invalid return path")
    p = urlsplit(value)
    if p.scheme or p.netloc or p.fragment or "%" in p.path:
        raise ValueError("invalid return path")
    return value


def opaque(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]{43}", value):
        raise ValueError("invalid exchange")
    return value


def exchange_cookie(token="", ttl=0):
    return f"{SSO_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={ttl}"


def start(conn, values, headers):
    audience = request_origin(headers)
    npub = security.canonical_npub(values.get("npub"))
    target = safe_return(values.get("return_path", "/"))
    request_id, binder = secrets.token_urlsafe(32), secrets.token_urlsafe(32)
    timestamp = int(time.time())
    conn.execute("DELETE FROM business_exchanges WHERE expires_at<=? OR consumed=1", (timestamp,))
    if conn.execute("SELECT COUNT(*) FROM business_exchanges").fetchone()[0] >= 1000:
        raise ValueError("exchange capacity reached")
    conn.execute("INSERT INTO business_exchanges(request_hash,binder_hash,npub,audience,return_path,expires_at) VALUES (?,?,?,?,?,?)", (security.token_hash(request_id), security.token_hash(binder), npub, audience, target, timestamp + 180))
    conn.commit()
    return {"request_id": request_id, "authorize_url": DEFAULT_ORIGINS[0] + "/business-sso?" + urlencode({"npub": npub}) + "#" + request_id, "source_origin": DEFAULT_ORIGINS[0]}, exchange_cookie(binder, 180)


def pending(conn, request_id):
    row = conn.execute("SELECT * FROM business_exchanges WHERE request_hash=? AND expires_at>? AND consumed=0", (security.token_hash(opaque(request_id)), int(time.time()))).fetchone()
    if not row:
        raise ValueError("exchange expired or used")
    return row


def source_details(conn, values, headers, approve=False):
    if request_origin(headers) != DEFAULT_ORIGINS[0]:
        raise ValueError("only Business session hub may approve")
    row = pending(conn, values.get("request_id"))
    current = security.session(conn, headers)
    if not current or current["npub"] != row["npub"]:
        raise ValueError("matching verified Business login required")
    if row["audience"] not in origins():
        raise ValueError("audience no longer allowed")
    result = {"audience": row["audience"], "npub": row["npub"]}
    if approve:
        if row["code_hash"]:
            raise ValueError("exchange already approved")
        code = secrets.token_urlsafe(32)
        cookie = SimpleCookie(); cookie.load(headers.get("Cookie", ""))
        source_hash = security.token_hash(cookie[security.COOKIE].value)
        conn.execute("UPDATE business_exchanges SET code_hash=?,source_session=?,expires_at=? WHERE request_hash=?", (security.token_hash(code), source_hash, min(row["expires_at"], int(time.time()) + 60), row["request_hash"]))
        conn.commit()
        result["code"] = code
        security.audit(conn, "exchange_approved", "/api/business-sso/approve", row["npub"], current["npub"])
    return result


def complete(conn, values, headers):
    audience = request_origin(headers)
    row = pending(conn, values.get("request_id"))
    cookie = SimpleCookie(); cookie.load(headers.get("Cookie", ""))
    binder = cookie[SSO_COOKIE].value if SSO_COOKIE in cookie else ""
    code_hash = security.token_hash(opaque(values.get("code")))
    source = conn.execute("SELECT s.npub FROM business_sessions s JOIN business_session_audiences a ON a.token_hash=s.token_hash WHERE s.token_hash=? AND s.expires_at>? AND a.audience=?", (row["source_session"], int(time.time()), DEFAULT_ORIGINS[0][8:])).fetchone()
    if row["audience"] != audience or not secrets.compare_digest(row["binder_hash"], security.token_hash(binder)) or not secrets.compare_digest(row["code_hash"] or "", code_hash) or not source or source[0] != row["npub"]:
        raise ValueError("exchange binding mismatch")
    # The caller serializes HTTP operations with the application lock. This UPDATE
    # also makes consumption conditional for other operator/SQLite clients.
    updated = conn.execute("UPDATE business_exchanges SET consumed=1 WHERE request_hash=? AND consumed=0", (row["request_hash"],))
    if updated.rowcount != 1:
        raise ValueError("exchange already used")
    conn.commit()
    session_cookie, expires = security.issue(conn, row["npub"], headers)
    security.audit(conn, "exchange_completed", "/api/business-sso/complete", row["npub"], row["npub"])
    return {"ok": True, "npub": row["npub"], "expires_at": expires, "return_path": row["return_path"]}, session_cookie


def readiness(conn, npub, group):
    npub = security.canonical_npub(npub)
    if group not in GROUPS:
        raise ValueError("unknown route group")
    blockers = []
    if not conn.execute("SELECT 1 FROM business_identity_verifications WHERE npub=?", (npub,)).fetchone():
        blockers.append("no_verified_owner_login")
    hosts = {row[0] for row in conn.execute("SELECT DISTINCT a.audience FROM business_sessions s JOIN business_session_audiences a ON a.token_hash=s.token_hash WHERE s.npub=? AND s.expires_at>?", (npub, int(time.time())))}
    if not all(urlsplit(origin).hostname in hosts for origin in origins()):
        blockers.append("missing_verified_session_on_configured_hosts")
    if not enabled() or not conn.execute("SELECT 1 FROM business_audit WHERE event='exchange_completed' AND merchant=?", (npub,)).fetchone():
        blockers.append("subdomain_exchange_not_verified")
    if legacy_merchant(conn) == "localpub-demo":
        blockers.append("legacy_owner_mapping_unresolved")
    return blockers


def approve_group(conn, npub, group):
    blockers = readiness(conn, npub, group)
    if blockers:
        raise ValueError("readiness blocked: " + ", ".join(blockers))
    conn.execute("INSERT OR REPLACE INTO business_rollout_approvals VALUES (?,?,?)", (group, npub, int(time.time())))
    conn.commit()
    security.audit(conn, "rollout_approved", "operator", npub, npub, group)


def audit_report(conn, since=0, reveal=False):
    def identity(value):
        return value if reveal or value == "localpub-demo" else "merchant:" + hashlib.sha256(value.encode()).hexdigest()[:12] if value else "unknown"
    rows = conn.execute("SELECT event,route,merchant,reason,COUNT(*) AS events,MIN(occurred_at) AS first_seen,MAX(occurred_at) AS last_seen FROM business_audit WHERE occurred_at>=? AND event IN ('would_deny','denied') GROUP BY event,route,merchant,reason ORDER BY events DESC", (since,))
    observations = [{**dict(row), "merchant": identity(row["merchant"]), "group": group_for("GET" if row["route"] in security.PRIVILEGED_GET else "POST", row["route"])} for row in rows]
    groups, canaries = policy()
    active_sessions = conn.execute("SELECT COUNT(*) FROM business_sessions WHERE expires_at>?", (int(time.time()),)).fetchone()[0]
    blockers = []
    if not active_sessions: blockers.append("no_active_business_sessions")
    if legacy_merchant(conn) == 'localpub-demo': blockers.append("legacy_owner_mapping_unresolved")
    if not enabled(): blockers.append("subdomain_exchange_disabled")
    if not canaries: blockers.append("no_explicit_canary_merchants")
    return {"mode": security.mode(), "force_shadow": os.environ.get("JWB_AUTH_FORCE_SHADOW") == "1", "groups": groups,
            "canaries": [identity(x) for x in sorted(canaries)], "legacy_mapping": identity(legacy_merchant(conn)),
            "active_sessions": active_sessions, "blockers": blockers,
            "observations": observations, "coalesced_events_not_request_counts": True,
            "canary_readiness": [{"merchant": identity(x), "group": group, "blockers": readiness(conn, x, group), "approved": bool(conn.execute("SELECT 1 FROM business_rollout_approvals WHERE route_group=? AND npub=?", (group, x)).fetchone())} for x in sorted(canaries) for group in GROUPS]}
