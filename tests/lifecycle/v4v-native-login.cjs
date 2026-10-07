// Authorized, real V4V native login. No payment or relay publication permitted.
const fs = require('node:fs');
const { chromium, expect } = require(process.env.PLAYWRIGHT_MODULE || '../../neode-ui/node_modules/@playwright/test');
(async () => {
  if (process.env.ALLOW_REAL_AUTH_SIGNING !== '1') throw Error('Explicit real-auth test authorization required');
  const origin = new URL(process.env.QUALIFICATION_ORIGIN).origin;
  const parsed = new URL(origin);
  const octets = parsed.hostname.split('.').map(Number);
  const local = parsed.hostname === 'localhost' || (octets.length === 4 && octets.every(n => Number.isInteger(n) && n >= 0 && n <= 255) && (octets[0] === 127 || octets[0] === 10 || (octets[0] === 192 && octets[1] === 168) || (octets[0] === 172 && octets[1] >= 16 && octets[1] <= 31)));
  if (!local || !['http:', 'https:'].includes(parsed.protocol)) throw Error('Private node origin required');
  parsed.port = '7475'; const appOrigin = parsed.origin;
  const ownBrowser = process.env.QUALIFICATION_LAUNCH_BROWSER === '1';
  const browser = ownBrowser ? await chromium.launch({ headless: true })
    : await chromium.connectOverCDP(process.env.BROWSER_CDP || 'http://127.0.0.1:32911');
  try {
  for (const width of process.env.TEST_WIDTH ? [Number(process.env.TEST_WIDTH)] : [390, 1440]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, serviceWorkers: 'block' });
    const proof = { width, signatures: 0, login: [], blocked: false };
    const challenges = new Set();
    let page;
    let step = 'load dashboard';
    try {
      const cookies = JSON.parse(fs.readFileSync(process.env.QUALIFICATION_COOKIES, 'utf8'));
      await context.addCookies(Object.entries(cookies).map(([name, value]) => ({ name, value, url: origin, httpOnly: name !== 'csrf_token' })));
      await context.addInitScript(() => { localStorage.setItem('neode-auth', 'true'); localStorage.setItem('neode_onboarding_complete', '1'); localStorage.setItem('neode_companion_intro_seen', 'build:54'); });
      await context.route('**/rpc/v1', async route => {
        let rpc; try { rpc = route.request().postDataJSON(); } catch { return route.continue(); }
        const method = rpc?.method || '';
        if (['identity.nostr-sign', 'node.nostr-sign'].includes(method)) {
          const event = rpc.params?.event;
          const tags = event?.tags;
          const valid = event?.kind === 21236 && event.content === '' && Array.isArray(tags) && tags.length === 3 && tags.every(t => Array.isArray(t) && t.length === 2) && tags.some(t => t[0] === 'origin' && t[1] === appOrigin) && tags.some(t => t[0] === 'challenge' && challenges.has(t[1])) && tags.some(t => t[0] === 'app' && t[1] === 'v4v') && Math.abs(Date.now() / 1000 - event.created_at) < 60;
          if (!valid || proof.signatures >= 2) { proof.blocked = true; return route.abort(); }
          proof.signatures++;
        } else if (method === 'identity.sign') {
          if (!/^archipelago-identity:\d+$/.test(rpc.params?.message || '')) { proof.blocked = true; return route.abort(); }
        } else if (/nostr-(encrypt|decrypt)|(?:^|[.-])(pay|send|spend|melt|withdraw|publish|transfer)(?:[.-]|$)/.test(method)) {
          proof.blocked = true; return route.abort();
        }
        return route.continue();
      });
      // Record the actual challenge before allowing the corresponding signature.
      await context.route(appOrigin + '/api/auth/nostr/challenge', async route => {
        const response = await route.fetch(); const body = await response.json();
        if (body.enabled === true && /^[0-9a-f]{64}$/.test(body.challenge)) challenges.add(body.challenge);
        await route.fulfill({ response });
      });
      if (process.env.QUALIFY_PLAYBACK === '1') {
        await context.addInitScript(() => {
          window.__qualificationMedia = [];
          const play = HTMLMediaElement.prototype.play;
          HTMLMediaElement.prototype.play = function (...args) {
            this.muted = true;
            if (!window.__qualificationMedia.includes(this)) window.__qualificationMedia.push(this);
            return play.apply(this, args);
          };
        });
        await context.route(appOrigin + '/api/**', async route => {
          const request = route.request(); const path = new URL(request.url()).pathname;
          if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method()) && /(?:pay|invoice|tip|purchase|checkout|melt|withdraw|spend|zap|fund|stream-credit)/i.test(path)) {
            proof.blocked = true; return route.abort();
          }
          return route.fallback();
        });
      }
      page = await context.newPage();
      page.on('response', response => { const u = new URL(response.url()); if (u.origin === appOrigin && u.pathname === '/api/auth/nostr/login') proof.login.push(response.status()); });
      await page.goto(origin + '/dashboard/discover', { waitUntil: 'domcontentloaded', timeout: 60000 });
      await page.waitForFunction(() => document.querySelector('#app')?.__vue_app__?.config.globalProperties.$pinia?._s.has('appLauncher'), undefined, { timeout: 30000 });
      step = 'open managed app';
      await page.evaluate(() => document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('appLauncher').openSession('node-demo-v4v'));
      const frame = page.frameLocator('iframe[src*="7475"]');
      const signIn = frame.getByRole('button', { name: 'Sign in with Nostr', exact: true });
      await expect(signIn).toBeVisible({ timeout: 45000 });
      proof.initialLaunchPath = await page.locator('iframe[src*="7475"]').evaluate(frame => new URL(frame.src).pathname);
      await expect(frame.locator('input[type=password]')).toHaveCount(0);
      // A fresh browser gets the real app introduction before authentication.
      // Drive its visible controls instead of clicking through the overlay.
      const introPlay = frame.getByRole('button', { name: 'Play intro', exact: true });
      const introSkip = frame.getByRole('button', { name: 'Skip intro', exact: true });
      step = 'wait for app introduction';
      await expect(introPlay).toBeVisible();
      step = 'play app introduction';
      await introPlay.click();
      step = 'skip app introduction';
      await expect(introSkip).toBeVisible();
      await introSkip.click();
      step = 'click native Nostr sign-in';
      await signIn.click();
      const deadline = Date.now() + 45000; let approvals = 0;
      while (Date.now() < deadline && !proof.login.includes(200)) {
        if (proof.blocked) throw Error('Unexpected signing or payment request blocked');
        for (const name of ['Authenticate', 'Approve']) {
          const button = page.getByRole('button', { name, exact: true });
          if (await button.isVisible().catch(() => false)) { if (++approvals > 6) throw Error('Repeated consent loop'); step = 'native consent: ' + name; await button.click(); await expect(button).toBeHidden(); }
        }
        await page.waitForTimeout(200);
      }
      expect(proof.blocked).toBe(false); expect(proof.signatures).toBeGreaterThan(0); expect(proof.login).toContain(200);
      await expect(signIn).toHaveCount(0, { timeout: 15000 });
      step = 'native playback controls';
      if (process.env.QUALIFY_PLAYBACK === '1') await require('./v4v-native-playback.cjs')(page, appOrigin, proof);
      console.log(JSON.stringify({ ...proof, result: proof.initialBrowse === false ? 'PARTIAL_PASS_BROWSE_OPEN' : 'PASS', scope: process.env.QUALIFY_PLAYBACK === '1' ? 'real managed V4V native login and bundled-song playback' : 'real managed V4V native authentication only; playback remains separately qualified' }));
    } catch (error) {
      const prefix = '/tmp/archy-v4v-native-failure-' + width;
      fs.writeFileSync(prefix + '.private.txt', String(error.stack || error), { mode: 0o600 });
      if (page) await page.screenshot({ path: prefix + '.png' }).then(() => fs.chmodSync(prefix + '.png', 0o600)).catch(() => {});
      console.error(JSON.stringify({ width, failedStep: step, signatures: proof.signatures, loginStatuses: proof.login }));
      throw error;
    } finally { await context.unrouteAll({ behavior: 'ignoreErrors' }).catch(() => {}); await context.close(); }
  }
  } finally { if (ownBrowser) await browser.close(); }
})().then(() => process.exit(0)).catch(error => { console.error(String(error.message).split(/Call [Ll]og:/)[0]); process.exit(1); });
