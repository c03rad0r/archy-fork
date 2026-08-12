/**
 * NIP-07 Nostr Provider Shim — Archipelago
 *
 * Provides window.nostr (NIP-07) for iframe apps.
 * Auto sign-in: does NIP-98 auth directly then reloads so the app
 * picks up the valid session. Shows a loading overlay during auth.
 */
(function () {
  'use strict';
  if (window.__archipelagoNostr) return;
  window.__archipelagoNostr = true;
  if (window === window.top) return;

  var pending = {}, nextId = 1;

  function request(method, params) {
    return new Promise(function (resolve, reject) {
      var id = nextId++;
      pending[id] = { resolve: resolve, reject: reject };
      window.parent.postMessage({ type: 'nostr-request', id: id, method: method, params: params || {} }, '*');
      setTimeout(function () { if (pending[id]) { pending[id].reject(new Error('NIP-07 timeout')); delete pending[id]; } }, 30000);
    });
  }

  window.addEventListener('message', function (e) {
    if (!e.data || e.data.type !== 'nostr-response') return;
    var h = pending[e.data.id]; if (!h) return; delete pending[e.data.id];
    e.data.error ? h.reject(new Error(e.data.error)) : h.resolve(e.data.result);
  });

  window.nostr = {
    getPublicKey: function () { return request('getPublicKey'); },
    signEvent: function (ev) { return request('signEvent', { event: ev }); },
    sign: function (ev) { return request('signEvent', { event: ev }); },
    getRelays: function () { return request('getRelays'); },
    nip04: {
      encrypt: function (pk, pt) { return request('nip04.encrypt', { pubkey: pk, plaintext: pt }); },
      decrypt: function (pk, ct) { return request('nip04.decrypt', { pubkey: pk, ciphertext: ct }); },
    },
    nip44: {
      encrypt: function (pk, pt) { return request('nip44.encrypt', { pubkey: pk, plaintext: pt }); },
      decrypt: function (pk, ct) { return request('nip44.decrypt', { pubkey: pk, ciphertext: ct }); },
    },
  };

  // --- Loading Overlay ---
  var overlay = null;

  function showLoader(message) {
    if (overlay) return;
    overlay = document.createElement('div');
    overlay.id = 'archipelago-auth-overlay';
    overlay.innerHTML =
      '<div style="display:flex;flex-direction:column;align-items:center;gap:16px;">' +
        '<svg width="40" height="40" viewBox="0 0 24 24" fill="none" style="animation:archy-spin 1s linear infinite">' +
          '<circle cx="12" cy="12" r="10" stroke="rgba(255,255,255,0.2)" stroke-width="3"/>' +
          '<path d="M12 2a10 10 0 019.95 9" stroke="#fb923c" stroke-width="3" stroke-linecap="round"/>' +
        '</svg>' +
        '<div style="color:rgba(255,255,255,0.9);font:500 14px/1.4 -apple-system,system-ui,sans-serif">' + (message || 'Signing in...') + '</div>' +
      '</div>';
    overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,0.7);backdrop-filter:blur(8px);';
    var style = document.createElement('style');
    style.textContent = '@keyframes archy-spin{to{transform:rotate(360deg)}}';
    document.head.appendChild(style);
    document.body.appendChild(overlay);
  }

  function updateLoader(message) {
    if (!overlay) return;
    var txt = overlay.querySelector('div > div');
    if (txt) txt.textContent = message;
  }

  function hideLoader() {
    if (overlay) { overlay.remove(); overlay = null; }
  }

  // --- Direct NIP-98 Auth ---
  var authDone = false;

  function doNip98Auth(pubkey) {
    if (authDone) return;
    authDone = true;

    var apiBase = '/api';
    var healthUrl = window.location.origin + apiBase + '/nostr-auth/health';
    var sessionUrl = window.location.origin + apiBase + '/auth/nostr/session';

    // 1. Check if API backend is reachable (3s timeout)
    var hc = new AbortController();
    var ht = setTimeout(function () { hc.abort(); }, 3000);

    fetch(healthUrl, { signal: hc.signal }).then(function (r) {
      clearTimeout(ht);
      if (!r.ok) throw new Error('Health ' + r.status);

      // 2. API is up — show loader and do NIP-98
      showLoader('Signing in with Nostr...');
      var now = Math.floor(Date.now() / 1000);
      var event = {
        kind: 27235, created_at: now, content: '', pubkey: pubkey,
        tags: [['u', sessionUrl], ['method', 'POST']]
      };
      console.log('[nostr-provider] NIP-98: signing for', sessionUrl);
      return window.nostr.signEvent(event);

    }).then(function (signed) {
      updateLoader('Creating session...');
      var ac = new AbortController();
      setTimeout(function () { ac.abort(); }, 10000);
      return fetch(sessionUrl, {
        method: 'POST',
        headers: { 'Authorization': 'Nostr ' + btoa(JSON.stringify(signed)) },
        signal: ac.signal
      });

    }).then(function (res) {
      console.log('[nostr-provider] NIP-98: response', res.status);
      if (!res.ok) throw new Error('Auth failed: ' + res.status);
      return res.json();

    }).then(function (data) {
      if (data.accessToken) {
        sessionStorage.setItem('nostr_token', data.accessToken);
        sessionStorage.setItem('nostr_pubkey', pubkey);
        if (data.refreshToken) sessionStorage.setItem('refresh_token', data.refreshToken);
        updateLoader('Signed in! Loading...');
        console.log('[nostr-provider] NIP-98: success, reloading...');
        setTimeout(function () { window.location.reload(); }, 400);
      } else {
        hideLoader(); authDone = false;
      }

    }).catch(function (err) {
      hideLoader(); authDone = false;
      var msg = err.message || String(err);
      if (msg.indexOf('abort') > -1) msg = 'API timeout';
      console.warn('[nostr-provider] NIP-98 skipped:', msg);
    });
  }

  // Listen for identity from parent Archipelago frame
  window.addEventListener('message', function (e) {
    if (!e.data || e.data.type !== 'archipelago:identity') return;
    var pk = e.data.nostr_pubkey;
    console.log('[nostr-provider] Identity received:', pk ? pk.slice(0, 12) + '...' : 'none');
    if (!pk) return;

    // Skip if already signed in with a real token (not mock)
    try {
      var token = sessionStorage.getItem('nostr_token');
      if (token && token.indexOf('mock-') === -1) {
        console.log('[nostr-provider] Already signed in with real token');
        return;
      }
    } catch (x) {}

    setTimeout(function () { doNip98Auth(pk); }, 1500);
  });
})();
