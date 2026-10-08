import { qrImageUrl } from './module-qr.js?v=3';
import { businessUrl, sheetDestinations, validNpub } from './business-destinations.js?v=1';

const status = document.querySelector('#status');
const sheet = document.querySelector('#sheet');
const print = document.querySelector('#print');
const labels = document.querySelector('#labels');
const npub = new URLSearchParams(location.search).get('npub')?.trim();
const fail = message => {
  status.textContent = message;
  status.classList.add('error');
  print.disabled = true;
};

async function openSheet() {
  if (!validNpub(npub)) throw new Error('Open this QR sheet from your Business dashboard with a valid merchant npub.');
  // Do not propagate unrelated URL parameters or fragments into links or prints.
  history.replaceState(null, '', `${location.pathname}?npub=${encodeURIComponent(npub)}`);
  document.querySelector('#back').href = businessUrl(npub);
  const response = await fetch(`/api/business?npub=${encodeURIComponent(npub)}`, {cache: 'no-store'});
  if (!response.ok) throw new Error('The merchant could not be loaded. Return to Business or reload to try again.');
  const data = await response.json();
  if (data.merchant?.npub !== npub) throw new Error('Merchant identity could not be verified. Open the sheet again from Business.');
  const destinations = sheetDestinations(data, npub);
  document.title = `${data.merchant.name} · QR in DIN A4 · Just Works`;
  document.querySelector('#merchant-name').textContent = data.merchant.name;
  document.querySelector('#label-count').textContent = `${destinations.length} merchant links · Print & cut`;
  const loading = destinations.map(({id, title, url}) => {
    const target = new URL(url, location.origin);
    if (!['https:', 'http:'].includes(target.protocol) || target.username || target.password) throw new Error('A merchant link is unavailable. Return to Business to check the links.');
    const card = document.createElement('article');
    card.className = 'qr-label';
    card.dataset.destination = id;
    const name = document.createElement('p');
    name.className = 'label-merchant';
    name.textContent = data.merchant.name;
    const heading = document.createElement('h2');
    heading.textContent = title;
    const anchor = document.createElement('a');
    anchor.href = target.href;
    anchor.setAttribute('aria-label', `Open ${title}`);
    const image = document.createElement('img');
    image.alt = `${title} QR code for ${data.merchant.name}`;
    image.width = image.height = 320;
    const ready = new Promise((resolve, reject) => {
      image.onload = resolve;
      image.onerror = () => reject(new Error('Some QR codes could not load. Reload this page before printing.'));
    });
    image.src = qrImageUrl(target.href);
    anchor.append(image);
    const caption = document.createElement('p');
    caption.className = 'label-caption';
    caption.textContent = ['business', 'pos', 'stats'].includes(id) ? 'Team access · Just Works' : 'Scan to open · Just Works';
    card.append(name, heading, anchor, caption);
    labels.append(card);
    return ready;
  });
  sheet.hidden = false;
  await Promise.all(loading);
  await document.fonts.ready;
  status.textContent = 'Ready to print. All QR codes loaded.';
  print.disabled = false;
  sheet.dataset.ready = 'true';
}
print.addEventListener('click', () => { if (!print.disabled) window.print(); });
openSheet().catch(fail);
