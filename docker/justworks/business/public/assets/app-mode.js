// Shared display controls; credentials, live records and writes never enter storage.
const app = document.body.dataset.pwaApp;
const standalone = matchMedia('(display-mode: standalone)');
const fullscreen = matchMedia('(display-mode: fullscreen)');
let installed = standalone.matches || fullscreen.matches || navigator.standalone === true;
let installPrompt;
const controls = document.createElement('div');
controls.className = 'app-mode';
controls.setAttribute('aria-label', `${app === 'pos' ? 'POS' : 'Screen'} app controls`);
controls.innerHTML = `<button type="button" class="app-install">Install app</button><button type="button" class="app-fullscreen" aria-pressed="false">Fullscreen</button><span class="app-mode-status" role="status" aria-live="polite"></span><dialog class="app-install-help" aria-labelledby="app-install-title"><h2 id="app-install-title">Install Just Works ${app === 'pos' ? 'POS' : 'Screen'}</h2><p class="app-install-instructions"></p><p>Your merchant link stays with this app. A connection is required for live activity.</p><button type="button" class="app-help-close">Got it</button></dialog>`;
const install = controls.querySelector('.app-install');
const full = controls.querySelector('.app-fullscreen');
const status = controls.querySelector('.app-mode-status');
const help = controls.querySelector('dialog');
const manifest = document.querySelector('link[rel="manifest"]');
const npub = new URLSearchParams(location.search).get('npub');
// Manifest endpoint validates checksum. Never carry login tokens or fragments.
if (npub && /^npub1[023456789acdefghjklmnpqrstuvwxyz]{58}$/.test(npub)) {
  manifest.href = `/${app}.webmanifest?npub=${encodeURIComponent(npub)}`;
}
function syncInstall() {
  install.hidden = installed;
  install.textContent = installPrompt ? 'Install app' : 'Install guide';
}
function syncFullscreen() {
  const active = Boolean(document.fullscreenElement || document.webkitFullscreenElement);
  full.textContent = active ? 'Exit fullscreen' : 'Fullscreen';
  full.setAttribute('aria-pressed', String(active));
  full.title = active ? 'Exit fullscreen (Esc)' : 'Enter fullscreen';
}
function syncConnection() {
  status.textContent = navigator.onLine ? '' : 'Offline · live updates unavailable';
  controls.classList.toggle('is-offline', !navigator.onLine);
}
window.addEventListener('beforeinstallprompt', event => {
  event.preventDefault();
  installPrompt = event;
  syncInstall();
});
window.addEventListener('appinstalled', () => {
  installed = true;
  installPrompt = undefined;
  syncInstall();
  status.textContent = 'App installed';
});
standalone.addEventListener('change', () => {installed = standalone.matches || navigator.standalone === true; syncInstall();});
install.addEventListener('click', async () => {
  if (installPrompt) {
    const prompt = installPrompt;
    installPrompt = undefined;
    install.disabled = true;
    try {
      await prompt.prompt();
      const choice = await prompt.userChoice;
      if (choice.outcome === 'accepted') status.textContent = 'Installation requested';
    } catch {status.textContent = 'Use your browser menu to install this app.';}
    finally {install.disabled = false; syncInstall();}
    return;
  }
  const ios = /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  help.querySelector('.app-install-instructions').textContent = ios
    ? 'In Safari, tap Share → Add to Home Screen, then enable Open as Web App if shown.'
    : 'Open your browser menu and choose Install app or Add to Home Screen. If it is not offered, use a browser that supports web app installation.';
  help.showModal();
});
help.querySelector('.app-help-close').addEventListener('click', () => help.close());
help.addEventListener('click', event => {if (event.target === help) {const r = help.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) help.close();}});
full.addEventListener('click', async () => {
  const active = document.fullscreenElement || document.webkitFullscreenElement;
  const action = active ? (document.exitFullscreen || document.webkitExitFullscreen) : (document.documentElement.requestFullscreen || document.documentElement.webkitRequestFullscreen);
  if (!action) {status.textContent = 'Fullscreen is unavailable in this browser. Install the app for a standalone view.'; return;}
  try {await action.call(active ? document : document.documentElement); syncFullscreen(); status.textContent = '';}
  catch {status.textContent = 'Fullscreen could not open. Try again or use your browser fullscreen command.';}
});
document.addEventListener('fullscreenchange', syncFullscreen);
document.addEventListener('webkitfullscreenchange', syncFullscreen);
document.addEventListener('fullscreenerror', () => {status.textContent = 'Fullscreen is unavailable right now.'; syncFullscreen();});
window.addEventListener('online', syncConnection);
window.addEventListener('offline', syncConnection);
syncInstall(); syncFullscreen(); syncConnection();

// Dedicated production origins only: a localhost multi-surface server must not
// install competing root-scope workers. Tests use separate secure origins.
if (location.hostname === `${app}.justworks.cash` && 'serviceWorker' in navigator) {
  navigator.serviceWorker.register(`/${app}-sw.js`, {scope: '/', updateViaCache: 'none'})
    .catch(() => {install.title = 'Installation may be unavailable; the live website still works.';});
}
export function mountAppControls(host) {
  host.append(controls);
  if (app === 'pos' && 'ResizeObserver' in window) {
    new ResizeObserver(() => document.body.style.setProperty('--app-toolbar-height', `${host.getBoundingClientRect().height}px`)).observe(host);
  }
}
