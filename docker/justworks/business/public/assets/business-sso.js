// Business-only session transfer. Identity verification remains with Core/Keys/Login.
export async function post(path, body) {
  const response = await fetch(path, {method:'POST', credentials:'same-origin', cache:'no-store', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body)});
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || 'Please log in and try again.');
  return result;
}

export async function transferSession(npub, status) {
  const popup = window.open('about:blank', 'justworks-session', 'popup,width=560,height=680');
  if (!popup) throw new Error('Allow the login window, or use the login form below.');
  let remove = () => {};
  try {
    status.textContent = 'Confirm access in the Business login window…';
    const pending = await post('/api/business-sso/start', {npub, return_path:location.pathname + location.search});
    await new Promise((resolve, reject) => {
      let busy = false;
      const timeout = setTimeout(() => { remove(); reject(new Error('Login window expired. Please try again.')); }, 180000);
      const onMessage = async event => {
        if (busy || event.origin !== pending.source_origin || event.source !== popup || event.data?.type !== 'justworks-session' || event.data.request_id !== pending.request_id) return;
        busy = true;
        try {
          await post('/api/business-sso/complete', {request_id:pending.request_id, code:event.data.code});
          remove(); resolve();
        } catch (error) { remove(); reject(error); }
      };
      remove = () => { clearTimeout(timeout); window.removeEventListener('message', onMessage); };
      window.addEventListener('message', onMessage);
      popup.location = pending.authorize_url;
    });
    status.textContent = 'Business session connected.';
    // Keep the existing URL and fragment; no server-provided redirect is followed.
    location.reload();
  } finally { remove(); if (!popup.closed) popup.close(); }
}

if (location.pathname.endsWith('/business-sso')) {
  const requestId = location.hash.slice(1);
  history.replaceState(null, '', location.pathname + location.search);
  const status = document.querySelector('#exchange-status');
  const destination = document.querySelector('#exchange-destination');
  const approve = document.querySelector('#exchange-approve');
  async function details() {
    approve.disabled = true;
    try {
      const result = await post('/api/business-sso/details', {request_id:requestId});
      destination.textContent = new URL(result.audience).hostname;
      status.textContent = 'Your verified Business login will be used for this site only.';
      approve.disabled = !window.opener;
      if (!window.opener) status.textContent = 'Start session transfer from the site you want to open.';
    } catch (error) { status.textContent = 'Use Login above, then confirm access. ' + error.message; }
  }
  approve.addEventListener('click', async () => {
    approve.disabled = true;
    try {
      const result = await post('/api/business-sso/approve', {request_id:requestId});
      window.opener.postMessage({type:'justworks-session', request_id:requestId, code:result.code}, result.audience);
      status.textContent = 'Access confirmed. Return to your original window.';
    } catch (error) { status.textContent = error.message; }
  });
  document.querySelector('#exchange-cancel').onclick = () => window.close();
  window.addEventListener('jw-session-changed', details);
  details();
}
