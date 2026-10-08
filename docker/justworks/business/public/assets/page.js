// Just Works page template — renders the platform-hosted merchant page
// locally from /api/business. This is the design surface for the stable
// justworks.cash/{slug} page: module CTAs, the QR that never moves, and the
// pointer to the sovereign nsite copy rendered at /site.
import {mountLogo} from './ds/logo.js';
import {link} from './ds/frame.js';

const app=document.querySelector('#app');
const base=location.pathname.split('/').slice(0,-1).join('/');
const npub=new URLSearchParams(location.search).get('npub')?.trim()||'';
const esc=value=>String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const qr=text=>`${base}/api/qr?text=${encodeURIComponent(text)}`;

const moduleMeta={
  order:{name:'Order to table',copy:'Scan, order, and pay from the table. No app for customers, no terminal for staff.',glyph:'＋',target:'order'},
  pay:{name:'Pay Bill',copy:'Close the table from the customer side. A confirmed Lightning payment settles instantly.',glyph:'€',target:'pay'},
  payandgo:{name:'Just Pay',copy:'Pay a paper receipt in sats. Type the total, get an invoice, done.',glyph:'₿',target:'payandgo'},
  booking:{name:'Bookings',copy:'Book a table now or later, straight from the page.',glyph:'⌁',target:'book'},
};

function fail(message){
  app.innerHTML=`<section class="page-fail"><span>⌁</span><h1>${esc(message)}</h1><small>Use /page?npub=npub1…</small></section>`;
}

function fact(term,value,href){
  return `<div><dt>${esc(term)}</dt><dd>${href?`<a href="${esc(href)}" target="_blank" rel="noopener">${esc(value)} ↗</a>`:esc(value)}</dd></div>`;
}

function moduleCard(id,state){
  const meta=moduleMeta[id];
  if(!meta||!state.enabled||state.status==='coming-soon')return '';
  const url=link(meta.target);
  return `<article class="module-card">
    <div>
      <h3><span class="glyph">${meta.glyph}</span>${esc(meta.name)} <span class="status ${state.status==='live'?'live':''}">${state.status==='live'?'Live service':'Preview'}</span></h3>
      <p>${esc(meta.copy)}</p>
    </div>
    ${id==='order'?`<a class="qr" href="${esc(url)}" aria-label="Order to table QR"><img src="${qr(url)}" alt=""></a>`:''}
    <a class="cta" href="${esc(url)}">Open ${esc(meta.name)} <span>→</span></a>
  </article>`;
}

function render(data){
  const merchant=data.merchant,about=(data.profile?.about||'').trim(),hero=data.images[0]||'';
  const facts=[merchant.hours,merchant.address,merchant.phone].filter(Boolean);
  document.title=`${merchant.name} — Just Works`;
  const cards=Object.entries(moduleMeta).map(([id,meta])=>moduleCard(id,data.modules[id]||{})).filter(Boolean).join('');
  app.innerHTML=`
<nav class="page-nav">
  <a class="brand" href="${esc(link('business'))}"><span data-logo="icon"></span><span>Just Works<small>Merchant page</small></span></a>
  <span class="chip">${esc(merchant.name)}</span>
</nav>
<section class="page-hero" ${hero?`data-hero style="--hero:url('${esc(hero)}')"`:''}>
  ${hero?'<div class="backdrop"></div>':''}
  <div class="shade"></div>
  <div class="copy">
    <span class="tag">${esc([merchant.category,merchant.area].filter(Boolean).join(' · ')||'Merchant')}</span>
    <h1>${esc(merchant.name)}</h1>
    ${about?`<p class="lead">${esc(about)}</p>`:''}
    ${facts.length?`<div class="facts">${facts.map(item=>`<span>${esc(item)}</span>`).join('')}</div>`:''}
  </div>
</section>
<main>
  ${cards?`<section class="page-section" id="modules"><div><p class="eyebrow">Just Works modules</p><h2>Run the place from the page.</h2><div class="module-grid">${cards}</div></div></section>`:''}
  ${data.images.length>1?`<section class="page-section" id="gallery"><div><p class="eyebrow">Gallery</p><h2>The place in photos.</h2><div class="photo-grid">${data.images.slice(1).map(src=>`<figure><img src="${esc(src)}" alt="${esc(merchant.name)}" loading="lazy"></figure>`).join('')}</div></div></section>`:''}
  ${(merchant.address||merchant.hours||merchant.phone||merchant.website)?`<section class="page-section" id="visit"><div><p class="eyebrow">Visit</p><h2>Find us.</h2><dl class="facts-list">${merchant.address?fact('Address',merchant.address):''}${merchant.hours?fact('Hours',merchant.hours):''}${merchant.phone?fact('Phone',merchant.phone,`tel:${esc(merchant.phone)}`):''}${merchant.website?fact('Website',merchant.website.replace(/^https?:\/\//,''),merchant.website):''}${merchant.rating?fact('Rating',`★ ${merchant.rating} / 5`):''}</dl></div></section>`:''}
  <section class="page-section dark" id="sovereign"><div class="sovereign">
    <div>
      <p class="eyebrow">Ownership</p>
      <h2>This page has a sovereign copy.</h2>
      <p>The same page is signed by the merchant and published to Nostr relays and Blossom storage. It resolves even if Just Works disappears.</p>
    </div>
    <a class="go" href="${esc(data.links.nsite)}">View the nsite copy <span>↗</span></a>
  </div></section>
</main>
<footer class="page-footer">
  <span class="tagline"><span data-logo="icon"></span> Built with Just Works</span>
  <code>${esc(npub)}</code>
</footer>`;
  document.querySelectorAll('[data-logo]').forEach(host=>mountLogo(host,{icon:true}));
}

if(!npub)fail('Add the merchant npub.');
else fetch(`${base}/api/business?npub=${encodeURIComponent(npub)}`,{cache:'no-store'})
  .then(async response=>{const data=await response.json();if(!response.ok)throw new Error(data.error||'Page unavailable');render(data)})
  .catch(error=>fail(error.message));
