// Invoked only after the real managed app has completed native Nostr login.
// No fixture media, intercepted success, payment or application state fabrication.
const { expect } = require(process.env.PLAYWRIGHT_MODULE || '../../neode-ui/node_modules/@playwright/test');
module.exports = async function qualifyPlayback(page, appOrigin, proof) {
  const app = page.frames().find(frame => frame.url().startsWith(appOrigin + '/'));
  if (!app) throw Error('Managed V4V iframe missing after login');
  await app.waitForFunction(() => window.PulsewireLegacy?.getReactBridge?.(), undefined, { timeout: 30000 });
  const later = app.getByRole('button', { name: 'Not now', exact: true });
  if (await later.isVisible().catch(() => false)) await later.click();
  await app.waitForFunction(() => {
    const bridge = window.PulsewireLegacy.getReactBridge();
    return bridge.getState().releases.filter(release => {
      try { const url = new URL(bridge.helpers.releasePlayableUrl(release), location.href);
        return url.origin === location.origin && url.pathname.startsWith('/media/dev-audio/');
      } catch { return false; }
    }).length >= 2;
  }, undefined, { timeout: 30000 });
  const tracks = await app.evaluate(() => {
    const bridge = window.PulsewireLegacy.getReactBridge();
    return bridge.getState().releases.filter(release => {
      try {
        const url = new URL(bridge.helpers.releasePlayableUrl(release), location.href);
        return url.origin === location.origin && url.pathname.startsWith('/media/dev-audio/');
      } catch { return false; }
    }).slice(0, 2).map(release => ({ id: release.id, title: release.title }));
  });
  if (tracks.length !== 2) throw Error('Need two existing bundled demo tracks for a no-payment playback check');
  await app.evaluate(async ids => {
    const bridge = window.PulsewireLegacy.getReactBridge();
    if (bridge.getState().ui.shuffleEnabled) bridge.actions.toggleShuffle();
    bridge.actions.clearQueue();
    bridge.actions.play(ids[0]);
    bridge.actions.queueRelease(ids[1]);
  }, tracks.map(track => track.id));
  await app.waitForFunction(() => window.__qualificationMedia?.some(audio => !audio.paused && audio.readyState >= 2 && audio.currentTime > 0), undefined, { timeout: 30000 });
  const initial = await app.evaluate(() => window.__qualificationMedia.find(audio => !audio.paused).currentTime);
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  const backupLater = page.getByRole('button', { name: 'Later', exact: true });
  if (await backupLater.isVisible().catch(() => false)) await backupLater.click();
  const bar = page.locator('.audio-player-bar');
  await expect(bar).toBeVisible();
  await expect(bar).toContainText(tracks[0].title);
  await app.waitForFunction(time => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime > time + 1), initial, { timeout: 15000, polling: 100 });
  await page.getByRole('button', { name: 'Pause audio', exact: true }).click();
  await app.waitForFunction(() => window.__qualificationMedia.every(audio => audio.paused), undefined, { polling: 100 });
  await page.getByRole('button', { name: 'Play audio', exact: true }).click();
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused), undefined, { polling: 100 });
  await page.getByRole('button', { name: 'Next track', exact: true }).click();
  await app.waitForFunction(id => window.PulsewireLegacy.getReactBridge().helpers.activePlayerRelease()?.id === id, tracks[1].id, { polling: 100 });
  await expect(bar).toContainText(tracks[1].title);
  // Previous after >3 seconds intentionally restarts the current song in V4V.
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime > 4), undefined, { polling: 100 });
  await page.getByRole('button', { name: 'Previous track', exact: true }).click();
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime < 3), undefined, { polling: 100 });
  await page.getByRole('button', { name: 'Shuffle', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Shuffle', exact: true })).toHaveAttribute('aria-pressed', 'true');
  expect(await app.evaluate(() => window.PulsewireLegacy.getReactBridge().getState().ui.shuffleEnabled)).toBe(true);
  const artwork = await app.evaluate(() => window.PulsewireLegacy.getReactBridge().helpers.activePlayerRelease()?.cover || '');
  if (artwork) await expect(bar.locator('img')).toHaveAttribute('src', new URL(artwork, appOrigin).href);
  const beforeReopen = await app.evaluate(() => window.__qualificationMedia.find(audio => !audio.paused).currentTime);
  await page.getByRole('button', { name: 'Open playing app', exact: true }).click();
  if (!page.frames().includes(app)) throw Error('Reopening recreated the playback iframe');
  await expect(bar).toBeHidden();
  await app.waitForFunction(time => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime >= time), beforeReopen);
  await app.evaluate(() => window.PulsewireLegacy.getReactBridge().actions.pauseAudio());
  if (proof.blocked) throw Error('Unexpected payment/signing attempted during playback');
  proof.playback = { actualDemoMedia: true, hiddenAudioContinues: true, controls: ['pause', 'play', 'next', 'previous', 'shuffle'], sameIframe: true, artworkChecked: Boolean(artwork), muted: true };
};
