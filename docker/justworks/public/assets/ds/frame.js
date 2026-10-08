import {mountLogo} from './logo.js';
import {startNavScroll} from './header.js';

const prod=location.hostname.endsWith('justworks.cash');
export const base=location.pathname.startsWith('/business-demo')?'/business-demo':'';
const search=location.search;
export const link=id=>prod?`${id==='business'?'https://business.justworks.cash/':`https://${id}.justworks.cash/`}${search}`:({business:`${base}/dashboard${search}`,pos:`${base}/pos${search}`,order:`${base}/order${search}`,book:`${base}/book${search}`,booking:`${base}/book${search}`,screen:`${base}/screen${search}`,gallery:`${base}/gallery${search}`})[id]||`${base}/portal?surface=${id}`;
// Archipelago navigation adapter; visual components remain upstream.
let links=[['order','Order'],['pay','Pay Bill'],['book','Book'],['gallery','Explore']];
let brandHref;
export function configureNavigation(options){links=options.items;brandHref=options.brandHref}
const destination=id=>brandHref ? id : link(id);
const menuSvg='<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 5h11M2.5 11h11"/></svg>';
const closeSvg='<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>';
const upSvg='<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 9.5 8 5l4.5 4.5"/></svg>';
const topbar=(label,endHtml='')=>`<nav class="jw-topbar" data-topbar><a class="jw-brand brand" href="${brandHref || link('business')}"><span data-logo="icon"></span><b>Just Works<small>${label}</small></b></a><div class="jw-topbar-links links">${links.map(([id,name])=>`<a href="${destination(id)}">${name}</a>`).join('')}</div>${endHtml?`<div class="jw-topbar-end top">${endHtml}</div>`:''}</nav>`;

function mountChromeControls(app){
  const existing=app.querySelector('.mobile-menu');
  if(existing)return existing;
  app.insertAdjacentHTML('beforeend',`<button class="button icon ring mobile-menu-button" type="button" aria-label="Menu" aria-expanded="false" aria-controls="mobileMenu">${menuSvg}</button><button class="button icon ring back-to-top" type="button" aria-label="Back to top">${upSvg}</button><div class="mobile-menu" id="mobileMenu" aria-hidden="true"><button class="button icon ring mobile-menu-close" type="button" aria-label="Close menu">${closeSvg}</button><nav class="mobile-menu-items" aria-label="Menu">${links.map(([id,name])=>`<a href="${destination(id)}">${name}</a>`).join('')}</nav></div>`);
  const trigger=app.querySelector('.mobile-menu-button');
  const menu=app.querySelector('.mobile-menu');
  const close=app.querySelector('.mobile-menu-close');
  const back=app.querySelector('.back-to-top');
  let previousFocus=null;
  const focusable=()=>[...menu.querySelectorAll('a,button,[tabindex]:not([tabindex="-1"])')];
  const setMenu=open=>{menu.classList.toggle('open',open);menu.setAttribute('aria-hidden',String(!open));trigger.setAttribute('aria-expanded',String(open));document.body.classList.toggle('creating',open);if(open){previousFocus=document.activeElement;close.focus()}else if(previousFocus instanceof HTMLElement){previousFocus.focus()}};
  trigger.addEventListener('click',()=>setMenu(true));
  close.addEventListener('click',()=>setMenu(false));
  menu.addEventListener('click',event=>{if(event.target.closest('a'))setMenu(false)});
  menu.addEventListener('keydown',event=>{if(event.key==='Escape'){setMenu(false);return}if(event.key!=='Tab')return;const items=focusable();const first=items[0],last=items.at(-1);if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus()}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus()}});
  back.addEventListener('click',()=>scrollTo({top:0,behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'auto':'smooth'}));
  let request=0;
  const update=()=>{request=0;back.classList.toggle('show',scrollY>innerHeight)};
  addEventListener('scroll',()=>{if(!request)request=requestAnimationFrame(update)},{passive:true});
  addEventListener('resize',()=>{if(innerWidth>980&&menu.classList.contains('open'))setMenu(false)},{passive:true});
  update();
  return menu;
}

function initialize(app,grounded=false){
  app.querySelectorAll('[data-logo]').forEach(host=>mountLogo(host,{icon:true}));
  mountChromeControls(app);
  const bar=app.querySelector('[data-topbar]');
  if(grounded)bar?.classList.add('is-scrolled');else startNavScroll(bar);
}

export function mountMobileChrome(app){return mountChromeControls(app)}
export function mountAppChrome(app,{label='',endHtml='',grounded=false}={}){app.classList.toggle('chrome-grounded',grounded);app.insertAdjacentHTML('afterbegin',topbar(label,endHtml));initialize(app,grounded)}
export function frame(app,{label='',surface='',html='',nav=true}){app.innerHTML=nav?`${topbar(label)}<main class="surface ${surface}">${html}</main>`:`<main class="surface ${surface}">${html}</main>`;if(nav)initialize(app);else app.querySelectorAll('[data-logo]').forEach(host=>mountLogo(host,{icon:true}))}
