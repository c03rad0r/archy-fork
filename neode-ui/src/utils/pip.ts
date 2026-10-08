/** Video Picture-in-Picture helpers (Chromium/Safari; no-ops elsewhere).
 *  Kiosk note: PiP is skipped on the WM-less kiosk X session by callers that
 *  care — an unmanaged popup there is unusable (docs/tv-input-iframe-apps.md
 *  sibling investigation, task #18). */

export const pipSupported =
  typeof document !== 'undefined' &&
  'pictureInPictureEnabled' in document &&
  document.pictureInPictureEnabled

/** Same three checks as `pipSupported`, but evaluated at call time rather
 *  than import time — lets a test stub support before or after this module
 *  is imported, and lets a component re-check support on demand. */
export function isPipSupported(): boolean {
  return (
    typeof document !== 'undefined' &&
    'pictureInPictureEnabled' in document &&
    document.pictureInPictureEnabled
  )
}

export async function togglePip(video: HTMLVideoElement | null | undefined): Promise<void> {
  // Check at call time. Embedded WebViews can expose the document capability
  // after the module has been evaluated, and companion navigation can restore
  // a page with a different media capability than its first load.
  if (!video || !isPipSupported()) return
  try {
    if (document.pictureInPictureElement === video) await document.exitPictureInPicture()
    else await video.requestPictureInPicture()
  } catch {
    // Permission/transient failure — the button is best-effort.
  }
}
