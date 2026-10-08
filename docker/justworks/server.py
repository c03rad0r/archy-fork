"""Small fixed-origin adapter to Just Works Core; no credentials are stored."""
import json
import mimetypes
import os
import re
from payments import connect_payment, PublishUnverified
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlparse
from urllib.request import build_opener, HTTPRedirectHandler, Request
import http.client

PUBLIC = Path(__file__).resolve().parent / "public"
CORE = "https://justworks.cash"
MAX_RESPONSE = 2 * 1024 * 1024


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def site_id(value):
    value = value.strip()
    if value.startswith("https://"):
        url = urlparse(value)
        if url.netloc != "justworks.cash" or url.query or url.fragment:
            raise ValueError("Use the public justworks.cash page link, without extra parameters.")
        value = url.path.strip("/")
    if value.startswith(("nsec1", "ncryptsec1")) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", value) or len(value) > 160:
        raise ValueError("Enter your Just Works page link or page name.")
    return value


def read_core(path, timeout=10):
    request = Request(f"{CORE}{path}", headers={"Accept": "application/json", "User-Agent": "Archipelago-JustWorks/0.1"})
    with build_opener(NoRedirect).open(request, timeout=timeout) as response:
        raw = response.read(MAX_RESPONSE + 1)
    if len(raw) > MAX_RESPONSE:
        raise ValueError("The site response was too large. Try again later.")
    return json.loads(raw)


def read_site(value):
    locator = site_id(value)
    record = read_core(f"/api/nsites/{locator}").get("nsite", {})
    npub = record.get("npub", "")
    slug = site_id(record.get("id", ""))
    if (locator != slug and locator != npub) or not re.fullmatch(r"npub1[023456789acdefghjklmnpqrstuvwxyz]{58}", npub):
        raise ValueError("Just Works returned an incomplete site. Open its editor and try again.")
    if record.get("nsite", {}).get("deploymentStatus") != "published":
        raise ValueError("This site is not published yet. Publish it in the Just Works editor, then check again.")
    business_available = True  # Isolated Business resolves the same published Core record.
    # Deliberately do not relay owner material, arbitrary URLs, or upstream HTML.
    return {"id": slug, "name": str(record.get("name") or slug)[:160], "npub": npub,
            "published": True, "url": f"{CORE}/{slug}",
            "editorUrl": f"{CORE}/cms?id={slug}",
            "businessUrl": f"/business-demo/?npub={npub}",
            "businessAvailable": business_available}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # Avoid recording site queries and app-gate tokens.

    def send(self, status, body, content_type="application/json; charset=utf-8"):
        if not isinstance(body, bytes):
            body = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "strict-origin-when-cross-origin")
        self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-src 'self' https://justworks.cash https://business.justworks.cash; object-src 'none'; base-uri 'none'; form-action 'self'")
        self.end_headers()
        self.wfile.write(body)

    def proxy_business(self):
        try: size = int(self.headers.get("Content-Length", "0"))
        except ValueError: return self.send(400, {"error":"Invalid request size"})
        if size < 0 or size > 1024 * 1024:
            return self.send(413, {"error":"Request too large"})
        if self.command != "GET":
            origin = urlparse(self.headers.get("Origin", ""))
            if origin.scheme not in ("http", "https") or origin.netloc != self.headers.get("Host"):
                return self.send(403, {"error":"Open Business inside Archipelago"})
        headers = {key:self.headers[key] for key in ("Host", "Origin", "Cookie", "Content-Type", "Sec-Fetch-Site") if key in self.headers}
        conn = http.client.HTTPConnection("127.0.0.1",4310,timeout=100)
        try:
            conn.request(self.command,self.path,body=self.rfile.read(size) if size else None,headers=headers)
            response=conn.getresponse()
            body=response.read(8*1024*1024+1)
            if len(body)>8*1024*1024: return self.send(502,{"error":"Business response too large"})
            self.send_response(response.status)
            for key,value in response.getheaders():
                if key.lower() in {"content-type","set-cookie","cache-control","x-content-type-options"}: self.send_header(key,value)
            self.send_header("Content-Length",str(len(body)))
            self.end_headers(); self.wfile.write(body)
        except (OSError, http.client.HTTPException): self.send(502,{"error":"Business tools are unavailable. Try again shortly."})
        finally: conn.close()

    def do_POST(self):
        if self.path.startswith("/business-demo/"): return self.proxy_business()
        if self.path != "/api/payment-connect":
            return self.send(404, {"error": "Not found"})
        origin = urlparse(self.headers.get("Origin", ""))
        if origin.scheme not in ("http", "https") or origin.netloc != self.headers.get("Host") or origin.path or origin.query or origin.fragment:
            return self.send(403, {"error": "Open setup from your Archipelago dashboard."})
        if self.headers.get("Content-Type", "").split(";")[0] != "application/json":
            return self.send(415, {"error": "JSON required"})
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if size < 1 or size > 8192:
                return self.send(413, {"error": "Invalid request size"})
            body = json.loads(self.rfile.read(size))
            return self.send(200, connect_payment(body))
        except PublishUnverified:
            return self.send(502, {"error": "The address may be saved, but publication could not be verified. Retry to publish and check again.", "verified": False})
        except HTTPError as error:
            return self.send(401 if error.code in (401, 403) else 502, {"error": "Just Works could not authorize this update. Check your owner login and website, then retry."})
        except (ValueError, TypeError, AttributeError):
            return self.send(422, {"error": "Check the connected website, native wallet address and owner sign-in."})
        except (OSError, URLError):
            return self.send(502, {"error": "Cannot reach Just Works. Try again shortly."})

    def do_GET(self):
        path = urlparse(self.path)
        if path.path.startswith("/business-demo/"): return self.proxy_business()
        if path.path == "/healthz":
            return self.send(200, {"ok": True})
        if path.path == "/api/site":
            try:
                return self.send(200, read_site(parse_qs(path.query).get("url", [""])[0]))
            except HTTPError as error:
                message = "That page was not found. Copy its public link from Just Works." if error.code == 404 else "Just Works is unavailable. Your saved link has not changed; try again shortly."
                return self.send(404 if error.code == 404 else 502, {"error": message})
            except (ValueError, TypeError, AttributeError):
                # Validation failures must not expose the remote response body.
                return self.send(422, {"error": "Check your Just Works page link and make sure the site is published, then try again."})
            except (OSError, URLError):
                return self.send(502, {"error": "Cannot reach Just Works. Check your connection and try again."})
        file = (PUBLIC / ("index.html" if path.path == "/" else path.path.lstrip("/"))).resolve()
        if not file.is_relative_to(PUBLIC) or not file.is_file():
            return self.send(404, {"error": "Not found"})
        return self.send(200, file.read_bytes(), mimetypes.guess_type(file.name)[0] or "application/octet-stream")


if __name__ == "__main__":
    ThreadingHTTPServer((os.environ.get("JW_HOST", "0.0.0.0"), int(os.environ.get("JW_PORT", "8340"))), Handler).serve_forever()
