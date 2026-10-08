// nsite template — renders the sovereign published page locally from the
// merchant's real nsite data. This is the design surface for the generated
// customer pages: changes here are what the product's generator adopts.
import {mountLogo} from './ds/logo.js';

const app=document.querySelector('#app');
const base=location.pathname.split('/').slice(0,-1).join('/');
const npub=new URLSearchParams(location.search).get('npub')?.trim()||'';
const esc=value=>String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const qr=text=>`${base}/api/qr?text=${encodeURIComponent(text)}`;

function fail(message){
  app.innerHTML=`<section class="site-fail"><span>⌁</span><h1>${esc(message)}</h1><small>Use /site?npub=npub1…</small></section>`;
}

function fact(term,value,href){
  return `<div><dt>${esc(term)}</dt><dd>${href?`<a href="${esc(href)}" target="_blank" rel="noopener">${esc(value)} ↗</a>`:esc(value)}</dd></div>`;
}

function render(data){
  const merchant=data.merchant,about=data.about,hero=data.images[0]||'';
  document.title=`${merchant.name} — Just Works`;
  app.innerHTML=`
<nav class="site-nav">
  <a class="brand" href="${esc(data.links.page)}"><span data-logo="icon"></span><span>Just Works<small>The sovereign copy</small></span></a>
  <div class="anchors">
    ${data.sources.length?'<a href="#whats-on">What’s on</a>':''}
    <a href="#story">The place</a>
    ${(merchant.address||merchant.hours||merchant.phone)?'<a href="#visit">Visit</a>':''}
  </div>
</nav>
<section class="site-hero" ${hero?`data-hero style="--hero:url('${esc(hero)}')"`:''}>
  ${hero?'<div class="backdrop"></div>':''}
  <div class="shade"></div>
  <div class="copy">
    <span class="tag">${esc([merchant.category,merchant.area].filter(Boolean).join(' · ')||'Merchant')}</span>
    <h1>${esc(merchant.name)}</h1>
    ${about?`<p class="lead">${esc(about)}</p>`:''}
  </div>
</section>
<main>
  ${data.sources.length?`<section class="site-section" id="whats-on"><div><p class="eyebrow">${data.products.length?'This week':'What’s on'}</p><h2>What’s on.</h2><div class="feature-grid">${data.sources.map((source,index)=>`<article class="feature-card"><span>${String(index+1).padStart(2,'0')}</span><div><strong>${esc(source.host||'Source')}${source.photos?` · ${source.photos} photos`:''}</strong><p>${esc(source.description||source.name)}${source.url?`<br><a href="${esc(source.url)}" target="_blank" rel="noopener">${esc(source.url.replace(/^https?:\//,''))} ↗</a>`:''}</p></div></article>`).join('')}</div></div></section>`:''}
  <section class="site-section" id="story"><div><p class="eyebrow">The place</p><h2>A local place called ${esc(merchant.name)}.</h2><div class="story-grid"><p class="statement">${esc(about||`${merchant.name} keeps a page on the open web — photos, links and the latest info, signed by the owner.`)}</p>${data.images[1]?`<div class="story-media" style="--story:url('${esc(data.images[1])}')"></div>`:''}</div></div></section>
  ${data.products.length?`<section class="site-section" id="menu"><div><p class="eyebrow">Menu</p><h2>What we serve.</h2><div class="menu-grid">${data.products.map(product=>`<article class="menu-item"><div><h3>${esc(product.name)}</h3>${product.description?`<p>${esc(product.description)}</p>`:''}</div>${product.price?`<strong>${esc(product.price)}</strong>`:''}</article>`).join('')}</div></div></section>`:''}
  ${data.images.length>1?`<section class="site-section" id="gallery"><div><p class="eyebrow">Gallery</p><h2>The place in photos.</h2><div class="photo-grid">${data.images.slice(1).map(src=>`<figure><img src="${esc(src)}" alt="${esc(merchant.name)}" loading="lazy"></figure>`).join('')}</div></div></section>`:''}
  ${(merchant.address||merchant.hours||merchant.phone||merchant.website)?`<section class="site-section" id="visit"><div><p class="eyebrow">Visit</p><h2>Find us.</h2><dl class="facts-list">${merchant.address?fact('Address',merchant.address):''}${merchant.hours?fact('Hours',merchant.hours):''}${merchant.phone?fact('Phone',merchant.phone,`tel:${esc(merchant.phone)}`):''}${merchant.website?fact('Website',merchant.website.replace(/^https?:\/\//,''),merchant.website):''}${merchant.rating?fact('Rating',`★ ${merchant.rating} / 5`):''}</dl></div></section>`:''}
  <section class="site-section dark" id="connect"><div class="connect-grid">
    <a class="qr" href="${esc(data.links.page)}" aria-label="QR code for the merchant page"><img src="${qr(data.links.page)}" alt=""></a>
    <div class="connect-copy">
      <p class="eyebrow">Stay connected</p>
      <h2>Keep the direct connection.</h2>
      <p>This page is signed by the merchant and published to Nostr relays and Blossom storage. If every platform disappears, this page still resolves.</p>
      <div class="pair"><a href="${esc(data.links.page)}">Just Works page ↗</a><a href="${esc(data.links.business)}">Business tools ↗</a></div>
    </div>
  </div></section>
  <section class="site-section dark" id="ownership"><div><p class="eyebrow">Technical ownership details</p><h2>Who owns this page.</h2><dl class="own-grid">${fact('Signed by',merchant.npub)}${data.gateway?fact('Nsite gateway',data.gateway.replace(/^https?:\/\//,''),data.gateway):''}${fact('Published via','Nostr relays · Blossom storage')}${fact('Stable page',data.links.page)}</dl></div></section>
</main>`;
  document.querySelectorAll('[data-logo]').forEach(host=>mountLogo(host,{icon:true}));
}

if(!npub)fail('Add the merchant npub.');
else fetch(`${base}/api/nsite-page?npub=${encodeURIComponent(npub)}`,{cache:'no-store'})
  .then(async response=>{const data=await response.json();if(!response.ok)throw new Error(data.error||'Site unavailable');render(data)})
  .catch(error=>fail(error.message));
