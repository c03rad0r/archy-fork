import { configureNavigation, mountAppChrome } from './ds/frame.js';
const $ = id => document.getElementById(id);
const base = new URL('..', import.meta.url).pathname.replace(/\/?$/, '/');
const key = 'archipelago-justworks-site-v1';
let connectedSite;
let businessOpen = false;
configureNavigation({brandHref: base, items: [[base, 'Your launcher']]});
mountAppChrome($('app'), {label: 'Archipelago'});
function showLauncher() {
  if (!connectedSite) return;
  $('hosted-surface').hidden = true;
  $('app').hidden = false;
}
function openHosted(url, title) {
  const target = new URL(url, location.href);
  businessOpen = target.origin === location.origin && target.pathname.startsWith('/business-demo/');
  $('switch-label').textContent = businessOpen ? 'Website' : 'Business';
  $('switch-building').toggleAttribute('hidden', businessOpen);
  $('switch-person').toggleAttribute('hidden', !businessOpen);
  const switchLabel = businessOpen ? 'Switch to customer website' : 'Switch to Business tools';
  $('hosted-switch').setAttribute('aria-label', switchLabel);
  $('hosted-switch').title = switchLabel;
  $('hosted-switch').href = businessOpen ? (connectedSite?.editorUrl || 'https://justworks.cash/') : (connectedSite?.businessUrl || '/business-demo/');
  $('app').hidden = true;
  $('hosted-site').src = url;
  $('hosted-site').title = title;
  $('back-to-launcher').hidden = !connectedSite;
  $('hosted-surface').hidden = false;
}
$('hosted-switch').addEventListener('click', event => {
  event.preventDefault();
  openHosted($('hosted-switch').href, businessOpen ? 'Just Works website' : 'Just Works business tools');
});
$('back-to-launcher').addEventListener('click', showLauncher);
$('website-launch').addEventListener('click', event => {
  event.preventDefault();
  openHosted(connectedSite?.editorUrl || 'https://justworks.cash/', 'Just Works website');
});
$('business-launch').addEventListener('click', event => {
  event.preventDefault();
  if (connectedSite) openHosted(connectedSite.businessUrl, 'Just Works business tools');
});
async function refresh() {
  $('business-card').hidden = true;
  try {
    const saved = localStorage.getItem(key);
    if (!saved) return;
    const response = await fetch(`${base}api/site?url=${encodeURIComponent(saved)}`, {cache:'no-store', signal:AbortSignal.timeout(15000)});
    const site = await response.json();
    if (!response.ok || site.published !== true) return;
    connectedSite = site;
    $('website-launch').href = site.editorUrl;
    $('website-launch').textContent = 'Open your website';
    $('business-card').hidden = false;
    $('business-launch').href = site.businessUrl;
    $('business-launch').textContent = 'Open business tools';
    $('business-help').textContent = site.businessAvailable === true ? 'Your merchant dashboard, POS and signage.' : 'Your website is ready. Business tools depend on availability in Just Works.';
    $('connection-title').textContent = site.name;
    $('connection-description').textContent = site.url;
    $('forget').hidden = false;
    showLauncher();
  } catch { /* Keep the original Just Works first screen available. */ }
}
$('forget').addEventListener('click', () => { localStorage.removeItem(key); location.reload(); });
window.addEventListener('storage', () => { if (!connectedSite) void refresh(); });
void refresh();
