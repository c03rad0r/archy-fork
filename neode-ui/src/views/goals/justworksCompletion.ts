/** Accept setup progress only from the active, gate-protected Just Works frame.
 * A connected public page is not proof of merchant ownership or a login. */
export function isJustworksCompletion(
  event: MessageEvent,
  appId: string,
  appUrl: string,
  frameWindow: Window | null,
  allowDemo = false,
): boolean {
  if (appId !== 'justworks' || !frameWindow || event.source !== frameWindow) return false
  try {
    if (event.origin !== new URL(appUrl, window.location.origin).origin) return false
  } catch { return false }
  const site = event.data?.site
  const verified = event.data?.type === 'justworks:site-connected' && site?.published === true
  const sample = allowDemo && event.data?.type === 'justworks:demo-site-connected'
    && site?.published === false && site?.id === 'corner-bakery-demo'
  return (verified || sample)
    && typeof site.id === 'string'
    && site.id.length <= 160
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(site.id)
}
