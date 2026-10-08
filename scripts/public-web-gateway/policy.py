#!/usr/bin/env python3
"""Local frps admission plugin. Reload enrollments for every request.

The enrollment file is private operator configuration, never a public catalogue.
Transport must require TLS; the node pins the gateway CA. No bearer value is
logged. An unavailable/malformed policy rejects requests, including heartbeats.
"""
import argparse
import hashlib
import hmac
import json
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

OPS = {'Login', 'NewProxy', 'Ping', 'NewWorkConn', 'NewUserConn'}
NAME = re.compile(r'[a-z0-9][a-z0-9-]{0,47}\Z')
DOMAIN = re.compile(r'(?=.{1,253}\Z)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}\Z')


def authorize(op, content, enrollments):
    if op not in OPS or not isinstance(content, dict):
        return False
    user = content if op == 'Login' else content.get('user')
    if not isinstance(user, dict):
        return False
    name = user.get('user')
    if not isinstance(name, str) or not NAME.fullmatch(name):
        return False
    entry = enrollments.get(name)
    if not isinstance(entry, dict) or entry.get('enabled') is not True:
        return False
    metas = user.get('metas', {})
    token = metas.get('enrollment_token') if isinstance(metas, dict) else None
    expected = entry.get('token_sha256')
    if not isinstance(token, str) or not 32 <= len(token) <= 256:
        return False
    if not isinstance(expected, str) or not re.fullmatch('[a-f0-9]{64}', expected):
        return False
    if not hmac.compare_digest(hashlib.sha256(token.encode()).hexdigest(), expected):
        return False
    domains = entry.get('domains')
    if not isinstance(domains, list) or not domains or len(domains) > 32:
        return False
    if any(not isinstance(d, str) or not DOMAIN.fullmatch(d) for d in domains):
        return False
    if op in {'NewProxy', 'NewUserConn'}:
        # frpc prefixes proxy names with its configured user.
        proxy = content.get('proxy_name', '')
        if not isinstance(proxy, str) or not proxy.startswith(name + '.'):
            return False
        if not NAME.fullmatch(proxy[len(name) + 1:]):
            return False
        if content.get('proxy_type') != 'https':
            return False
    if op == 'NewProxy':
        requested = content.get('custom_domains')
        if not isinstance(requested, list) or len(requested) != 1 or requested[0] not in domains:
            return False
        # No arbitrary TCP ports, wildcard subdomains, shared groups or routing
        # rewrites. TLS terminates on the node; gateway only forwards SNI.
        if any(content.get(k) for k in ('remote_port', 'subdomain', 'group', 'group_key', 'locations', 'host_header_rewrite', 'headers', 'http_user', 'http_pwd', 'multiplexer')):
            return False
    return True


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        accepted = False
        try:
            self.connection.settimeout(3)
            url = urlsplit(self.path)
            query = parse_qs(url.query, strict_parsing=True)
            size = int(self.headers.get('Content-Length', '0'))
            if url.path != '/handler' or query.get('version') != ['0.1.0'] or len(query.get('op', [])) != 1 or not 0 < size <= 65536:
                raise ValueError('Invalid request')
            if self.headers.get('Transfer-Encoding'):
                raise ValueError('Streaming request unsupported')
            config = self.server.policy_path
            if config.stat().st_mode & 0o077:
                raise ValueError('Enrollment file must be private')
            raw = config.read_bytes()
            if len(raw) > 1024 * 1024:
                raise ValueError('Oversized policy')
            enrollments = json.loads(raw)
            request = json.loads(self.rfile.read(size))
            accepted = authorize(query['op'][0], request['content'], enrollments)
        except (OSError, ValueError, TypeError, KeyError, AttributeError):
            pass
        body = json.dumps({'reject': not accepted, 'unchange': True, 'reject_reason': '' if accepted else 'Enrollment or route is not authorized'}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--enrollments', type=Path, required=True)
    parser.add_argument('--port', type=int, default=17700)
    args = parser.parse_args()
    server = ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
    server.policy_path = args.enrollments
    server.serve_forever()


if __name__ == '__main__':
    main()
