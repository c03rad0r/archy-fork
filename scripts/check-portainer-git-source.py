#!/usr/bin/env python3
"""Test Git from Portainer's server context without creating a Source or stack."""
import argparse
import json
import pathlib
import socket
import stat
import urllib.error
import urllib.parse
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def classify(error):
    text = error.lower()
    for category, patterns in (
        ('connection-refused', ('connection refused',)),
        ('dns-failure', ('no such host', 'name resolution', 'server misbehaving')),
        ('timeout', ('timeout', 'timed out', 'deadline exceeded')),
        ('tls-failure', ('x509:', 'certificate', 'tls handshake')),
        ('proxy-or-login-interception', ('text/html', '<html', '<!doctype', 'unexpected content-type', 'invalid pkt-len')),
        ('repository-authentication', ('authentication required', 'authentication failed', 'authorization failed', '401', '403')),
        ('repository-not-found-or-private', ('repository not found', '404')),
    ):
        if any(pattern in text for pattern in patterns):
            return category
    return 'git-error'


def safe_url(value):
    parsed = urllib.parse.urlsplit(value)
    if parsed.scheme not in ('http', 'https') or not parsed.hostname:
        raise ValueError('Use an HTTP(S) URL')
    if parsed.username is not None or parsed.password is not None or parsed.query or parsed.fragment:
        raise ValueError('URLs must not contain credentials, query parameters or fragments')
    return value.rstrip('/')


def private_json(path):
    path = pathlib.Path(path)
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise ValueError('Credential file must be private (chmod 600)')
    return json.loads(path.read_text())


def check(base, repository, credentials, opener=None):
    base, repository = safe_url(base), safe_url(repository)
    # JWT/API keys and Git credentials travel in headers/body, never URLs or logs.
    headers = {'Content-Type': 'application/json'}
    if credentials.get('api_key'):
        headers['X-API-Key'] = credentials['api_key']
    elif credentials.get('jwt'):
        headers['Authorization'] = 'Bearer ' + credentials['jwt']
    else:
        raise ValueError('Credential file needs api_key or jwt')
    payload = {'url': repository, 'tlsSkipVerify': False, 'interval': '5m'}
    if credentials.get('git'):
        payload['authentication'] = credentials['git']
    request = urllib.request.Request(base + '/api/gitops/sources/test',
                                     data=json.dumps(payload).encode(), headers=headers)
    opener = opener or urllib.request.build_opener(NoRedirect())
    try:
        with opener.open(request, timeout=45) as response:
            result = json.load(response)
    except urllib.error.HTTPError as error:
        return {'success': False, 'category': 'portainer-authentication' if error.code in (401, 403) else 'portainer-api-error', 'http_status': error.code}
    except (urllib.error.URLError, TimeoutError, socket.timeout) as error:
        return {'success': False, 'category': 'portainer-api-' + classify(str(error))}
    except (ValueError, UnicodeError):
        return {'success': False, 'category': 'portainer-api-invalid-response'}
    if not isinstance(result, dict) or not isinstance(result.get('success'), bool):
        return {'success': False, 'category': 'portainer-api-invalid-response'}
    return {'success': result['success'], 'category': 'git-refs-readable' if result['success'] else classify(str(result.get('error', '')))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--portainer-url', required=True, help='Reachable Portainer origin, without /api')
    parser.add_argument('--repository-url', required=True, help='The same clone URL entered in Portainer')
    parser.add_argument('--credentials-file', required=True, help='Mode 600 JSON: api_key or jwt; optional git: {username,password}')
    args = parser.parse_args()
    try:
        result = check(args.portainer_url, args.repository_url, private_json(args.credentials_file))
    except (OSError, ValueError):
        parser.exit(2, 'Invalid URL or private credential file; no credentials were printed.\n')
    print(json.dumps(result))
    return 0 if result['success'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
