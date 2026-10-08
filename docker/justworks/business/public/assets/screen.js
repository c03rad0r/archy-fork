import {mountAppControls} from './app-mode.js?v=1';
import {mountLogo} from './ds/logo.js';
import {npubFrom} from './ds/env.js';

const app = document.querySelector('#screen');
const npub = npubFrom();
const esc = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const apiBase = location.pathname.startsWith('/business-demo') ? '/business-demo' : '';
const ago = stamp => {
  const seconds = Math.max(0, Math.floor((Date.now() - stamp * 1000) / 1000));
  return seconds < 60 ? 'just now' : seconds < 3600 ? `${Math.floor(seconds / 60)}m ago` : `${Math.floor(seconds / 3600)}h ago`;
};

function error(message) {
  app.innerHTML = `<section class="screen-error"><span>⌁</span><p>Just Works Screen</p><h1>${esc(message)}</h1><small>Use screen.justworks.cash/?npub=npub1…</small></section>`;
}

function qrCard(id, label, url) {
  // Only server-resolved public destinations; never forward the screen query or hash.
  const target = new URL(url);
  if (!['https:', 'http:'].includes(target.protocol)) throw new Error('Invalid screen destination');
  return `<a class="screen-qr${id === 'order' ? ' is-current' : ''}" id="qr-${id}" data-screen-qr="${id}" href="${esc(url)}" aria-label="${label} for this merchant"><span>${label}</span><img src="${apiBase}/api/qr?text=${encodeURIComponent(url)}" alt="QR code: ${label}" width="480" height="480"><small>Scan to ${id === 'order' ? 'order' : id === 'justpay' ? 'pay' : 'tip'} ↗</small></a>`;
}

function render(data) {
  const {merchant, images, latest, screen_links: links} = data;
  const safeImages = images || [];
  app.innerHTML = `<section class="stage" data-view="idle">
    <div class="slides" aria-hidden="true">${safeImages.map((src, i) => `<img class="slide ${i === 0 ? 'active' : ''}" src="${esc(src)}" alt="">`).join('')}</div><div class="shade"></div>
    <header><div class="brand"><span data-logo="icon"></span><b>Just Works<small>Live merchant screen</small></b></div><div class="merchant"><strong>${esc(merchant.name)}</strong><small>${esc([merchant.category, merchant.area].filter(Boolean).join(' · '))}</small></div><div class="screen-utilities"><time id="clock" aria-label="Local time"></time><div class="screen-app-controls"></div></div></header>
    <aside class="screen-actions" aria-label="Quick access for this merchant"><div class="qr-intro"><p class="eyebrow">One scan away</p><h2 id="qr-title">Good things<br> start here.</h2><p class="qr-invitation">Order to your table. Pay simply. Leave a little love.</p></div><div class="qr-cards">${qrCard('order', 'Order', links.order)}${qrCard('justpay', 'JustPay', links.justpay)}${qrCard('tip', 'Tip', links.tip)}</div><div class="qr-controls" aria-label="Choose a QR code"><div class="qr-choices">${['Order', 'JustPay', 'Tip'].map((label, i) => `<button type="button" data-qr-choice="${i}" aria-controls="qr-${['order','justpay','tip'][i]}" aria-pressed="${i === 0}" aria-label="Show ${label} QR code"><span>0${i + 1}</span>${label}</button>`).join('')}</div><button class="qr-pause" type="button" aria-pressed="false">Pause rotation</button></div></aside>
    <section class="live-feed" aria-labelledby="live-title"><div class="feed-heading"><div><span class="live-dot" aria-hidden="true"></span><h2 id="live-title">Happening now</h2></div><span class="feed-intro">Orders · Payments · Tips</span></div><div id="activity-items"></div><p class="feed-footnote">Only the last 2 minutes · completed orders leave automatically</p></section>
    <section class="nostr-note" aria-label="Merchant’s Nostr feed"><div class="note-head"><span aria-hidden="true">◉</span><div><strong>Latest on Nostr</strong><small>${latest ? `${ago(latest.created_at)} · signed event` : 'Merchant feed'}</small></div></div><p>${latest ? esc(latest.content.slice(0, 220)) : 'No signed text post from this merchant yet.'}</p><small class="npub">${esc(merchant.npub.slice(0, 18))}…${esc(merchant.npub.slice(-8))}</small></section>
    <section class="identity"><p>Now at</p><h1>${esc(merchant.name)}</h1><div class="progress" aria-hidden="true">${safeImages.map((_, i) => `<i class="${i === 0 ? 'active' : ''}"></i>`).join('')}</div></section>
    <footer><span>Pictures from the merchant’s nsite</span><span class="feed-connection" role="status">Connecting…</span><b>₿ Payments · Orders · Nostr-owned</b></footer><p class="activity-announcement" role="status" aria-live="polite" aria-atomic="true"></p></section>`;
  document.querySelectorAll('[data-logo]').forEach(host => mountLogo(host, {icon: true}));
  document.querySelectorAll('.screen-qr img').forEach(img => img.addEventListener('error', () => {
    img.hidden = true;
    img.closest('a').querySelector('small').textContent = 'QR unavailable · open link ↗';
  }));
  mountAppControls(document.querySelector(".screen-app-controls"));
  startSlideshow(safeImages.length);
  startActivity(startQRTheatre());
}

function startQRTheatre() {
  const stage = document.querySelector('.stage'), actions = document.querySelector('.screen-actions');
  const cards = [...actions.querySelectorAll('.screen-qr')], choices = [...actions.querySelectorAll('[data-qr-choice]')];
  const pause = actions.querySelector('.qr-pause'), motion = matchMedia('(prefers-reduced-motion: reduce)');
  let current = 0, timer, paused = false, active = false;
  const choose = index => {
    current = index;
    cards.forEach((card, i) => card.classList.toggle('is-current', i === current));
    choices.forEach((button, i) => button.setAttribute('aria-pressed', String(i === current)));
  };
  const sync = () => {
    clearTimeout(timer);
    const running = !active && !motion.matches && !paused && !actions.contains(document.activeElement);
    stage.dataset.rotation = running ? 'running' : 'paused';
    if (running) timer = setTimeout(() => {choose((current + 1) % cards.length); sync();}, 12000);
  };
  choices.forEach((button, i) => button.addEventListener('click', () => {choose(i); sync();}));
  pause.addEventListener('click', () => {
    paused = !paused;
    pause.setAttribute('aria-pressed', String(paused));
    pause.textContent = paused ? 'Resume rotation' : 'Pause rotation';
    sync();
  });
  actions.addEventListener('focusin', sync);
  actions.addEventListener('focusout', () => queueMicrotask(sync));
  motion.addEventListener('change', () => {
    if (motion.matches && actions.querySelector('.qr-controls').contains(document.activeElement)) cards[current].focus();
    // When leaving reduced motion, retain whichever of the three links has focus.
    const focused = cards.indexOf(document.activeElement);
    if (focused !== -1) choose(focused);
    sync();
  });
  return count => {
    const nextActive = count > 0;
    // Keep keyboard focus on a usable link when a control leaves the layout.
    if (nextActive && actions.querySelector('.qr-controls').contains(document.activeElement)) cards[current].focus();
    if (!nextActive && active) {
      const focused = cards.indexOf(document.activeElement);
      if (focused !== -1) choose(focused);
    }
    stage.dataset.view = nextActive ? 'active' : 'idle';
    stage.dataset.activityCount = String(count);
    if (nextActive !== active || !stage.dataset.rotation) {active = nextActive; sync();}
  };
}

function startSlideshow(count) {
  let current = 0;
  const slides = [...document.querySelectorAll('.slide')], dots = [...document.querySelectorAll('.progress i')];
  const clock = document.querySelector('#clock');
  const tick = () => {const date = new Date(); clock.textContent = date.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'}); clock.dateTime = date.toISOString();};
  tick();
  setInterval(tick, 10000);
  if (count > 1) setInterval(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    slides[current].classList.remove('active'); dots[current].classList.remove('active');
    current = (current + 1) % count;
    slides[current].classList.add('active'); dots[current].classList.add('active');
  }, 9000);
}

function startActivity(setView) {
  const list = document.querySelector('#activity-items'), connection = document.querySelector('.feed-connection');
  const announcement = document.querySelector('.activity-announcement');
  let events = [], lastHTML = '', connected = false, tipSource = false;
  let expiryTimer;
  let serverTime = Date.now(), receivedAt = performance.now();
  const draw = () => {
    const current = serverTime + performance.now() - receivedAt;
    events = events.filter(item => Date.parse(item.expires_at) > current);
    setView(Math.min(events.length, 4));
    clearTimeout(expiryTimer);
    if (events.length) expiryTimer = setTimeout(draw, Math.max(1, Math.ceil(Math.min(...events.map(item => Date.parse(item.expires_at))) - current)));
    const html = events.slice(0, 4).map(item => {
      const order = item.type === 'order', tip = item.type === 'tip';
      const title = order ? `Table ${item.table}` : tip ? 'Thank you for the tip!' : 'JustPay received';
      const details = order ? (item.items?.length ? item.items.map(product => `${product.qty} × ${product.name}`).join(' · ') : 'New order · details with the team') : tip ? `${Number(item.amount_sats).toLocaleString()} sats` : new Intl.NumberFormat('en', {style:'currency',currency:'EUR'}).format(item.amount_cents / 100);
      const remaining = Math.max(0, Date.parse(item.expires_at) - current);
      return `<article class="activity-card ${esc(item.type)}"><span class="activity-icon" aria-hidden="true">${order ? '↗' : tip ? '♥' : '₿'}</span><div><div class="activity-label">${order ? 'Order to table' : tip ? 'Nostr tip' : 'Payment'}<span>${order ? item.status === 'accepted' ? 'Preparing' : 'New' : 'Received'}</span></div><h3>${esc(title)}</h3><p>${esc(details)}</p></div><div class="event-lifetime" aria-hidden="true" style="--remaining:${remaining}ms"></div></article>`;
    }).join('');
    // Avoid repeat live-region announcements on every timer tick.
    const signature = events.slice(0, 4).map(event => JSON.stringify(event)).join('|') + connected;
    if (signature !== lastHTML) {
      list.innerHTML = html; lastHTML = signature;
      // This live region stays in the accessibility tree even while the empty
      // visual feed is hidden, so the first incoming activity is announced too.
      announcement.textContent = events.length ? [...list.children].map(card => card.innerText).join('. ') : 'No recent activity. Scan to order, pay or tip.';
    }
    connection.textContent = connected ? tipSource ? 'Live' : 'Live · tips reconnecting' : 'Reconnecting';
    connection.classList.toggle('offline', !connected);
  };
  const poll = async () => {
    const requestedAt = performance.now();
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 6500);
    try {
      const response = await fetch(`${apiBase}/api/screen-activity?npub=${encodeURIComponent(npub)}`, {cache: 'no-store', signal: controller.signal});
      if (!response.ok) throw new Error('Activity unavailable');
      const data = await response.json();
      const reference = Date.parse(data.server_time);
      if (!Number.isFinite(reference) || !Array.isArray(data.events)) throw new Error('Invalid activity response');
      receivedAt = performance.now();
      // Conservatively include request travel time; latency must not extend the TTL.
      serverTime = reference + receivedAt - requestedAt;
      events = data.events.filter(item => ['order', 'justpay', 'tip'].includes(item.type) &&
        Date.parse(item.occurred_at) <= reference && Date.parse(item.expires_at) > reference &&
        Date.parse(item.expires_at) - Date.parse(item.occurred_at) <= 120000 &&
        (item.type !== 'order' || ['new', 'accepted'].includes(item.status)));
      connected = true; tipSource = data.tips_available === true;
    } catch {
      // A completed order may have disappeared while offline. Do not keep stale tickets.
      events = []; connected = false;
    } finally {
      clearTimeout(timeout); draw(); setTimeout(poll, 4000);
    }
  };
  poll();
}

if (!npub) error('Add the merchant npub to start.');
else fetch(`${apiBase}/api/screen?npub=${encodeURIComponent(npub)}`, {cache: 'no-store'})
  .then(async response => {const data = await response.json(); if (!response.ok) throw new Error(data.error || 'Merchant unavailable'); return data;})
  .then(render).catch(failure => error(failure.message));
