export function qrImageUrl(text) {
  const prefix = location.pathname.startsWith('/business-demo/') ? '/business-demo' : '';
  return `${prefix}/api/qr?text=${encodeURIComponent(text)}`;
}

// QR payloads contain only the public destination and merchant context.
export function moduleQrUrl(destination, npub) {
  const url = new URL(destination, location.origin);
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('Invalid module URL');
  const surface = url.searchParams.get('surface');
  url.username = '';
  url.password = '';
  url.search = '';
  url.hash = '';
  if (surface) url.searchParams.set('surface', surface);
  url.searchParams.set('npub', npub);
  return url.href;
}

const escapeHtml = value => String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));

// Both public links and business modules share the exact same QR control.
// Free-link URLs are already resolved by the API: preserve them verbatim.
export function qrButton(id, name, url, {free = false, hidden = false} = {}) {
  return `<button type="button" class="module-qr" data-qr-trigger="${escapeHtml(id)}" ${free ? 'data-free-qr' : 'data-module-qr'}="${escapeHtml(id)}" data-qr-name="${escapeHtml(name)}" data-qr-url="${escapeHtml(url)}" aria-label="Show ${escapeHtml(name)} QR code" aria-haspopup="dialog" title="Show ${escapeHtml(name)} QR code" ${hidden ? 'hidden' : ''}><img src="${qrImageUrl(url)}" alt="" loading="lazy"><span>QR</span></button>`;
}

export function mountModuleQrs(root) {
  const dialog = document.createElement('dialog');
  dialog.className = 'module-qr-dialog';
  dialog.setAttribute('aria-labelledby', 'module-qr-title');
  dialog.setAttribute('aria-describedby', 'module-qr-description');
  dialog.innerHTML = `<button type="button" class="module-qr-close" aria-label="Close QR code" autofocus>×</button><p class="eyebrow">Quick access</p><h2 id="module-qr-title"></h2><p id="module-qr-description">Scan to open this merchant’s module on your phone.</p><div class="module-qr-image"><img alt=""><p role="status" aria-live="polite"></p></div><a class="module-qr-url" target="_blank" rel="noopener"></a><div class="module-qr-tools"><button type="button" data-qr-copy>Copy link</button><a data-qr-download>Download QR</a><button type="button" data-qr-print>Print QR</button></div><p class="module-qr-feedback" role="status" aria-live="polite"></p>`;
  document.body.append(dialog);
  const title = dialog.querySelector('h2');
  const img = dialog.querySelector('img');
  const status = dialog.querySelector('.module-qr-image [role=status]');
  const urlLink = dialog.querySelector('.module-qr-url');
  const feedback = dialog.querySelector('.module-qr-feedback');
  const download = dialog.querySelector('[data-qr-download]');
  const print = dialog.querySelector('[data-qr-print]');
  img.onload = () => {
    img.hidden = false;
    status.textContent = '';
    download.hidden = false;
    print.disabled = false;
  };
  img.onerror = () => {
    img.hidden = true;
    status.textContent = 'QR code could not load. You can still open or copy the link.';
    download.hidden = true;
    print.disabled = true;
  };
  root.addEventListener('click', (event) => {
    const button = event.target.closest('[data-qr-trigger]');
    if (!button || button.hidden) return;
    const { qrUrl, qrName, qrTrigger } = button.dataset;
    dialog.querySelector('#module-qr-description').textContent = button.hasAttribute('data-free-qr')
      ? 'Scan to open this merchant’s public page on your phone.'
      : 'Scan to open this merchant’s module on your phone.';
    title.textContent = qrName;
    urlLink.href = qrUrl;
    urlLink.textContent = qrUrl;
    feedback.textContent = '';
    img.hidden = true;
    status.textContent = 'Loading QR code…';
    print.disabled = true;
    download.hidden = true;
    img.alt = `QR code for ${qrName}`;
    img.src = qrImageUrl(qrUrl);
    download.href = img.src;
    download.download = `justworks-${qrTrigger}-qr.svg`;
    dialog.showModal();
  });
  dialog.querySelector('.module-qr-close').onclick = () => dialog.close();
  dialog.addEventListener('click', (event) => {
    if (event.target !== dialog) return;
    const bounds = dialog.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) dialog.close();
  });
  // Native modal dialogs restore focus on close. Do not also restore it from
  // the queued `close` event: that can steal focus from the next QR trigger.
  dialog.addEventListener('keydown', (event) => {
    if (event.key !== 'Tab') return;
    const controls = [...dialog.querySelectorAll('button:not(:disabled), a[href]')].filter(node => !node.hidden);
    const first = controls[0], last = controls.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });
  dialog.querySelector('[data-qr-copy]').onclick = async () => {
    try {
      await navigator.clipboard.writeText(urlLink.href);
      feedback.textContent = 'Link copied.';
    } catch {
      feedback.textContent = 'Could not copy automatically. Select and copy the link above.';
    }
  };
  print.onclick = () => window.print();
}
