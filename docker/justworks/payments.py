"""Owner-authorized, fixed-origin payment destination update. No secret storage."""
import json
import re
import threading
from urllib.request import Request, build_opener, HTTPRedirectHandler

CORE = 'https://justworks.cash'
LOCK = threading.Lock()


class PublishUnverified(Exception):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def request_core(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    request = Request(CORE + path, data=data, method=method,
                      headers={'Content-Type': 'application/json', 'Accept': 'application/json',
                               'User-Agent': 'Archipelago-JustWorks/0.1'})
    with build_opener(NoRedirect).open(request, timeout=100 if path.endswith('/deploy') else 20) as response:
        raw = response.read(2 * 1024 * 1024 + 1)
    if len(raw) > 2 * 1024 * 1024:
        raise ValueError('Response too large')
    return json.loads(raw)


def checked_input(body):
    if not isinstance(body, dict):
        raise ValueError('Invalid request')
    slug = body.get('siteId', '')
    address = body.get('address', '')
    if not isinstance(slug, str) or not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', slug) or len(slug) > 160 or slug.startswith(('nsec1', 'ncryptsec1')):
        raise ValueError('Invalid website')
    # This integration connects only the native wallet address returned by the OS.
    if not isinstance(address, str) or not re.fullmatch(r'[a-z0-9._+-]{1,100}@minibits\.cash', address):
        raise ValueError('Invalid native wallet address')
    credentials = body.get('credentials')
    if not isinstance(credentials, dict):
        raise ValueError('Owner sign-in required')
    allowed = ('ownerIdentifier', 'ownerPassword', 'ownerKey', 'password')
    clean = {}
    for key in allowed:
        value = credentials.get(key, '')
        if not isinstance(value, str) or len(value) > 2048:
            raise ValueError('Invalid owner sign-in')
        if value:
            clean[key] = value
    if not clean.get('ownerKey') and not (clean.get('ownerIdentifier') and clean.get('ownerPassword')):
        raise ValueError('Owner sign-in required')
    return slug, address, clean


def destination_matches(record, slug, address):
    return (isinstance(record, dict) and record.get('id') == slug
            and record.get('nsite', {}).get('deploymentStatus') == 'published'
            and record.get('identity', {}).get('profile', {}).get('lud16') == address)


def connect_payment(body):
    slug, address, credentials = checked_input(body)
    if not LOCK.acquire(blocking=False):
        raise ValueError('Another update is in progress. Wait and retry.')
    try:
        # Confirm ownership before attempting a change. Never keep the returned
        # owner session or any recovery material from Core's response.
        owner = request_core('POST', f'/api/nsites/{slug}/login', credentials).get('nsite', {})
        if owner.get('id') != slug:
            raise ValueError('Owner sign-in did not match this website')
        try:
            # Only payment changes; do not replay or overwrite other profile fields.
            request_core('PATCH', f'/api/nsites/{slug}', {**credentials, 'payment': address})
            published = request_core('POST', f'/api/nsites/{slug}/deploy', credentials)
            if not published.get('artifact', {}).get('verified') or not destination_matches(published.get('nsite'), slug, address):
                raise PublishUnverified()
            # This is the public record consumed by the hosted customer tip page.
            public = request_core('GET', f'/api/nsites/{slug}').get('nsite')
            if not destination_matches(public, slug, address):
                raise PublishUnverified()
        except Exception as error:
            raise PublishUnverified('The address may be saved, but publication could not be verified. Retry to publish and check again.') from error
        return {'verified': True, 'siteId': slug, 'address': address,
                'url': f'{CORE}/{slug}'}
    finally:
        credentials.clear()
        LOCK.release()
