// Connector for the native OS guide. No Just Works UI is replaced.
const base = new URL('..', import.meta.url).pathname.replace(/\/?$/, '/');
let parentOrigin;
try { parentOrigin = new URL(document.referrer).origin; } catch { /* standalone */ }
if (parentOrigin && new URL(parentOrigin).hostname === location.hostname && new URL(parentOrigin).protocol === location.protocol && window.parent !== window) {
  window.addEventListener('message', async event => {
    if (event.source !== parent || event.origin !== parentOrigin || typeof event.data.requestId !== 'string') return;
    const requestId = event.data.requestId;
    try {
      if (event.data.type === 'justworks:get-connection') {
        const siteId = localStorage.getItem('archipelago-justworks-site-v1') || '';
        let npub = '';
        if (siteId) {
          const response = await fetch(`${base}api/site?url=${encodeURIComponent(siteId)}`, {cache:'no-store', signal:AbortSignal.timeout(12000)});
          const site = await response.json();
          if (response.ok && site.id === siteId) npub = site.npub;
        }
        parent.postMessage({type:'justworks:connection', requestId, siteId, npub}, parentOrigin);
        return;
      }
      if (event.data.type === 'justworks:business-login') {
        const response = await fetch(`${base}business-demo/api/business-owner-login`, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(event.data.payload), signal:AbortSignal.timeout(60000)});
        const result = await response.json();
        parent.postMessage({type:'justworks:business-result', requestId, ok:response.ok && result.ok === true, npub:result.npub, error:result.error}, parentOrigin);
        return;
      }
      if (event.data.type === 'justworks:connect-payment') {
        const response = await fetch(`${base}api/payment-connect`, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(event.data.payload), signal:AbortSignal.timeout(170000)});
        const result = await response.json();
        parent.postMessage({type:'justworks:payment-result', requestId, ...result}, parentOrigin);
        return;
      }
      if (event.data.type !== 'justworks:check-site' || typeof event.data.url !== 'string') return;
      const response = await fetch(`${base}api/site?url=${encodeURIComponent(event.data.url)}`, {cache:'no-store', signal:AbortSignal.timeout(15000)});
      const site = await response.json();
      if (!response.ok || site.published !== true) throw new Error(site.error || 'Publish your website in Just Works, then try again.');
      let remembered = true;
      try { localStorage.setItem('archipelago-justworks-site-v1', site.id); } catch { remembered = false; }
      parent.postMessage({type:'justworks:site-connected', requestId, site:{id:site.id,published:true}, remembered}, parentOrigin);
    } catch (error) {
      parent.postMessage({type:'justworks:site-error', requestId, error:error.message}, parentOrigin);
    }
  });
}
