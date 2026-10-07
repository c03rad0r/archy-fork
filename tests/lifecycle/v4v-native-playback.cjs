// Invoked only after the real managed app has completed native Nostr login.
// No fixture media, intercepted success, payment or application state fabrication.
const { expect } = require(process.env.PLAYWRIGHT_MODULE || '../../neode-ui/node_modules/@playwright/test');
module.exports = async function qualifyPlayback(page, appOrigin, proof) {
  const nativeBridge = process.env.QUALIFY_COMPANION_BRIDGE === '1';
  async function control(command, button) {
    if (!nativeBridge) return page.getByRole('button', { name: button, exact: true }).click();
    await page.evaluate(command => {
      const state = window.__qualificationAudioBridge?.filter(value => value.action === 'state').at(-1);
      if (!state?.session || !window.ArchipelagoAudio?.onmessage) throw Error('Companion audio session missing');
      window.ArchipelagoAudio.onmessage({ data: JSON.stringify({ version: 1, type: 'command', session: state.session, command }) });
    }, command);
  }
  const app = page.frames().find(frame => frame.url().startsWith(appOrigin + '/'));
  if (!app) throw Error('Managed V4V iframe missing after login');
  proof.initialBrowse = new URL(app.url()).pathname === '/browse';
  if (process.env.QUALIFY_ARTWORK_ONLY !== '1') {
    await expect.poll(() => new URL(app.url()).pathname).toBe('/browse');
    proof.initialBrowse = true;
  }
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
  await control('pause', 'Pause audio');
  await app.waitForFunction(() => window.__qualificationMedia.every(audio => audio.paused), undefined, { polling: 100 });
  await control('play', 'Play audio');
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused), undefined, { polling: 100 });
  await control('next', 'Next track');
  await app.waitForFunction(id => window.PulsewireLegacy.getReactBridge().helpers.activePlayerRelease()?.id === id, tracks[1].id, { polling: 100 });
  await expect(bar).toContainText(tracks[1].title);
  // Previous after >3 seconds intentionally restarts the current song in V4V.
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime > 4), undefined, { polling: 100 });
  await control('previous', 'Previous track');
  await app.waitForFunction(() => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime < 3), undefined, { polling: 100 });
  await control('shuffle', 'Shuffle');
  await expect(page.getByRole('button', { name: 'Shuffle', exact: true })).toHaveAttribute('aria-pressed', 'true');
  expect(await app.evaluate(() => window.PulsewireLegacy.getReactBridge().getState().ui.shuffleEnabled)).toBe(true);
  const artwork = await app.evaluate(() => window.PulsewireLegacy.getReactBridge().helpers.activePlayerRelease()?.cover || '');
  if (artwork) {
    const background = bar.locator('img.player-artwork');
    await expect(background).toHaveAttribute('src', new URL(artwork, appOrigin).href);
    await expect.poll(() => background.evaluate(img => img.complete && img.naturalWidth > 0)).toBe(true);
    const geometry = await background.evaluate(img => {
      const image = img.getBoundingClientRect(); const bar = img.parentElement.getBoundingClientRect();
      const style = getComputedStyle(img);
      return { image: { x: image.x, y: image.y, width: image.width, height: image.height }, bar: { x: bar.x, y: bar.y, width: bar.width, height: bar.height }, opacity: Number(style.opacity), fit: style.objectFit };
    });
    expect(geometry.opacity).toBeGreaterThan(0); expect(geometry.fit).toBe('cover');
    for (const axis of ['x', 'y', 'width', 'height']) expect(Math.abs(geometry.image[axis] - geometry.bar[axis])).toBeLessThanOrEqual(2);
    proof.renderedArtwork = true;
  }
  if (nativeBridge) {
    await expect.poll(() => page.evaluate(() => window.__qualificationAudioBridge?.filter(value => value.action === 'state').at(-1)?.title)).toContain(tracks[1].title);
    const nativeState = await page.evaluate(() => window.__qualificationAudioBridge.filter(value => value.action === 'state').at(-1));
    expect(nativeState.playing).toBe(true);
    expect(nativeState.session).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(nativeState.shuffled).toBe(true);
    if (artwork) await expect.poll(() => page.evaluate(() => window.__qualificationAudioBridge.some(value => value.action === 'state' && value.artwork?.startsWith('data:image/jpeg;base64,'))), { timeout: 15000 }).toBe(true);
    proof.companionBridge = { realBrowserPlayer: true, nativeTransportSimulated: true, controls: ['pause', 'play', 'next', 'previous', 'shuffle'], thumbnail: Boolean(artwork), physicalAndroid: false };
  }
  const barBounds = await bar.boundingBox();
  for (const name of ['Previous track', 'Next track', 'Open playing app']) {
    const control = bar.getByRole('button', { name, exact: true });
    await expect(control).toBeVisible(); const bounds = await control.boundingBox();
    expect(bounds.x).toBeGreaterThanOrEqual(barBounds.x);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(barBounds.x + barBounds.width);
    expect(bounds.y).toBeGreaterThanOrEqual(barBounds.y);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(barBounds.y + barBounds.height);
  }
  const screenshot = '/tmp/archy-v4v-live-player-' + proof.width + '.png';
  await page.screenshot({ path: screenshot }); require('node:fs').chmodSync(screenshot, 0o600);

  const beforeReopen = await app.evaluate(() => window.__qualificationMedia.find(audio => !audio.paused).currentTime);
  await page.getByRole('button', { name: 'Open playing app', exact: true }).click();
  if (!page.frames().includes(app)) throw Error('Reopening recreated the playback iframe');
  await expect(bar).toBeHidden();
  await app.waitForFunction(time => window.__qualificationMedia.some(audio => !audio.paused && audio.currentTime >= time), beforeReopen);
  await app.evaluate(() => window.PulsewireLegacy.getReactBridge().actions.pauseAudio());
  if (proof.blocked) throw Error('Unexpected payment/signing attempted during playback');
  proof.playback = { actualDemoMedia: true, hiddenAudioContinues: true, controls: ['pause', 'play', 'next', 'previous', 'shuffle'], sameIframe: true, artworkChecked: Boolean(artwork), muted: true };
};
