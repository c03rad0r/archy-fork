/*
 * Top bar scroll state — shared with the main site's behaviour.
 * Transparent while parked; past the first 80px the bar earns the
 * translucent ground, hairline and blur. Toggles `is-scrolled`.
 */
const SCROLL_START = 80;

export function startNavScroll(bar = document.querySelector("[data-topbar]")) {
  if (!bar) return () => {};
  let frame = 0;
  const apply = () => {
    frame = 0;
    bar.classList.toggle("is-scrolled", window.scrollY > SCROLL_START);
  };
  const onScroll = () => {
    if (!frame) frame = requestAnimationFrame(apply);
  };
  apply();
  window.addEventListener("scroll", onScroll, { passive: true });
  return () => {
    window.removeEventListener("scroll", onScroll);
    if (frame) cancelAnimationFrame(frame);
  };
}
