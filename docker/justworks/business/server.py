#!/usr/bin/env python3
import json
import re
import ipaddress
import mimetypes
import os
import secrets
import socket
import ssl
import http.client
import sqlite3
import subprocess
import threading
import business_security as security
import business_rollout as rollout
import screen_activity
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.cookies import CookieError
from pathlib import Path
from urllib.parse import parse_qs, urlparse
from urllib.parse import urlencode
from urllib.error import HTTPError
from urllib.request import Request, urlopen, build_opener, HTTPRedirectHandler
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent
PUBLIC = ROOT / "public"
NAPLET_DIST = ROOT / "napplets/order-to-table/dist"
DATA_DIR = Path(os.environ.get("JWB_DATA_DIR", ROOT / "data"))
DB_FILE = DATA_DIR / "business.sqlite3"
HOST = os.environ.get("JWB_HOST", "127.0.0.1")
PORT = int(os.environ.get("JWB_PORT", "4310"))
BASE = os.environ.get("JWB_BASE_PATH", "/business-demo").rstrip("/")
LOCK = threading.RLock()
PAYMENT_HELPER = ROOT / "payment-helper.cjs"
PAYMENT_RELAYS = ["wss://relay.primal.net", "wss://nos.lol"]
PAYMENT_RECIPIENT = "f81611363554b64306467234d7396ec88455707633f54738f6c4683535098cd3"
LNURL_URL = "https://primal.net/.well-known/lnurlp/fucking"
DEV_LIGHTNING_ADDRESS = "fucking@primal.net"  # dev stand-in matching LNURL_URL, used when a merchant has no real lud16
LNURL_CALLBACK = "https://primal.net/lnurlp/fucking/callback"
MODULE_DEFAULTS = {
    "order": (1, "live"), "pay": (1, "live"), "payandgo": (1, "preview"),
    "booking": (1, "live"), "screen": (1, "preview"),
    "gallery": (1, "preview"), "stats": (1, "live"), "events": (0, "coming-soon"),
}


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def db():
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_FILE, timeout=10)
    conn.row_factory = sqlite3.Row
    security.migrate(conn)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.executescript("""
      CREATE TABLE IF NOT EXISTS merchant (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, module_enabled INTEGER NOT NULL,
        table_count INTEGER NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS products (
        id TEXT PRIMARY KEY, merchant_id TEXT NOT NULL, name TEXT NOT NULL,
        price_cents INTEGER NOT NULL, enabled INTEGER NOT NULL, sort_order INTEGER NOT NULL
      );
      CREATE TABLE IF NOT EXISTS tables (
        merchant_id TEXT NOT NULL, table_number INTEGER NOT NULL, token TEXT NOT NULL UNIQUE,
        PRIMARY KEY(merchant_id, table_number)
      );
      CREATE TABLE IF NOT EXISTS orders (
        id TEXT PRIMARY KEY, merchant_id TEXT NOT NULL, table_number INTEGER NOT NULL,
        items_json TEXT NOT NULL, note TEXT NOT NULL, status TEXT NOT NULL,
        created_at TEXT NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS payments (
        id TEXT PRIMARY KEY, order_id TEXT NOT NULL, merchant_id TEXT NOT NULL,
        table_number INTEGER NOT NULL, amount_cents INTEGER NOT NULL,
        amount_sats INTEGER NOT NULL, bolt11 TEXT NOT NULL, zap_pubkey TEXT NOT NULL,
        status TEXT NOT NULL, receipt_id TEXT, created_at TEXT NOT NULL, settled_at TEXT
      );
      CREATE TABLE IF NOT EXISTS quick_payments (
        id TEXT PRIMARY KEY, merchant_id TEXT NOT NULL, merchant_name TEXT NOT NULL,
        amount_cents INTEGER NOT NULL, amount_sats INTEGER NOT NULL, bolt11 TEXT NOT NULL,
        zap_pubkey TEXT, recipient TEXT, status TEXT NOT NULL, receipt_id TEXT,
        created_at TEXT NOT NULL, settled_at TEXT
      );
      CREATE TABLE IF NOT EXISTS bookings (
        id TEXT PRIMARY KEY, merchant_id TEXT NOT NULL, guest_name TEXT NOT NULL,
        guests INTEGER NOT NULL, scheduled_at TEXT NOT NULL, mode TEXT NOT NULL,
        notes TEXT NOT NULL, status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS merchant_modules (
        merchant_id TEXT NOT NULL, module_id TEXT NOT NULL, enabled INTEGER NOT NULL,
        status TEXT NOT NULL, config_json TEXT NOT NULL, updated_at TEXT NOT NULL,
        PRIMARY KEY(merchant_id,module_id)
      );
    """)
    legacy = rollout.legacy_merchant(conn)
    conn.execute("INSERT OR IGNORE INTO merchant VALUES (?,'LocalPub Madeira — Napplet Demo',1,30,?)", (legacy, now()))
    defaults = [("poncha", "Poncha Fisherman", 350, 1), ("nikita", "Nikita", 450, 2), ("coral", "Coral", 250, 3)]
    for pid, name, cents, order in defaults:
        conn.execute("INSERT OR IGNORE INTO products VALUES (?, ?, ?, ?, 1, ?)", (pid, legacy, name, cents, order))
    for table in range(1, 31):
        conn.execute("INSERT OR IGNORE INTO tables VALUES (?, ?, ?)", (legacy, table, secrets.token_urlsafe(18)))
    for module_id, (enabled, status) in MODULE_DEFAULTS.items():
        conn.execute("INSERT OR IGNORE INTO merchant_modules VALUES (?,?,?,?,?,?)",
                     (legacy, module_id, enabled, status, '{}', now()))
    conn.execute("UPDATE merchant_modules SET status='live' WHERE module_id='booking' AND status='preview'")
    conn.commit()
    return conn


def helper(payload):
    result = subprocess.run(["node", str(PAYMENT_HELPER)], input=json.dumps(payload), text=True, capture_output=True, timeout=12, check=True)
    return json.loads(result.stdout)


def fetch_json(url):
    request = Request(url, headers={"User-Agent": "JustWorks-Business/0.1"})
    with urlopen(request, timeout=10) as response:
        return json.loads(response.read())


def screen_tips(npub, lightning_address):
    """Read-only LNURL metadata + signed NIP-57 announcements; no invoice creation."""
    if lightning_address.count('@') != 1:
        raise ValueError('invalid Lightning address')
    name, host = lightning_address.split('@')
    if not name or not host or any(char in host for char in '/?#:@'):
        raise ValueError('invalid Lightning address')
    endpoint = f'https://{host}/.well-known/lnurlp/{quote(name, safe="")}'
    pay = fetch_lnurl_json(endpoint, timeout=4)
    provider = pay.get('nostrPubkey')
    if pay.get('tag') != 'payRequest' or pay.get('allowsNostr') is not True or not isinstance(provider, str) or len(provider) != 64 or any(c not in '0123456789abcdef' for c in provider):
        raise ValueError('No verified Nostr receipt source')
    return helper({'action': 'screen-tips', 'npub': npub, 'provider': provider, 'relays': PAYMENT_RELAYS})['tips']


def resolve_public_https(url):
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
        raise ValueError("Lightning provider returned an unsafe URL")
    addresses = socket.getaddrinfo(parsed.hostname, parsed.port or 443, type=socket.SOCK_STREAM)
    if not addresses or any(not ipaddress.ip_address(item[4][0]).is_global for item in addresses):
        raise ValueError("Lightning provider must use a public host")
    return parsed, addresses


class PinnedLnurlConnection(http.client.HTTPSConnection):
    def __init__(self, parsed, addresses, timeout):
        # Direct sockets intentionally ignore ambient HTTP(S)_PROXY settings.
        super().__init__(parsed.hostname, parsed.port or 443, timeout=timeout,
                         context=ssl.create_default_context())
        self.addresses = addresses

    def connect(self):
        last_error = None
        for family, kind, protocol, _canonical, address in self.addresses:
            connection = socket.socket(family, kind, protocol)
            try:
                connection.settimeout(self.timeout)
                # Numeric sockaddr from the validated lookup: no second DNS lookup.
                connection.connect(address)
                self.sock = self._context.wrap_socket(connection, server_hostname=self.host)
                return
            except OSError as error:
                connection.close()
                last_error = error
        raise last_error or OSError("Lightning provider connection unavailable")


def fetch_lnurl_json(url, timeout=10):
    parsed, addresses = resolve_public_https(url)
    connection = PinnedLnurlConnection(parsed, addresses, timeout)
    try:
        target = parsed.path or "/"
        if parsed.query:
            target += "?" + parsed.query
        connection.request("GET", target, headers={"User-Agent": "JustWorks-Business/0.1"})
        response = connection.getresponse()
        if 300 <= response.status < 400:
            raise ValueError("Lightning provider redirects are not permitted")
        if response.status >= 400:
            raise HTTPError(url, response.status, response.reason, response.headers, None)
        raw = response.read(65537)
        if len(raw) > 65536:
            raise ValueError("Lightning provider response too large")
        return json.loads(raw)
    finally:
        connection.close()


def lightning_invoice(lightning_address, amount_msat, comment=""):
    if "@" not in lightning_address:
        raise ValueError("Merchant Lightning address is invalid")
    name, host = lightning_address.rsplit("@", 1)
    if not name or not host or any(char in host for char in "/?#"):
        raise ValueError("Merchant Lightning address is invalid")
    endpoint = f"https://{host}/.well-known/lnurlp/{quote(name, safe="")}"
    pay = fetch_lnurl_json(endpoint)
    if pay.get("tag") != "payRequest" or not pay.get("callback"):
        raise ValueError("Lightning address does not support payments")
    minimum, maximum = int(pay.get("minSendable", 0)), int(pay.get("maxSendable", 0))
    if amount_msat < minimum or (maximum and amount_msat > maximum):
        raise ValueError(f"Amount must be between {max(1, minimum // 1000)} and {maximum // 1000} sats")
    callback = str(pay["callback"])
    # Some LNURL providers reject otherwise valid NIP-57 requests when their
    # signed content contains a literal percent sign. Keep the human meaning
    # while using a provider-safe comment for both the zap and callback.
    safe_comment = str(comment or "").replace("%", " percent").strip()
    params = {"amount": amount_msat}
    proof = None
    recipient = str(pay.get("nostrPubkey") or "").strip()
    if pay.get("allowsNostr") is True and len(recipient) == 64:
        proof = helper({"action": "create", "lnurlUrl": endpoint, "relays": PAYMENT_RELAYS,
                        "amountMsat": amount_msat, "recipient": recipient,
                        "content": safe_comment or "Just Works payment"})
        params["nostr"] = json.dumps(proof["event"], separators=(",", ":"))
        params["lnurl"] = proof["lnurl"]
    if safe_comment and int(pay.get("commentAllowed", 0)):
        params["comment"] = safe_comment[:int(pay["commentAllowed"])]
    separator = "&" if "?" in callback else "?"
    try:
        invoice = fetch_lnurl_json(f"{callback}{separator}{urlencode(params)}")
    except HTTPError:
        if not proof:
            raise
        # Invoice creation is the primary action. If a provider advertises
        # NIP-57 but fails its zap path, retry as standard LNURL-pay and mark
        # the result non-verifiable instead of blocking the customer.
        proof = None
        fallback = {key: value for key, value in params.items() if key not in {"nostr", "lnurl"}}
        invoice = fetch_lnurl_json(f"{callback}{separator}{urlencode(fallback)}")
    if invoice.get("status") == "ERROR" or not invoice.get("pr"):
        raise ValueError(invoice.get("reason", "Lightning invoice unavailable"))
    return {"bolt11": invoice["pr"], "zap_pubkey": proof["pubkey"] if proof else None,
            "recipient": recipient if proof else None, "verifiable": bool(proof)}


def remote_json(url, method="GET", payload=None, timeout=60):
    data = json.dumps(payload).encode() if payload is not None else None
    request = Request(url, data=data, method=method, headers={"User-Agent": "JustWorks-Business/0.1", "Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=timeout) as response:
            return json.loads(response.read())
    except HTTPError as error:
        try: message = json.loads(error.read()).get("error")
        except Exception: message = None
        raise ValueError(message or f"Just Works profile request failed ({error.code})") from error


def resolve_merchant_npub(npub):
    npub = security.canonical_npub(npub)
    # Core resolves every published website; the regional directory is optional
    # presentation metadata, never a merchant allowlist or ownership proof.
    try:
        nsite = fetch_json(f"https://justworks.cash/api/nsites/{npub}").get("nsite", {})
    except HTTPError as error:
        if error.code == 404: return None, None
        raise
    slug = nsite.get("id", "")
    if (nsite.get("npub") != npub or not isinstance(slug, str)
            or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", slug)
            or nsite.get("nsite", {}).get("deploymentStatus") != "published"):
        return None, None
    place = {"npub": npub, "slug": slug, "name": nsite.get("name") or slug,
             "justworks": f"https://justworks.cash/{slug}"}
    return place, nsite


def merchant_payload(conn, include_tokens=False):
    merchant = dict(conn.execute("SELECT * FROM merchant WHERE id=?", (rollout.legacy_merchant(conn),)).fetchone())
    merchant["module_enabled"] = bool(merchant["module_enabled"])
    merchant["products"] = [dict(row) for row in conn.execute("SELECT id,name,price_cents,enabled,sort_order FROM products WHERE merchant_id=? ORDER BY sort_order", (merchant["id"],))]
    for product in merchant["products"]:
        product["enabled"] = bool(product["enabled"])
    merchant["modules"] = {}
    for row in conn.execute("SELECT module_id,enabled,status,config_json FROM merchant_modules WHERE merchant_id=?", (merchant["id"],)):
        merchant["modules"][row["module_id"]] = {"enabled": bool(row["enabled"]), "status": row["status"], "config": json.loads(row["config_json"] or '{}')}
    if include_tokens:
        merchant["tables"] = [dict(row) for row in conn.execute("SELECT table_number,token FROM tables WHERE merchant_id=? ORDER BY table_number", (merchant["id"],))]
    return merchant


def merchant_modules(conn, merchant_id):
    for module_id, (enabled, status) in MODULE_DEFAULTS.items():
        conn.execute("INSERT OR IGNORE INTO merchant_modules VALUES (?,?,?,?,?,?)",
                     (merchant_id, module_id, enabled, status, '{}', now()))
    conn.commit()
    return {row["module_id"]: {"enabled": bool(row["enabled"]), "status": row["status"], "config": json.loads(row["config_json"] or '{}')}
            for row in conn.execute("SELECT module_id,enabled,status,config_json FROM merchant_modules WHERE merchant_id=?", (merchant_id,))}


class Handler(BaseHTTPRequestHandler):
    server_version = "JustWorksBusiness/0.1"

    def log_message(self, fmt, *args):
        # Query strings may contain table tokens. Do not log request lines or bodies.
        print(json.dumps({"ts": now(), "method": self.command, "status": str(args[1]) if len(args) > 1 else ""}))

    def send_json(self, status, value):
        mutation = getattr(self, "authorized_mutation", None)
        if mutation and 200 <= status < 300:
            with LOCK, db() as conn:
                security.audit(conn, "mutation", self.route()[0], *mutation)
            self.authorized_mutation = None
        raw = json.dumps(value, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "no-store")
        if getattr(self, "session_cookie", None):
            self.send_header("Set-Cookie", self.session_cookie)
        if getattr(self, "exchange_cookie", None):
            self.send_header("Set-Cookie", self.exchange_cookie)
        self.send_header("Referrer-Policy", "no-referrer")
        self.end_headers()
        self.wfile.write(raw)

    def redirect(self, location):
        self.send_response(302)
        self.send_header("Location", location)
        self.send_header("Cache-Control", "public,max-age=3600")
        self.end_headers()

    def body(self):
        size = int(self.headers.get("Content-Length", "0"))
        if size < 0 or size > 64 * 1024:
            raise ValueError("body too large")
        value = json.loads(self.rfile.read(size) or b"{}")
        if not isinstance(value, dict): raise ValueError("JSON object required")
        return value

    def route(self):
        parsed = urlparse(self.path)
        path = parsed.path
        if BASE and path.startswith(BASE):
            path = path[len(BASE):] or "/"
        return path, parse_qs(parsed.query)

    def surface(self):
        host = self.headers.get("Host", "").split(":", 1)[0].lower()
        return host.removesuffix(".justworks.cash") if host.endswith(".justworks.cash") else ""

    def is_local(self):
        if os.environ.get("JWB_ARCHIPELAGO") == "1": return False
        # Anything not served behind the production subdomains is a dev server.
        host = self.headers.get("Host", "").split(":", 1)[0].lower()
        return not host.endswith(".justworks.cash")

    def static(self, file, cache="no-store"):
        file = file.resolve()
        allowed = [PUBLIC.resolve(), NAPLET_DIST.resolve()]
        if not any(file == root or root in file.parents for root in allowed) or not file.is_file():
            self.send_error(404)
            return
        raw = file.read_bytes()
        if os.environ.get("JWB_ARCHIPELAGO") == "1" and file.suffix == ".html":
            html = raw.decode().replace('"/assets/', '"/business-demo/assets/').replace('"/fonts/', '"/business-demo/fonts/')
            html = html.replace('<head>', '<head><script src="/business-demo/assets/archipelago-paths.js"></script>', 1)
            raw = html.encode()
        mime = mimetypes.guess_type(file.name)[0] or "application/octet-stream"
        self.send_response(200)
        self.send_header("Content-Type", mime)
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", cache)
        self.send_header("X-Content-Type-Options", "nosniff")
        if file.name == "business-sso.html":
            self.send_header("Referrer-Policy", "no-referrer")
            self.send_header("X-Frame-Options", "DENY")
            self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
        self.end_headers()
        self.wfile.write(raw)

    def nsite(self):
        file = PUBLIC / "nsite.html"
        html = file.read_text()
        html = html.replace("</body>", f'<script type="module" src="{BASE}/assets/nsite-live.js"></script></body>')
        raw = html.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(raw)

    def app_manifest(self, app, query):
        manifest = json.loads((PUBLIC / f"{app}.webmanifest").read_text())
        npub = (query.get("npub") or [""])[0]
        if npub:
            try: npub = security.canonical_npub(npub)
            except ValueError: return self.send_json(400, {"error": "invalid npub"})
            manifest["start_url"] = f"/?npub={npub}"
            manifest["id"] += f"&npub={npub}"
        raw = json.dumps(manifest, separators=(",", ":")).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/manifest+json")
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        self.session_cookie = self.exchange_cookie = self.authorized_mutation = None
        path, query = self.route()
        if path == "/api/business-session" or path in security.PRIVILEGED_GET:
            with LOCK, db() as conn:
                if path == "/api/business-session":
                    current = security.session(conn, self.headers)
                    return self.send_json(200, {"authenticated": bool(current), "session": current, "mode": security.mode(), "sso_enabled": rollout.enabled() and ('https://' + self.headers.get('Host', '').lower()) in rollout.origins(), "legacy_npub": rollout.legacy_merchant(conn) if rollout.legacy_merchant(conn) != 'localpub-demo' else None})
                allowed, _ = security.authorize(conn, "GET", path, {key: value[0] for key, value in query.items()}, self.headers)
            if not allowed: return self.send_json(403, {"error": "Merchant session required for this account"})
        if path == "/healthz": return self.send_json(200, {"ok": True})
        if path == "/api/config":
            with LOCK, db() as conn: return self.send_json(200, merchant_payload(conn))
        if path == "/api/dashboard":
            with LOCK, db() as conn:
                payload = merchant_payload(conn, True)
                payload["orders"] = [dict(row) | {"items": json.loads(row["items_json"])} for row in conn.execute("SELECT * FROM orders WHERE merchant_id=? AND status NOT IN ('delivered','closed') ORDER BY created_at DESC LIMIT 50", (rollout.legacy_merchant(conn),))]
                for item in payload["orders"]: item.pop("items_json", None)
                payload["payments"] = [dict(row) for row in conn.execute("SELECT id,order_id,table_number,amount_cents,amount_sats,status,created_at,settled_at FROM payments WHERE merchant_id=? AND status='paid' ORDER BY settled_at DESC LIMIT 20", (rollout.legacy_merchant(conn),))]
                return self.send_json(200, payload)
        if path == "/api/business":
            npub = (query.get("npub") or [""])[0].strip()
            try:
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                with LOCK, db() as conn:
                    modules = merchant_modules(conn, npub)
                    counts = {
                        "orders": conn.execute("SELECT COUNT(*) FROM orders WHERE merchant_id=? AND status!='closed'", (npub,)).fetchone()[0],
                        "payments": conn.execute("SELECT COUNT(*) FROM payments WHERE merchant_id=? AND status NOT IN ('acknowledged')", (npub,)).fetchone()[0],
                        "bookings": conn.execute("SELECT COUNT(*) FROM bookings WHERE merchant_id=? AND status NOT IN ('seated','closed','cancelled')", (npub,)).fetchone()[0],
                    }
                    current = security.session(conn, self.headers)
                    if rollout.effective_mode(conn, "GET", "/api/business", npub) == "enforce" and (not current or current["npub"] != npub):
                        counts = {key: 0 for key in counts}
                images = list(dict.fromkeys([nsite.get("media", {}).get("heroImage"), *nsite.get("media", {}).get("galleryImages", []), *place.get("images", [])]))
                images = [image for image in images if image and not any(marker in image.lower() for marker in ("logo", "icon", "static.cdninstagram.com"))]
                merchant_name = nsite.get("name") or place.get("name")
                query_string = urlencode({"q": merchant_name})
                map_query_string = urlencode({"q": merchant_name, "place": place.get("slug")})
                links = {
                    "justworks": place.get("justworks") or f"https://justworks.cash/{place.get('slug')}",
                    "tip": f"https://justworks.cash/tip/?{urlencode({'id': place.get('slug')})}",
                    "nsite": nsite.get("nsite", {}).get("gatewayUrl"),
                    "nostr": nsite.get("identity", {}).get("primalUrl") or f"https://njump.me/{npub}",
                    "maps": f"https://maps.justworks.cash/?{map_query_string}",
                    "gallery": f"https://gallery.justworks.cash/?{query_string}",
                }
                if self.is_local():
                    # Dev: keep merchants on local surfaces — /page renders the
                    # platform-hosted Just Works page, /site renders the nsite
                    # copy the product generates and publishes to Nostr.
                    links["justworks"] = f"{BASE}/page?npub={npub}"
                    links["nsite"] = f"{BASE}/site?npub={npub}"
                    links["gallery"] = f"{BASE}/gallery?{query_string}"
                profile = nsite.get("identity", {}).get("profile", {})
                lightning_address = str(profile.get("lud16") or "").strip()
                payment_ready = bool(lightning_address and not lightning_address.lower().endswith("@npub.cash"))
                if self.is_local() and not payment_ready:
                    # Dev only: stand in a working Lightning address so Pay / Pay & Go are unlocked.
                    lightning_address = DEV_LIGHTNING_ADDRESS
                    payment_ready = True
                return self.send_json(200, {"merchant": {"name": merchant_name, "npub": npub, "slug": place.get("slug"), "area": place.get("area"), "category": place.get("category")}, "images": images[:8], "links": links, "profile": profile, "lightning_address": lightning_address, "payment_ready": payment_ready, "modules": modules, "counts": counts})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"merchant unavailable: {error}"})
        if path == "/api/nsite-page":
            npub = (query.get("npub") or [""])[0].strip()
            try:
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                sections = nsite.get("page", {}).get("sections", [])
                menu = next((section for section in sections if section.get("type") == "menu" and section.get("enabled")), {})
                products = [{"id": str(item.get("id") or f"item-{index}")[:40], "name": str(item.get("title") or item.get("name") or "Item")[:80], "description": str(item.get("body") or item.get("description") or "")[:180], "price": str(item.get("price") or "")[:30]} for index, item in enumerate(menu.get("items", [])[:60])]
                assets = [item.get("url") for item in nsite.get("page", {}).get("assets", []) if item.get("url")]
                sources = [image for source in nsite.get("sources", []) for image in source.get("images", [])]
                media = nsite.get("media", {})
                images = [image for image in dict.fromkeys([media.get("heroImage"), *media.get("galleryImages", []), *assets, *sources, *place.get("images", [])]) if image and not any(marker in image.lower() for marker in ("logo", "icon", "static.cdninstagram.com"))][:12]
                profile = nsite.get("identity", {}).get("profile", {})
                sources = [{"name": str(source.get("title") or source.get("host") or "Source")[:80], "host": str(source.get("host") or "")[:60], "url": str(source.get("url") or "")[:300], "description": str(source.get("description") or "")[:180], "photos": len(source.get("images", []))} for source in nsite.get("sources", [])][:6]
                return self.send_json(200, {
                    "merchant": {"name": nsite.get("name") or place.get("name"), "npub": npub, "slug": place.get("slug"), "area": place.get("area"), "category": place.get("category"), "address": place.get("address"), "hours": place.get("hours"), "phone": place.get("phone"), "website": place.get("website"), "tags": [tag for tag in place.get("tags", [])[:8] if tag], "rating": place.get("rating")},
                    "about": str(profile.get("about") or "").strip()[:600],
                    "products": products,
                    "images": images,
                    "sources": sources,
                    "gateway": nsite.get("nsite", {}).get("gatewayUrl", ""),
                    "links": {"business": f"{BASE}/dashboard?npub={npub}", "page": f"{BASE}/page?npub={npub}"},
                })
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"site unavailable: {error}"})
        if path == "/api/qr":
            text = (query.get("text") or [""])[0].strip()
            if not text or len(text) > 500: return self.send_json(400, {"error": "provide qr text"})
            try:
                raw = helper({"action": "qr", "text": text})["svg"].encode()
                self.send_response(200)
                self.send_header("Content-Type", "image/svg+xml")
                self.send_header("Cache-Control", "public,max-age=3600")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)
                return
            except Exception as error: return self.send_json(502, {"error": f"qr unavailable: {error}"})
        if path == "/api/gallery":
            try:
                payload = fetch_json("https://justworks.cash/data/regions/madeira.json")
                return self.send_json(200, {"name": payload.get("name", "Madeira"), "places": payload.get("places", [])})
            except Exception as error:
                return self.send_json(502, {"error": f"gallery data unavailable: {error}"})
        if path == "/api/screen":
            npub = (query.get("npub") or [""])[0].strip()
            try:
                npub = security.canonical_npub(npub)
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                screen_activity.configure(npub, nsite)
                assets = [item.get("url") for item in nsite.get("page", {}).get("assets", []) if item.get("url")]
                sources = [image for source in nsite.get("sources", []) for image in source.get("images", [])]
                images = list(dict.fromkeys([nsite.get("media", {}).get("heroImage"), *nsite.get("media", {}).get("galleryImages", []), *assets, *sources, *place.get("images", [])]))
                images = [image for image in images if image and not any(marker in image.lower() for marker in ("static.cdninstagram.com", "facebook.com/images", "logo", "icon"))][:18]
                images.sort(key=lambda image: (image.lower().split("?", 1)[0].endswith(".png"), "folha" in image.lower()))
                order_url = f"http://{self.headers.get('Host', '127.0.0.1:4310')}{BASE}/order?npub={npub}" if self.is_local() else f"https://order.justworks.cash/?npub={npub}"
                justpay_url = f"http://{self.headers.get('Host', '127.0.0.1:4310')}{BASE}/portal?{urlencode({'surface': 'payandgo', 'npub': npub})}" if self.is_local() else f"https://payandgo.justworks.cash/?npub={npub}"
                tip_url = f"https://justworks.cash/tip/?{urlencode({'id': place['slug']})}"
                qr = helper({"action": "qr", "text": order_url})["svg"]
                try: latest = helper({"action": "latest", "npub": npub, "relays": PAYMENT_RELAYS})["event"]
                except Exception: latest = None
                return self.send_json(200, {"merchant": {"name": nsite.get("name") or place.get("name"), "npub": npub, "slug": place.get("slug"), "area": place.get("area"), "category": place.get("category")}, "images": images, "latest": latest, "order_url": order_url, "qr_svg": qr, "screen_links": {"order": order_url, "justpay": justpay_url, "tip": tip_url}})
            except ValueError as error:
                return self.send_json(400, {"error": str(error)})
            except Exception as error:
                return self.send_json(502, {"error": f"screen data unavailable: {error}"})
        if path == "/api/screen-activity":
            try:
                npub = security.canonical_npub((query.get('npub') or [''])[0].strip())
            except ValueError:
                return self.send_json(400, {'error': 'provide a valid merchant npub'})
            with LOCK, db() as conn:
                payload = screen_activity.projection(conn, npub, screen_tips)
            return self.send_json(200, payload)
        if path == "/api/order-merchant":
            npub = (query.get("npub") or [""])[0].strip()
            try:
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                sections = nsite.get("page", {}).get("sections", [])
                menu = next((section for section in sections if section.get("type") == "menu" and section.get("enabled")), {})
                products = [{"id": str(item.get("id") or f"item-{index}")[:40], "name": str(item.get("title") or item.get("name") or "Item")[:80], "description": str(item.get("body") or item.get("description") or "")[:180], "price": str(item.get("price") or "")[:30]} for index, item in enumerate(menu.get("items", [])[:50])]
                images = list(dict.fromkeys([nsite.get("media", {}).get("heroImage"), *nsite.get("media", {}).get("galleryImages", []), *place.get("images", [])]))
                images = [image for image in images if image and not any(marker in image.lower() for marker in ("logo", "icon", "static.cdninstagram.com"))]
                images.sort(key=lambda image: image.lower().split("?", 1)[0].endswith(".png"))
                return self.send_json(200, {"merchant": {"name": nsite.get("name") or place.get("name"), "npub": npub, "slug": place.get("slug"), "area": place.get("area"), "category": place.get("category")}, "images": images[:8], "products": products})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"merchant unavailable: {error}"})
        if path == "/api/merchant-order-status":
            npub = (query.get("npub") or [""])[0].strip()
            order_id = (query.get("id") or [""])[0].strip()
            if not npub.startswith("npub1") or len(npub) > 100 or not order_id:
                return self.send_json(400, {"error": "provide the merchant and order"})
            with LOCK, db() as conn:
                row = conn.execute("SELECT id,table_number,status,updated_at FROM orders WHERE id=? AND merchant_id=?", (order_id, npub)).fetchone()
                if not row: return self.send_json(404, {"error": "order not found"})
                return self.send_json(200, dict(row))
        if path == "/api/quick-pay-status":
            npub = (query.get("npub") or [""])[0].strip()
            payment_id = (query.get("id") or [""])[0].strip()
            if not npub.startswith("npub1") or len(npub) > 100 or len(payment_id) != 32:
                return self.send_json(400, {"error": "provide the merchant and payment"})
            with LOCK, db() as conn:
                row = conn.execute("SELECT * FROM quick_payments WHERE id=? AND merchant_id=?", (payment_id, npub)).fetchone()
                if not row: return self.send_json(404, {"error": "payment not found"})
                payment = dict(row)
            if payment["status"] == "pending" and payment["zap_pubkey"] and payment["recipient"]:
                try:
                    created_at = int(datetime.fromisoformat(payment["created_at"]).timestamp())
                    check = helper({"action": "check", "relays": PAYMENT_RELAYS, "recipient": payment["recipient"],
                                    "pubkey": payment["zap_pubkey"], "since": created_at})
                    if check["paid"]:
                        settled_at = now()
                        with LOCK, db() as conn:
                            conn.execute("UPDATE quick_payments SET status='paid',receipt_id=?,settled_at=? WHERE id=?",
                                         (check["receipt"], settled_at, payment_id))
                            conn.commit()
                        payment.update(status="paid", receipt_id=check["receipt"], settled_at=settled_at)
                except Exception:
                    pass
            fields = ("id", "merchant_name", "amount_cents", "amount_sats", "status", "receipt_id", "created_at", "settled_at")
            return self.send_json(200, {key: payment[key] for key in fields} | {"verifiable": bool(payment["zap_pubkey"])})
        if path == "/api/pay-orders":
            # Existing customer receipt discovery, without staff notes, guest bookings or payments.
            npub = (query.get("npub") or [""])[0].strip()
            try: security.canonical_npub(npub)
            except ValueError: return self.send_json(400, {"error": "provide a valid npub"})
            with LOCK, db() as conn:
                orders = [{"table_number": row["table_number"], "status": row["status"],
                           "items": [{key: item.get(key) for key in ("name", "qty", "price_cents")} for item in json.loads(row["items_json"])]}
                          for row in conn.execute("SELECT table_number,status,items_json FROM orders WHERE merchant_id=? AND status!='closed' ORDER BY created_at", (npub,))]
            return self.send_json(200, {"orders": orders})
        if path == "/api/pos":
            npub = (query.get("npub") or [""])[0].strip()
            if not npub.startswith("npub1") or len(npub) > 100:
                return self.send_json(400, {"error": "provide a valid npub"})
            with LOCK, db() as conn:
                orders = [dict(row) | {"items": json.loads(row["items_json"])} for row in conn.execute("SELECT * FROM orders WHERE merchant_id=? AND status!='closed' ORDER BY created_at", (npub,))]
                for item in orders: item.pop("items_json", None)
                payments = [dict(row) for row in conn.execute("SELECT id,order_id,table_number,amount_cents,amount_sats,status,created_at,settled_at FROM payments WHERE merchant_id=? AND status!='acknowledged' ORDER BY created_at", (npub,))]
                bookings = [dict(row) for row in conn.execute("SELECT * FROM bookings WHERE merchant_id=? AND status NOT IN ('closed','cancelled') ORDER BY scheduled_at", (npub,))]
                return self.send_json(200, {"orders": orders, "payments": payments, "bookings": bookings, "server_time": now()})
        if path == "/api/table":
            token = (query.get("token") or [""])[0]
            with LOCK, db() as conn:
                row = conn.execute("SELECT table_number FROM tables WHERE token=?", (token,)).fetchone()
                if not row: return self.send_json(404, {"error": "invalid table token"})
                config = merchant_payload(conn)
                if not config["module_enabled"]: return self.send_json(403, {"error": "module disabled"})
                return self.send_json(200, {"merchant": config["name"], "table": row["table_number"], "products": [p for p in config["products"] if p["enabled"]]})
        if path == "/api/order":
            token = (query.get("token") or [""])[0]
            order_id = (query.get("id") or [""])[0]
            with LOCK, db() as conn:
                row = conn.execute("""SELECT o.id,o.table_number,o.status,o.updated_at
                    FROM orders o JOIN tables t ON t.merchant_id=o.merchant_id AND t.table_number=o.table_number
                    WHERE o.id=? AND t.token=?""", (order_id, token)).fetchone()
                if not row: return self.send_json(404, {"error": "order not found"})
                return self.send_json(200, dict(row))
        if path == "/api/payment":
            token = (query.get("token") or [""])[0]
            payment_id = (query.get("id") or [""])[0]
            with LOCK, db() as conn:
                row = conn.execute("""SELECT p.* FROM payments p JOIN tables t ON t.merchant_id=p.merchant_id AND t.table_number=p.table_number
                    WHERE p.id=? AND t.token=?""", (payment_id, token)).fetchone()
                if not row: return self.send_json(404, {"error": "payment not found"})
                payment = dict(row)
            if payment["status"] == "pending":
                check = helper({"action": "check", "relays": PAYMENT_RELAYS, "recipient": PAYMENT_RECIPIENT, "pubkey": payment["zap_pubkey"], "since": int(datetime.fromisoformat(payment["created_at"]).timestamp())})
                if check["paid"]:
                    with LOCK, db() as conn:
                        conn.execute("UPDATE payments SET status='paid',receipt_id=?,settled_at=? WHERE id=?", (check["receipt"], now(), payment_id))
                        conn.commit()
                        payment["status"], payment["receipt_id"], payment["settled_at"] = "paid", check["receipt"], now()
            return self.send_json(200, {key: payment[key] for key in ("id", "order_id", "table_number", "amount_cents", "amount_sats", "bolt11", "status", "receipt_id", "created_at", "settled_at")})
        if path == "/napplet/order-to-table/index.html": return self.static(NAPLET_DIST / "index.html", "public,max-age=300")
        if path == "/assets/examples/localpub-venue.webp": return self.redirect("https://justworks.cash/assets/examples/localpub-venue.webp")
        if path == "/nsite": return self.nsite()
        if path == "/business-sso": return self.static(PUBLIC / "business-sso.html")
        if path in ("/pos-sw.js", "/screen-sw.js"):
            return self.static(PUBLIC / path[1:], "no-cache")
        if path in ("/pos.webmanifest", "/screen.webmanifest"):
            return self.app_manifest(path[1:].split(".")[0], query)
        surface_pages = {
            "business": "dashboard.html", "pos": "pos.html", "order": "order.html", "book": "book.html",
            "pay": "portal.html", "payandgo": "portal.html", "screen": "screen.html",
            "gallery": "gallery.html", "events": "portal.html",
        }
        pages = {"/qr-sheet": "qr-sheet.html", "/": surface_pages.get(self.surface(), "dashboard.html"), "/dashboard": "dashboard.html", "/profile": "profile.html", "/stats": "stats.html", "/embed": "shell.html", "/portal": "portal.html", "/gallery": "gallery.html", "/screen": "screen.html", "/order": "order.html", "/book": "book.html", "/pos": "pos.html", "/site": "site.html", "/page": "page.html"}
        if path in pages: return self.static(PUBLIC / pages[path])
        if path.startswith("/assets/"): return self.static(PUBLIC / path.removeprefix("/"), "public,max-age=3600")
        if path.startswith("/fonts/"): return self.static(PUBLIC / path.removeprefix("/"), "public,max-age=86400")
        self.send_error(404)

    def do_POST(self):
        self.session_cookie = self.exchange_cookie = self.authorized_mutation = None
        path, _ = self.route()
        try: body = self.body()
        except Exception as error: return self.send_json(400, {"error": str(error)})
        if path.startswith("/api/business-sso/"):
            if not rollout.enabled(): return self.send_json(404, {"error": "Session exchange is not enabled"})
            with LOCK, db() as conn:
                if not security.rate_allowed(self.client_address[0], "/api/business-owner-login", {}, self.headers):
                    return self.send_json(429, {"error": "Too many exchange requests"})
                try:
                    if path == "/api/business-sso/start":
                        result, self.exchange_cookie = rollout.start(conn, body, self.headers)
                    elif path in {"/api/business-sso/details", "/api/business-sso/approve"}:
                        result = rollout.source_details(conn, body, self.headers, path.endswith("approve"))
                    elif path == "/api/business-sso/complete":
                        result, self.session_cookie = rollout.complete(conn, body, self.headers)
                        self.exchange_cookie = rollout.exchange_cookie()
                    else: return self.send_json(404, {"error": "Unknown session action"})
                    return self.send_json(200, result)
                except (ValueError, KeyError, CookieError):
                    security.audit(conn, "exchange_denied", path, reason="invalid_or_expired_exchange")
                    return self.send_json(403, {"error": "Session exchange could not be verified; log in and try again"})
        self.authorized_mutation = None
        self.session_cookie = None
        if path in {"/api/business-owner-login", "/api/orders", "/api/merchant-orders", "/api/bookings", "/api/payments", "/api/quick-pay"}:
            with LOCK:
                if not security.rate_allowed(self.client_address[0], path, body, self.headers):
                    return self.send_json(429, {"error": "Too many requests; try again shortly"})
        if path in {"/api/business-owner-login", "/api/business-logout", "/api/business-revoke"} and not security.same_origin(self.headers):
            return self.send_json(403, {"error": "Same-origin request required"})
        with LOCK, db() as conn:
            if path in {"/api/business-logout", "/api/business-revoke"}:
                current = security.revoke(conn, self.headers, path.endswith("revoke"))
                security.audit(conn, "logout", path, actor=current["npub"] if current else "")
                self.session_cookie = security.cookie_header()
                return self.send_json(200, {"ok": True})
            allowed, context = security.authorize(conn, "POST", path, body, self.headers)
        if not allowed: return self.send_json(403, {"error": "Merchant session required for this account"})
        self.authorized_mutation = context
        if path == "/api/config":
            with LOCK, db() as conn:
                enabled = 1 if body.get("module_enabled") else 0
                count = max(1, min(100, int(body.get("table_count", 30))))
                legacy = rollout.legacy_merchant(conn)
                conn.execute("UPDATE merchant SET module_enabled=?,table_count=?,updated_at=? WHERE id=?", (enabled, count, now(), legacy))
                products = body.get("products") or []
                for index, product in enumerate(products[:50]):
                    pid = str(product.get("id") or f"item-{index}")[:40]
                    name = str(product.get("name") or "Item")[:80]
                    cents = max(0, min(1000000, int(product.get("price_cents", 0))))
                    conn.execute("INSERT INTO products VALUES (?, ?, ?, ?, 1, ?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,price_cents=excluded.price_cents,sort_order=excluded.sort_order WHERE products.merchant_id=excluded.merchant_id", (pid, legacy, name, cents, index))
                for table in range(1, count + 1): conn.execute("INSERT OR IGNORE INTO tables VALUES (?, ?, ?)", (legacy, table, secrets.token_urlsafe(18)))
                for module_id, value in (body.get("modules") or {}).items():
                    if module_id not in MODULE_DEFAULTS: continue
                    enabled = 1 if value.get("enabled") else 0
                    config = json.dumps(value.get("config") or {}, separators=(",", ":"))
                    conn.execute("UPDATE merchant_modules SET enabled=?,config_json=?,updated_at=? WHERE merchant_id=? AND module_id=?", (enabled, config, now(), legacy, module_id))
                conn.commit()
                return self.send_json(200, merchant_payload(conn, True))
        if path == "/api/business-config":
            npub = str(body.get("npub", "")).strip()
            try:
                place, _ = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"merchant unavailable: {error}"})
            requested = body.get("modules") or {}
            with LOCK, db() as conn:
                merchant_modules(conn, npub)
                for module_id, value in requested.items():
                    if module_id not in MODULE_DEFAULTS or MODULE_DEFAULTS[module_id][1] == "coming-soon": continue
                    conn.execute("UPDATE merchant_modules SET enabled=?,updated_at=? WHERE merchant_id=? AND module_id=?", (1 if value else 0, now(), npub, module_id))
                conn.commit()
                return self.send_json(200, {"ok": True, "modules": merchant_modules(conn, npub)})
        if path == "/api/business-owner-login":
            npub = str(body.get("npub", "")).strip()
            try:
                place, _ = resolve_merchant_npub(npub)
                if not place: raise ValueError("merchant_not_found")
                if os.environ.get("JWB_ALLOW_DEMO_LOGIN") == "1" and security.mode() != "enforce" and str(body.get("ownerIdentifier", "")).strip().lower() == "demo":
                    # Dev shortcut: prefilled demo login resolves locally without remote credentials.
                    return self.send_json(200, {"ok": True, "merchant": place.get("name"), "npub": npub, "dev": True})
                credentials = {key: str(body.get(key, "")).strip() for key in ("ownerKey", "ownerIdentifier", "ownerPassword") if body.get(key)}
                if not credentials.get("ownerKey") and not (credentials.get("ownerIdentifier") and credentials.get("ownerPassword")):
                    raise ValueError("credentials_required")
                result = remote_json(f"https://justworks.cash/api/nsites/{place['slug']}/login", "POST", credentials)
                owner = result.get("nsite", {})
                if owner.get("npub") != npub:
                    raise ValueError("ownership_mismatch")
                with LOCK, db() as conn:
                    self.session_cookie, expires = security.issue(conn, npub, self.headers)
                    rollout.verification(conn, npub, place['slug'])
                    security.audit(conn, "login_success", path, npub, npub)
                return self.send_json(200, {"ok": True, "merchant": owner.get("name") or place.get("name"), "npub": npub, "expires_at": expires})
            except Exception:
                self.session_cookie = None
                with LOCK, db() as conn: security.audit(conn, "login_failure", path, reason="verification_failed")
                return self.send_json(401, {"error": "Just Works login could not be verified"})
        if path == "/api/business-profile":
            npub = str(body.get("npub", "")).strip()
            try:
                place, _ = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                profile = body.get("profile") or {}
                lightning_address = str(profile.get("lud16", "")).strip().lower()
                if not lightning_address or "@" not in lightning_address or lightning_address.endswith("@npub.cash"):
                    return self.send_json(400, {"error": "Enter the merchant’s own Lightning address, not the default npub.cash address"})
                credentials = {key: str(body.get(key, "")).strip() for key in ("ownerKey", "ownerIdentifier", "ownerPassword") if body.get(key)}
                if not credentials.get("ownerKey") and not (credentials.get("ownerIdentifier") and credentials.get("ownerPassword")):
                    return self.send_json(400, {"error": "Enter the Just Works owner login or owner key"})
                if os.environ.get("JWB_ALLOW_DEMO_LOGIN") == "1" and security.mode() != "enforce" and credentials.get("ownerIdentifier", "").lower() == "demo":
                    # Dev shortcut: accept the save locally without republishing the production nsite.
                    dev_profile = {"display_name": str(profile.get("display_name", ""))[:120], "about": str(profile.get("about", ""))[:1200], "picture": str(profile.get("picture", ""))[:1000], "banner": str(profile.get("banner", ""))[:1000], "lud16": lightning_address, "website": place.get("justworks") or ""}
                    return self.send_json(200, {"ok": True, "profile": dev_profile, "published": True, "dev": True})
                update = {
                    **credentials,
                    "name": str(profile.get("display_name", ""))[:120],
                    "note": str(profile.get("about", ""))[:1200],
                    "payment": lightning_address,
                    "avatarImage": str(profile.get("picture", ""))[:1000],
                    "heroImage": str(profile.get("banner", ""))[:1000],
                    "stage": "Business profile updated",
                }
                remote_json(f"https://justworks.cash/api/nsites/{place['slug']}", "PATCH", update)
                published = remote_json(f"https://justworks.cash/api/nsites/{place['slug']}/deploy", "POST", credentials, 90)
                return self.send_json(200, {"ok": True, "profile": published.get("nsite", {}).get("identity", {}).get("profile", {}), "published": True})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"profile unavailable: {error}"})
        if path == "/api/orders":
            token, items = str(body.get("token", "")), body.get("items") or []
            note = str(body.get("note", ""))[:240]
            with LOCK, db() as conn:
                table = conn.execute("SELECT table_number,merchant_id FROM tables WHERE token=?", (token,)).fetchone()
                if not table: return self.send_json(404, {"error": "invalid table token"})
                merchant = conn.execute("SELECT module_enabled FROM merchant WHERE id=?", (table['merchant_id'],)).fetchone()
                if not merchant["module_enabled"]: return self.send_json(403, {"error": "module disabled"})
                clean = []
                for item in items[:20]:
                    product = conn.execute("SELECT id,name,price_cents FROM products WHERE id=? AND merchant_id=? AND enabled=1", (str(item.get("id", "")), table['merchant_id'])).fetchone()
                    qty = max(0, min(20, int(item.get("qty", 0))))
                    if product and qty: clean.append(dict(product) | {"qty": qty})
                if not clean: return self.send_json(400, {"error": "order is empty"})
                oid, timestamp = secrets.token_hex(5), now()
                conn.execute("INSERT INTO orders VALUES (?, ?, ?, ?, ?, 'new', ?, ?)", (oid, table['merchant_id'], table["table_number"], json.dumps(clean), note, timestamp, timestamp))
                conn.commit()
                total_cents = sum(item["price_cents"] * item["qty"] for item in clean)
                return self.send_json(201, {"id": oid, "table": table["table_number"], "status": "new", "total_cents": total_cents})
        if path == "/api/merchant-orders":
            npub = str(body.get("npub", "")).strip()
            request_text = str(body.get("request", "")).strip()[:500]
            guest = str(body.get("guest", "")).strip()[:80]
            try: table_number = max(1, min(999, int(body.get("table", 1))))
            except (TypeError, ValueError): return self.send_json(400, {"error": "enter a valid table number"})
            selected = body.get("items") or []
            if not request_text and not selected: return self.send_json(400, {"error": "add an order or request"})
            try:
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except Exception as error: return self.send_json(502, {"error": f"merchant unavailable: {error}"})
            items = [{"id": str(item.get("id", "item"))[:40], "name": str(item.get("name", "Item"))[:80], "qty": max(1, min(20, int(item.get("qty", 1)))), "price_cents": 0} for item in selected[:30]]
            if request_text: items.append({"id": "request", "name": request_text, "qty": 1, "price_cents": 0})
            oid, timestamp = secrets.token_hex(5), now()
            note = f"Guest: {guest}" if guest else ""
            with LOCK, db() as conn:
                conn.execute("INSERT INTO orders VALUES (?,?,?,?,?,'new',?,?)", (oid, npub, table_number, json.dumps(items), note, timestamp, timestamp))
                conn.commit()
            return self.send_json(201, {"id": oid, "merchant": nsite.get("name") or place.get("name"), "table": table_number, "status": "new", "created_at": timestamp})
        if path == "/api/bookings":
            npub = str(body.get("npub", "")).strip()
            name = str(body.get("name", "")).strip()[:80]
            notes = str(body.get("notes", "")).strip()[:240]
            mode = str(body.get("mode", "later"))
            try: guests = max(1, min(20, int(body.get("guests", 2))))
            except (TypeError, ValueError): return self.send_json(400, {"error": "enter a valid guest count"})
            if not name: return self.send_json(400, {"error": "tell the merchant who is booking"})
            if mode not in {"now", "later"}: return self.send_json(400, {"error": "choose now or later"})
            try:
                scheduled = datetime.fromisoformat(str(body.get("scheduled_at", "")).replace("Z", "+00:00"))
                if scheduled.tzinfo is None: scheduled = scheduled.replace(tzinfo=timezone.utc)
                current = datetime.now(timezone.utc)
                if scheduled < current.replace(microsecond=0) and (current - scheduled).total_seconds() > 120: return self.send_json(400, {"error": "choose a future time"})
                if scheduled < current: scheduled = current
                scheduled_at = scheduled.astimezone(timezone.utc).isoformat(timespec="seconds")
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
            except ValueError as error: return self.send_json(400, {"error": str(error) or "choose a valid time"})
            except Exception as error: return self.send_json(502, {"error": f"merchant unavailable: {error}"})
            booking_id, timestamp = secrets.token_hex(6), now()
            with LOCK, db() as conn:
                conn.execute("INSERT INTO bookings VALUES (?,?,?,?,?,?,?,'new',?,?)", (booking_id, npub, name, guests, scheduled_at, mode, notes, timestamp, timestamp))
                conn.commit()
            return self.send_json(201, {"id": booking_id, "merchant": nsite.get("name") or place.get("name"), "name": name, "guests": guests, "scheduled_at": scheduled_at, "status": "new"})
        if path == "/api/quick-pay":
            npub = str(body.get("npub", "")).strip()
            try:
                amount_cents = int(body.get("amount_cents", 0))
                if amount_cents < 1 or amount_cents > 10_000_000:
                    raise ValueError("Enter an amount between €0.01 and €100,000")
                place, nsite = resolve_merchant_npub(npub)
                if not place: return self.send_json(404, {"error": "merchant npub not found"})
                lightning_address = str(nsite.get("identity", {}).get("profile", {}).get("lud16") or "").strip()
                if not lightning_address: raise ValueError("This merchant has no Lightning address")
                prices = fetch_json("https://mempool.space/api/v1/prices")
                amount_sats = max(1, round(amount_cents * 1_000_000 / int(prices["EUR"])))
                merchant_name = nsite.get("name") or place.get("name")
                invoice = lightning_invoice(lightning_address, amount_sats * 1000, str(body.get("comment", "")))
                payment_id, created_at = secrets.token_hex(16), now()
                with LOCK, db() as conn:
                    conn.execute("INSERT INTO quick_payments VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
                                 (payment_id, npub, merchant_name, amount_cents, amount_sats, invoice["bolt11"],
                                  invoice["zap_pubkey"], invoice["recipient"], "pending", None, created_at, None))
                    conn.commit()
                qr_svg = helper({"action": "qr", "text": invoice["bolt11"]})["svg"]
                return self.send_json(201, {"id": payment_id, "merchant": merchant_name, "amount_cents": amount_cents,
                    "amount_sats": amount_sats, "bolt11": invoice["bolt11"], "qr_svg": qr_svg,
                    "status": "pending", "verifiable": invoice["verifiable"], "created_at": created_at})
            except ValueError as error: return self.send_json(400, {"error": str(error)})
            except HTTPError as error: return self.send_json(502, {"error": f"Lightning provider rejected the request ({error.code})"})
            except Exception as error: return self.send_json(502, {"error": f"Invoice unavailable: {error}"})
        if path == "/api/payments" and os.environ.get("JWB_ARCHIPELAGO") == "1":
            return self.send_json(503, {"error": "Legacy order checkout is not connected to your wallet. Use the merchant website payment page."})
        if path == "/api/payments":
            token, order_id = str(body.get("token", "")), str(body.get("order_id", ""))
            with LOCK, db() as conn:
                row = conn.execute("""SELECT o.*,t.token FROM orders o JOIN tables t ON t.merchant_id=o.merchant_id AND t.table_number=o.table_number
                    WHERE o.id=? AND t.token=?""", (order_id, token)).fetchone()
                if not row: return self.send_json(404, {"error": "order not found"})
                existing = conn.execute("SELECT * FROM payments WHERE order_id=? ORDER BY created_at DESC LIMIT 1", (order_id,)).fetchone()
                if existing and existing["status"] == "pending": return self.send_json(200, dict(existing))
                items = json.loads(row["items_json"])
                amount_cents = sum(item["price_cents"] * item["qty"] for item in items)
            prices = fetch_json("https://mempool.space/api/v1/prices")
            amount_sats = max(1, round(amount_cents * 1_000_000 / int(prices["EUR"])))
            created_at = now()
            zap = helper({"action": "create", "lnurlUrl": LNURL_URL, "relays": PAYMENT_RELAYS, "amountMsat": amount_sats * 1000, "recipient": PAYMENT_RECIPIENT, "table": row["table_number"]})
            params = urlencode({"amount": amount_sats * 1000, "nostr": json.dumps(zap["event"], separators=(",", ":")), "lnurl": zap["lnurl"], "comment": f"Just Works Table {row['table_number']}"})
            invoice = fetch_json(f"{LNURL_CALLBACK}?{params}")
            if invoice.get("status") == "ERROR" or not invoice.get("pr"): return self.send_json(502, {"error": invoice.get("reason", "invoice unavailable")})
            payment_id = secrets.token_hex(6)
            with LOCK, db() as conn:
                conn.execute("INSERT INTO payments VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", (payment_id, order_id, row["merchant_id"], row["table_number"], amount_cents, amount_sats, invoice["pr"], zap["pubkey"], "pending", None, created_at, None))
                conn.commit()
            return self.send_json(201, {"id": payment_id, "order_id": order_id, "table_number": row["table_number"], "amount_cents": amount_cents, "amount_sats": amount_sats, "bolt11": invoice["pr"], "status": "pending"})
        if path.startswith("/api/orders/") and path.endswith("/status"):
            oid = path.split("/")[3]
            status = str(body.get("status", ""))
            if status not in {"new", "accepted", "delivered", "closed"}: return self.send_json(400, {"error": "invalid status"})
            with LOCK, db() as conn:
                changed = conn.execute("UPDATE orders SET status=?,updated_at=? WHERE id=?", (status, now(), oid)).rowcount
                conn.commit()
                return self.send_json(200 if changed else 404, {"ok": bool(changed), "status": status})
        if path.startswith("/api/payments/") and path.endswith("/acknowledge"):
            payment_id = path.split("/")[3]
            with LOCK, db() as conn:
                changed = conn.execute("UPDATE payments SET status='acknowledged' WHERE id=? AND status='paid'", (payment_id,)).rowcount
                conn.commit()
                return self.send_json(200 if changed else 404, {"ok": bool(changed), "status": "acknowledged"})
        if path.startswith("/api/bookings/") and path.endswith("/status"):
            booking_id = path.split("/")[3]
            status = str(body.get("status", ""))
            if status not in {"new", "confirmed", "seated", "closed", "cancelled"}: return self.send_json(400, {"error": "invalid status"})
            with LOCK, db() as conn:
                changed = conn.execute("UPDATE bookings SET status=?,updated_at=? WHERE id=?", (status, now(), booking_id)).rowcount
                conn.commit()
                return self.send_json(200 if changed else 404, {"ok": bool(changed), "status": status})
        self.send_error(404)


if __name__ == "__main__":
    security.mode()
    rollout.policy()
    rollout.origins()
    with db(): pass
    print(f"JustWorks Business listening on http://{HOST}:{PORT}{BASE}/")
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()
