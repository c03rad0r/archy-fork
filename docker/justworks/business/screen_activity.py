"""Small public signage projection. No staff data, customer text or payment proofs."""
import json
import threading
import time
from collections import OrderedDict
from datetime import datetime, timezone

TTL = 120
_contexts = OrderedDict()
_lock = threading.Lock()
_workers = threading.BoundedSemaphore(2)


def timestamp(value):
    try:
        parsed = datetime.fromisoformat(str(value).replace('Z', '+00:00'))
        if parsed.tzinfo is None:
            return None
        return parsed.timestamp()
    except (ValueError, TypeError, OverflowError):
        return None


def iso(value):
    return datetime.fromtimestamp(value, timezone.utc).isoformat(timespec='seconds')


def configure(npub, nsite):
    """Only called with server-resolved Core metadata, never client-supplied labels."""
    sections = nsite.get('page', {}).get('sections', [])
    menu = next((s for s in sections if s.get('type') == 'menu' and s.get('enabled')), {})
    catalog = {str(item.get('id') or f'item-{i}')[:40]: str(item.get('title') or item.get('name') or 'Item')[:80]
               for i, item in enumerate(menu.get('items', [])[:50])}
    identity = nsite.get('identity', {})
    lud16 = str(identity.get('profile', {}).get('lud16') or identity.get('npubCashAddress') or f'{npub}@npub.cash').strip()
    with _lock:
        context = _contexts.setdefault(npub, {'tips': [], 'checked': 0, 'busy': False, 'available': False})
        if context.get('lud16') != lud16:
            context.update(tips=[], checked=0, available=False)
        context.update(catalog=catalog, lud16=lud16)
        _contexts.move_to_end(npub)
        while len(_contexts) > 64:
            _contexts.popitem(last=False)


def tips(npub, load, current):
    """Network work stays off the polling path; bounded concurrency/cache, fail closed."""
    with _lock:
        context = _contexts.get(npub)
        if not context:
            return [], False
        if not context['busy'] and time.monotonic() - context['checked'] >= 15 and _workers.acquire(blocking=False):
            context['busy'] = True
            lud16 = context['lud16']

            def refresh():
                try:
                    result = load(npub, lud16)
                    with _lock:
                        if _contexts.get(npub) is context and context['lud16'] == lud16:
                            context.update(tips=result[:30], available=True)
                except Exception:
                    with _lock:
                        context.update(tips=[], available=False)
                finally:
                    with _lock:
                        context.update(checked=time.monotonic(), busy=False)
                    _workers.release()

            threading.Thread(target=refresh, daemon=True).start()
        return list(context['tips']), context['available']


def projection(conn, npub, load_tips, current=None):
    current = time.time() if current is None else current
    cutoff = iso(current - TTL)
    events = []

    def event(kind, stamp, **fields):
        occurred = timestamp(stamp)
        if occurred is not None and current - TTL < occurred <= current:
            events.append({'type': kind, 'occurred_at': iso(occurred), 'expires_at': iso(occurred + TTL), **fields})

    with _lock:
        catalog = dict(_contexts.get(npub, {}).get('catalog', {}))
    catalog.update({row['id']: row['name'][:80] for row in conn.execute(
        'SELECT id,name FROM products WHERE merchant_id=? AND enabled=1', (npub,))})
    for row in conn.execute("SELECT table_number,items_json,status,created_at FROM orders WHERE merchant_id=? AND status IN ('new','accepted') AND created_at>? ORDER BY created_at DESC LIMIT 30", (npub, cutoff)):
        try:
            raw = json.loads(row['items_json'])
            raw = raw if isinstance(raw, list) else []
        except (ValueError, TypeError):
            raw = []
        items = []
        for item in raw[:30]:
            if not isinstance(item, dict) or not isinstance(item.get('id'), str) or item['id'] == 'request' or item['id'] not in catalog:
                continue
            qty = item.get('qty')
            if isinstance(qty, int) and not isinstance(qty, bool) and 0 < qty <= 20:
                items.append({'name': catalog[item['id']], 'qty': qty})
        event('order', row['created_at'], table=row['table_number'], items=items[:4], status=row['status'])
    for row in conn.execute("SELECT amount_cents,amount_sats,settled_at FROM quick_payments WHERE merchant_id=? AND status='paid' AND settled_at>? ORDER BY settled_at DESC LIMIT 30", (npub, cutoff)):
        event('justpay', row['settled_at'], amount_cents=row['amount_cents'], amount_sats=row['amount_sats'])
    recent_tips, available = tips(npub, load_tips, current)
    for tip in recent_tips:
        # Explicit allowlist even for our own helper; no receipt ids or payer identities.
        sats = tip.get('sats')
        if isinstance(sats, int) and not isinstance(sats, bool) and sats > 0:
            event('tip', tip.get('occurred_at'), amount_sats=sats)
    events.sort(key=lambda item: item['occurred_at'], reverse=True)
    return {'events': events[:30], 'server_time': iso(current), 'ttl_seconds': TTL, 'tips_available': available}
