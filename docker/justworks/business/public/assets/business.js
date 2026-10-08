import { mountLogo } from "./ds/logo.js";
import { startNavScroll } from "./ds/header.js";
import { mountMobileChrome, base } from "./ds/frame.js";
import { npubFrom, prod } from "./ds/env.js";
import { qrButton, mountModuleQrs } from "./module-qr.js?v=3";
import { freeDefinitions, businessDefinitions, moduleDestination, businessUrl } from './business-destinations.js?v=1';
const app = document.querySelector("#app"),
  toast = document.querySelector("#toast");
const npub = npubFrom();
const esc = (value) =>
  String(value ?? "").replace(
    /[&<>"']/g,
    (char) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        char
      ],
  );
function freeCard([id, name, copy, icon], data) {
  const url = data.links?.[id] || "#";
  const qr = url === '#' ? '' : qrButton(id, name, url, {free: true});
  return `<article class="free-link"><div class="free-link-top"><span class="free-icon">${icon}</span>${qr}</div><div><h2>${esc(name)}</h2><p>${esc(copy)}</p></div><a class="free-open" href="${esc(url)}" target="_blank" rel="noopener">Open <b>↗</b></a></article>`;
}
function moduleCard(
  [id, name, copy, audience, icon, configurable, target = id],
  data,
) {
  const {state, paymentSetup, disabled, url, qrUrl} = moduleDestination(
    [id, name, copy, audience, icon, configurable, target], data, npub);
  const qr = disabled ? '' : qrButton(id, name, qrUrl, {hidden: configurable && !state.enabled});
  return `<article class="module ${disabled ? "soon" : ""} ${paymentSetup ? "needs-setup" : ""}"><div class="module-top"><span class="module-icon">${icon}</span><span class="module-audience">${paymentSetup ? "Setup required" : esc(audience)}</span></div><div class="module-summary"><div><h2>${esc(name)}</h2><p>${paymentSetup ? "Add the merchant’s Lightning address before accepting payments." : esc(copy)}</p></div>${qr}</div><div class="module-actions"><a href="${esc(url)}" ${paymentSetup ? "" : 'target="_blank" rel="noopener"'}>${paymentSetup ? "Change Lightning address" : "Open"} <span>↗</span></a>${configurable && !paymentSetup ? `<label class="switch" aria-label="Activate ${esc(name)}"><input type="checkbox" data-module="${id}" ${state.enabled ? "checked" : ""} ${disabled ? "disabled" : ""}><span></span></label>` : paymentSetup ? '<span class="setup-lock">Locked</span>' : '<span class="always">Operations</span>'}</div></article>`;
}
function render(data) {
  const merchant = data.merchant,
    image = data.images[0] || "";
  document.title = `${merchant.name} · Just Works Business`;
  app.innerHTML = `<header class="top" data-topbar><a class="brand" href="${esc(data.links.justworks)}"><span data-logo="icon"></span><b>Just Works<small>Business</small></b></a><a class="stats-shortcut" href="${prod ? `https://business.justworks.cash/stats?npub=${encodeURIComponent(npub)}` : `${base}/stats?npub=${encodeURIComponent(npub)}`}" aria-label="Open Stats" title="Open Stats"><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3 16.5h14M5 14V9m5 5V4m5 10V7"/></svg></a><a class="profile" href="${esc(data.links.nsite)}" target="_blank" rel="noopener">View nsite ↗</a></header><section class="merchant" ${image ? `style="--hero:url('${esc(image)}')"` : ""}><div class="merchant-wash"></div><div class="merchant-copy"><p>${esc([merchant.category, merchant.area].filter(Boolean).join(" · "))}</p><h1>${esc(merchant.name)}</h1><span>Free public presence. Business tools when you need them.</span></div><div class="merchant-stats"><div><strong>${data.counts.orders}</strong><span>Open orders</span></div><div><strong>${data.counts.payments}</strong><span>Payments</span></div><a data-qr-sheet href="${esc(businessUrl(npub, true))}">QR in DIN A4 <b>↗</b></a></div></section><section class="modules-head free-head"><div><p class="eyebrow">Free links</p><h2>Included with Just Works.</h2></div><p>Your merchant identity, website and discovery pages stay free.</p></section><section class="free-links">${freeDefinitions.map((item) => freeCard(item, data)).join("")}</section><section class="modules-head business-head"><div><p class="eyebrow">Business modules</p><h2>Tools that run the place.</h2></div><p>Every module carries this merchant’s npub. Switch it on, share its link, or put it behind a QR code.</p></section><section class="modules">${businessDefinitions.map((item) => moduleCard(item, data)).join("")}</section><footer><span>npub</span><code>${esc(npub)}</code></footer>`;
  document
    .querySelectorAll("[data-module]")
    .forEach((input) => input.addEventListener("change", save));
  document
    .querySelectorAll("[data-logo]")
    .forEach((host) => mountLogo(host, { icon: true }));
  startNavScroll(document.querySelector("[data-topbar]"));
  mountModuleQrs(app);
}
async function save() {
  const input = this;
  input.disabled = true;
  const modules = Object.fromEntries(
    [...document.querySelectorAll("[data-module]")].map((item) => [
      item.dataset.module,
      item.checked,
    ]),
  );
  try {
    const response = await fetch("/api/business-config", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ npub, modules }),
    });
    if (!response.ok)
      throw new Error((await response.json()).error || "Could not save");
    const qr = input.closest('.module').querySelector('[data-module-qr]');
    if (qr) qr.hidden = !input.checked;
    toast.textContent = `${input.checked ? "Activated" : "Disabled"} ${input.closest(".module").querySelector("h2").textContent}`;
    toast.classList.add("show");
    setTimeout(() => toast.classList.remove("show"), 1500);
  } catch (error) {
    input.checked = !input.checked;
    toast.textContent = error.message;
    toast.classList.add("show", "error");
    setTimeout(() => toast.classList.remove("show", "error"), 2200);
  } finally {
    input.disabled = false;
  }
}
function fail(message) {
  app.innerHTML = `<section class="failure"><span>∩</span><p class="eyebrow">Business setup</p><h1>${esc(message)}</h1><p>Open this page with the merchant’s npub.</p><form><input name="npub" placeholder="npub1…" required><button>Open business <span>→</span></button></form></section>`;
}
if (!npub) fail("Which merchant is this?");
else
  fetch(`/api/business?npub=${encodeURIComponent(npub)}`, { cache: "no-store" })
    .then(async (response) => {
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "Merchant unavailable");
      render(data);
      mountMobileChrome(app);
    })
    .catch((error) => fail(error.message));
