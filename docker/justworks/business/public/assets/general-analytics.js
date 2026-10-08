const endpoint = "https://justworks.cash/api/analytics/event";

export function trackGallery(type = "surface_interaction", action = "open") {
  try {
    const body = JSON.stringify({ surface: "gallery", type, action });
    if (
      navigator.sendBeacon?.(endpoint, new Blob([body], { type: "text/plain" }))
    )
      return;
    fetch(endpoint, { method: "POST", body, keepalive: true }).catch(() => {});
  } catch {}
}
