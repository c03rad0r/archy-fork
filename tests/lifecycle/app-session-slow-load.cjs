// Real served dashboard, isolated delayed app document; no app mutation or signing.
const fs = require('fs');
const { chromium, expect } = require(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
(async () => {
  const origin = process.env.QUALIFICATION_ORIGIN;
  const cookieFile = process.env.QUALIFICATION_COOKIES;
  const node = process.env.QUALIFICATION_LABEL || 'node';
  if (!origin || !cookieFile) throw new Error('Set private node origin and cookie file');
  const url = new URL(origin), bytes = url.hostname.split('.').map(Number);
  const privateHost = url.hostname === 'localhost' || (bytes.length === 4 && bytes.every(n => Number.isInteger(n) && n >= 0 && n <= 255) &&
    (bytes[0] === 127 || bytes[0] === 10 || (bytes[0] === 192 && bytes[1] === 168) || (bytes[0] === 172 && bytes[1] >= 16 && bytes[1] <= 31) || (bytes[0] === 100 && bytes[1] >= 64 && bytes[1] <= 127)));
  if (!privateHost || !['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new Error('Refusing non-private node origin');
  const browser = await chromium.connectOverCDP(process.env.BROWSER_CDP || 'http://127.0.0.1:32911');
  for (const width of [390, 1440]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, serviceWorkers: 'block' });
    let releaseDocument;
    const heldDocument = new Promise(resolve => { releaseDocument = resolve; });
    let documentRequests = 0;
    try {
      const cookies = JSON.parse(fs.readFileSync(cookieFile, 'utf8'));
      await context.addCookies(Object.entries(cookies).map(([name, value]) => ({ name, value, url: origin, httpOnly: name !== 'csrf_token' })));
      await context.addInitScript(() => {
        localStorage.setItem('neode-auth', 'true');
        localStorage.setItem('lnd-seed-backup-prompt-snooze-until', String(Date.now() + 3600000));
      });
      await context.route('**:7778/**', async route => {
        if (route.request().resourceType() !== 'document') return route.abort();
        documentRequests++;
        await heldDocument;
        await route.fulfill({ contentType: 'text/html', body: '<!doctype html><html><body><h1 id="ready">Delayed app ready</h1></body></html>' });
      });
      // Explicitly deny signing in this loading-only fixture.
      await context.route('**/rpc/v1', async route => {
        let r; try { r = route.request().postDataJSON(); } catch { return route.continue(); }
        if (/nostr-sign|identity.sign|\.(pay|send|spend|melt|withdraw)(-|$)/.test(r?.method || '')) return route.abort();
        return route.continue();
      });
      const page = await context.newPage();
      await page.goto(origin + '/dashboard/app-session/indeedhub', { waitUntil: 'domcontentloaded' });
      const frame = page.locator('iframe[src*="7778"]');
      await expect(frame).toHaveCount(1, { timeout: 30000 });
      await frame.evaluate(el => { window.__qualificationFrame = el; });
      const notice = page.getByRole('status').filter({ hasText: 'is taking longer to load' });
      await expect(notice).toBeVisible({ timeout: 20000 });
      expect(await frame.evaluate(el => el === window.__qualificationFrame)).toBe(true);
      await expect(page.getByRole('button', { name: 'Close companion modal', exact: true })).toHaveCount(0);
      releaseDocument();
      await expect(page.frameLocator('iframe[src*="7778"]').locator('#ready')).toHaveText('Delayed app ready', { timeout: 15000 });
      await expect(notice).toHaveCount(0);
      expect(await frame.evaluate(el => el === window.__qualificationFrame)).toBe(true);
      // Modify readiness only in this isolated browser's store, never the node.
      for (const ready of [false, true]) {
        await page.evaluate(ready => {
          const app = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('app');
          app.data['package-data'].indeedhub['ui-ready'] = ready;
        }, ready);
        await page.waitForTimeout(300);
        await expect(frame).toHaveCount(1);
        expect(await frame.evaluate(el => el === window.__qualificationFrame)).toBe(true);
      }
      expect(documentRequests).toBe(1);
      console.log(JSON.stringify({ node, width, result: 'PASS', delayedBeyondDeadline: true, sameIframeRetained: true, readinessFlapRetained: true, companionOfferDeferred: true, documentRequests }));
    } finally {
      releaseDocument();
      await context.close();
    }
  }
  process.exit(0);
})().catch(e => { console.error(String(e.message).split('Call log:')[0]); process.exit(1); });
