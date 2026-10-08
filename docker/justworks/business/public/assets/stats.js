import { mountLogo } from "./ds/logo.js";
const app = document.querySelector("#app"),
  npub = new URLSearchParams(location.search).get("npub")?.trim() || "";
const esc = (value) =>
  String(value ?? "").replace(
    /[&<>"']/g,
    (char) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        char
      ],
  );
const compact = (value) =>
  new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(Number(value || 0));
const sats = (value) => `${Number(value || 0).toLocaleString()} sats`,
  euro = (value) =>
    new Intl.NumberFormat(undefined, {
      style: "currency",
      currency: "EUR",
    }).format(Number(value || 0) / 100),
  pct = (value) => (value == null ? "—" : `${(value * 100).toFixed(1)}%`);
function chart(rows) {
  const w = 900,
    h = 250,
    p = 18,
    max = Math.max(
      1,
      ...rows.flatMap((row) => [row.pageViews, row.interactions]),
    );
  const points = (key) =>
    rows
      .map(
        (row, i) =>
          `${p + (i / Math.max(1, rows.length - 1)) * (w - p * 2)},${h - p - (Number(row[key] || 0) / max) * (h - p * 2)}`,
      )
      .join(" ");
  return `<svg viewBox="0 0 ${w} ${h}" role="img" aria-label="Views and interactions over 90 days"><path class="grid" d="M18 62H882M18 125H882M18 188H882"/><polyline class="views" points="${points("pageViews")}"/><polyline class="interactions" points="${points("interactions")}"/></svg><div class="axis"><span>${esc(rows[0]?.date || "")}</span><span>${esc(rows.at(-1)?.date || "")}</span></div>`;
}
function render(data) {
  const m = data.merchant,
    a = data.analytics || {};
  document.title = `${m.name} · Stats`;
  app.innerHTML = `<header><a href="/?npub=${encodeURIComponent(npub)}" class="brand"><span data-logo></span><b>Just Works<small>Stats</small></b></a><span>Public preview · Nostr login follows</span></header><section class="hero"><div><p class="eyebrow">Merchant intelligence · ${data.periodDays} days</p><h1>${esc(m.name)}</h1><p>${esc(m.npub)}</p></div><a href="/?npub=${encodeURIComponent(npub)}">Back to Business ↙</a></section><section class="kpis">${[
    ["Routed views", compact(a.pageViews)],
    ["Interactions", compact(a.interactions)],
    ["Engagement", pct(a.engagementRate)],
    ["Revenue", euro(a.revenueCents)],
    ["Settled sats", sats(a.paymentSats)],
    ["Paid", a.paidPayments || 0],
  ]
    .map(
      ([label, value]) =>
        `<article><span>${label}</span><strong>${value}</strong></article>`,
    )
    .join(
      "",
    )}</section><section class="report"><article class="panel trend"><header><div><p class="eyebrow">Audience & intent</p><h2>What moves people.</h2></div><div class="legend"><span>Views</span><span>Interactions</span></div></header>${chart(a.daily || [])}</article><article class="panel outcomes"><p class="eyebrow">Business outcomes</p><h2>From attention to action.</h2>${[
    ["Orders", a.orders || 0],
    ["Bookings", a.bookings || 0],
    ["Payment attempts", a.paymentAttempts || 0],
    ["Payment conversion", pct(a.paymentConversionRate)],
    ["Public tips", sats(m.publicTipSats)],
  ]
    .map(
      ([label, value]) =>
        `<div><span>${label}</span><strong>${value}</strong></div>`,
    )
    .join(
      "",
    )}</article></section><section class="report lower"><article class="panel"><p class="eyebrow">Top actions</p><h2>What customers do.</h2><div class="actions">${a.actions?.length ? a.actions.map((item) => `<div><span>${esc(item.action.replaceAll("_", " "))}</span><strong>${item.count}</strong></div>`).join("") : '<p class="note">No tracked interactions yet. Collection starts with the analytics release.</p>'}</div></article><article class="panel"><p class="eyebrow">Modules</p><h2>Product footprint.</h2><div class="actions">${m.modules?.map((item) => `<div><span>${esc(item.id)}</span><strong>${item.enabled ? "On" : "Off"} · ${esc(item.status)}</strong></div>`).join("") || '<p class="note">No Business modules configured.</p>'}</div></article></section><footer>${esc(data.notes?.traffic || "")}</footer>`;
  document
    .querySelectorAll("[data-logo]")
    .forEach((node) => mountLogo(node, { icon: true }));
}
function fail(message) {
  app.innerHTML = `<section class="failure"><span>∩</span><p class="eyebrow">Stats unavailable</p><h1>${esc(message)}</h1><a href="/">Back to Business</a></section>`;
}
if (!npub) fail("Add a merchant npub to open its stats.");
else
  fetch(
    `https://justworks.cash/api/merchant-stats?npub=${encodeURIComponent(npub)}`,
    { cache: "no-store" },
  )
    .then(async (response) => {
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "Stats unavailable");
      render(data);
    })
    .catch((error) => fail(error.message));
