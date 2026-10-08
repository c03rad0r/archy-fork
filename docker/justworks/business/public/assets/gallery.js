import { mountLogo } from "./ds/logo.js";
import { trackGallery } from "./general-analytics.js";
document
  .querySelectorAll("[data-logo]")
  .forEach((el) => mountLogo(el, { icon: true }));
document.querySelector("#themeToggle")?.remove();
const $ = (id) => document.querySelector(id);
const gallery = $("#gallery"),
  search = $("#search"),
  empty = $("#empty");
const categoryNames = {
  restaurant: "Restaurant",
  cafe: "Café",
  bar: "Bar",
  bakery: "Bakery",
  stay: "Stay",
  hotel: "Stay",
  shop: "Shop",
  market: "Market",
  beauty: "Beauty",
  barber: "Barber",
  activity: "Experience",
  other: "Local place",
};
const mealTerms = {
  breakfast: ["breakfast", "brunch", "bakery", "cafe", "coffee"],
  lunch: ["lunch", "restaurant", "cafe", "takeaway"],
  dinner: ["dinner", "restaurant", "bar", "evening"],
  coffee: ["coffee", "cafe", "bakery"],
  drinks: ["drinks", "bar", "wine", "cocktail", "beer", "poncha"],
  stay: ["stay", "hotel", "hostel", "guesthouse", "accommodation"],
  shop: ["shop", "market", "store", "retail"],
};
let places = [],
  filtered = [],
  payment = "all",
  quick = "",
  shown = 0,
  seed = 0;
const PAGE = 36;
const esc = (v) =>
  String(v ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const words = (p) =>
  [p.name, p.category, p.area, p.address, ...(p.tags || [])]
    .join(" ")
    .toLowerCase();
const photo = (p) => (p.images || []).find(Boolean) || p.image || "";
function pseudo(value) {
  let n = 0;
  for (const c of value) n = (n * 31 + c.charCodeAt(0)) >>> 0;
  return n;
}
function isOpen(p) {
  if (!p.hours) return false;
  const text = p.hours.toLowerCase();
  if (/open 24|24\/7/.test(text)) return true;
  if (/closed/.test(text) && !/[–-]/.test(text)) return false;
  return true;
}
function matchQuick(p) {
  if (!quick) return true;
  if (quick === "open") return isOpen(p);
  const hay = words(p);
  return (mealTerms[quick] || [quick]).some((term) => hay.includes(term));
}
function card(p, index) {
  const image = photo(p),
    height = ["portrait", "landscape", "tall"][(pseudo(p.slug) + seed) % 3],
    bitcoin = p.payment === "bitcoin",
    url = p.justworks || p.nsite || p.website || "#";
  return `<article class="place-card ${height} ${image ? "has-image" : "no-image"}" data-category="${esc(p.category || "other")}" style="--delay:${Math.min(index, 15) * 25}ms"><a href="${esc(url)}" ${url.startsWith("http") ? 'target="_blank" rel="noopener"' : ""}>${image ? `<figure><img src="${esc(image)}" alt="${esc(p.name)}" loading="lazy" decoding="async"><div class="image-wash"></div><span class="pay-mark ${bitcoin ? "bitcoin" : "fiat"}">${bitcoin ? "₿ Bitcoin" : "Fiat"}</span></figure>` : `<figure class="fallback"><span>${esc((p.name || "?").slice(0, 2).toUpperCase())}</span><small>${esc(categoryNames[p.category] || p.category || "Local")}</small></figure>`}<div class="card-copy"><div><p>${esc(categoryNames[p.category] || p.category || "Local place")} · ${esc(p.area || "Madeira")}</p><h2>${esc(p.name)}</h2></div><span class="arrow">↗</span>${p.rating ? `<small class="rating">★ ${Number(p.rating).toFixed(1)}</small>` : ""}<div class="card-tags">${(
    p.tags || []
  )
    .slice(0, 3)
    .map((tag) => `<span>${esc(tag)}</span>`)
    .join("")}</div></div></a></article>`;
}
function apply(reset = true) {
  const term = search.value.trim().toLowerCase();
  filtered = places.filter(
    (p) =>
      (payment === "all" || p.payment === payment) &&
      matchQuick(p) &&
      (!term || words(p).includes(term)),
  );
  filtered.sort(
    (a, b) =>
      (photo(b) ? 1 : 0) - (photo(a) ? 1 : 0) ||
      (b.rating || 0) - (a.rating || 0),
  );
  if (seed)
    filtered.sort(
      (a, b) => (pseudo(a.slug + seed) % 997) - (pseudo(b.slug + seed) % 997),
    );
  if (reset) {
    shown = 0;
    gallery.innerHTML = "";
  }
  renderMore();
  $("#resultCount").textContent =
    `${filtered.length} ${filtered.length === 1 ? "place" : "places"}`;
  $("#resultContext").textContent =
    [
      payment !== "all"
        ? payment === "bitcoin"
          ? "Bitcoin accepted"
          : "Fiat payment"
        : "",
      quick && quick !== "open"
        ? quick[0].toUpperCase() + quick.slice(1)
        : quick === "open"
          ? "Published hours available"
          : "",
      term ? `matching “${term}”` : "",
    ]
      .filter(Boolean)
      .join(" · ") || "Real merchants from Just Works Maps";
  empty.hidden = filtered.length > 0;
  gallery.hidden = !filtered.length;
  history.replaceState(
    null,
    "",
    `?${new URLSearchParams({ ...(term ? { q: term } : {}), ...(payment !== "all" ? { pay: payment } : {}), ...(quick ? { filter: quick } : {}) })}`,
  );
}
function renderMore() {
  const batch = filtered.slice(shown, shown + PAGE);
  gallery.insertAdjacentHTML(
    "beforeend",
    batch.map((p, i) => card(p, shown + i)).join(""),
  );
  shown += batch.length;
  document
    .querySelectorAll("img")
    .forEach(
      (img) =>
        (img.onerror = () =>
          img.closest(".place-card")?.classList.add("broken-image")),
    );
}
function selectPayment(value) {
  payment = value;
  document
    .querySelectorAll("[data-payment]")
    .forEach((b) => b.classList.toggle("active", b.dataset.payment === value));
  apply();
  trackGallery("surface_interaction", `payment_filter:${value}`);
}
function selectQuick(value) {
  quick = value;
  document
    .querySelectorAll("[data-query]")
    .forEach((b) => b.classList.toggle("active", b.dataset.query === value));
  apply();
  if (value) trackGallery("surface_interaction", `filter:${value}`);
}
function clear() {
  search.value = "";
  payment = "all";
  quick = "";
  seed = 0;
  document
    .querySelectorAll("[data-payment]")
    .forEach((b) => b.classList.toggle("active", b.dataset.payment === "all"));
  document
    .querySelectorAll("[data-query]")
    .forEach((b) => b.classList.toggle("active", !b.dataset.query));
  apply();
}
document
  .querySelectorAll("[data-payment]")
  .forEach((b) => (b.onclick = () => selectPayment(b.dataset.payment)));
document
  .querySelectorAll("[data-query]")
  .forEach((b) => (b.onclick = () => selectQuick(b.dataset.query)));
let searchTimer;
search.oninput = () => {
  apply();
  clearTimeout(searchTimer);
  if (search.value.trim())
    searchTimer = setTimeout(
      () => trackGallery("surface_interaction", "search"),
      700,
    );
};
gallery.addEventListener("click", (event) => {
  const card = event.target.closest(".place-card");
  if (card)
    trackGallery(
      "surface_interaction",
      `place_open:${card.dataset.category || "other"}`,
    );
});
$("#clearSearch").onclick = () => {
  search.value = "";
  apply();
  search.focus();
};
$("#clearAll").onclick = clear;
$("#shuffle").onclick = () => {
  seed = Date.now();
  apply();
  trackGallery("surface_interaction", "shuffle");
};
new IntersectionObserver(
  (entries) => {
    if (entries[0].isIntersecting && shown < filtered.length) renderMore();
  },
  { rootMargin: "800px" },
).observe($("#sentinel"));
async function boot() {
  const data = await fetch("/api/gallery", { cache: "no-store" }).then((r) => {
    if (!r.ok) throw new Error("data");
    return r.json();
  });
  places = data.places || [];
  $("#totalCount").textContent = `${places.length} independent places`;
  $("#allCount").textContent = places.length;
  $("#bitcoinCount").textContent = places.filter(
    (p) => p.payment === "bitcoin",
  ).length;
  $("#fiatCount").textContent = places.filter(
    (p) => p.payment === "fiat",
  ).length;
  const params = new URLSearchParams(location.search);
  search.value = params.get("q") || "";
  payment = params.get("pay") || "all";
  quick = params.get("filter") || "";
  document
    .querySelectorAll("[data-payment]")
    .forEach((b) =>
      b.classList.toggle("active", b.dataset.payment === payment),
    );
  document
    .querySelectorAll("[data-query]")
    .forEach((b) => b.classList.toggle("active", b.dataset.query === quick));
  apply();
  trackGallery("surface_view", "open");
}
boot().catch(() => {
  $("#resultCount").textContent = "Gallery temporarily unavailable";
  empty.hidden = false;
  gallery.hidden = true;
});
