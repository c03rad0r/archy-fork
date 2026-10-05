// Real File Browser acceptance: only unique test files are created/deleted.
// CLOUD_URL=http://node CLOUD_COOKIE_FILE=/private/session.json node this-file
const fs = require('node:fs');
const crypto = require('node:crypto');
const { chromium, expect } = require('../../neode-ui/node_modules/@playwright/test');
const base = process.env.CLOUD_URL;
if (!base || !process.env.CLOUD_COOKIE_FILE) throw Error('Set CLOUD_URL and CLOUD_COOKIE_FILE');
(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const width of [1440, 390]) {
      const ctx = await browser.newContext({ viewport: { width, height: 950 }, serviceWorkers: 'block' });
      await ctx.addCookies(Object.entries(JSON.parse(fs.readFileSync(process.env.CLOUD_COOKIE_FILE))).map(([name, value]) => ({ name, value, domain: new URL(base).hostname, path: '/' })));
      const meta = await ctx.request.get(`${base}/packages/archipelago-companion.json`).then(r => r.json());
      await ctx.addInitScript(marker => { localStorage.setItem('neode-auth', 'true'); localStorage.setItem('neode_companion_intro_seen', marker) }, meta.versionCode ? `build:${meta.versionCode}` : `version:${meta.versionName}`);
      const page = await ctx.newPage();
      const lifecycle = await ctx.newCDPSession(page);
      await page.goto(`${base}/dashboard/cloud/files`, { waitUntil: 'domcontentloaded', timeout: 60000 });
      await page.locator('button[title="Upload file"]').waitFor({ timeout: 60000 });
      await page.evaluate(() => {
        const app = document.querySelector('#app').__vue_app__;
        window.testRouter = app.config.globalProperties.$router;
        window.testCloud = Reflect.ownKeys(app._context.provides).map(k => app._context.provides[k]).find(v => v?._s?.has('cloud'))._s.get('cloud');
      });
      const dest = await page.evaluate(() => window.testCloud.currentPath);
      const tag = `archy-resume-${width}-${Date.now()}`;
      const names = [`${tag} + 100% ü.txt`, `${tag}-cancel.txt`, `${tag}-never.txt`, `${tag}-empty.txt`];
      const bytes = crypto.randomBytes(5 * 1024 * 1024 + 123);
      const expected = crypto.createHash('sha256').update(bytes).digest('hex');
      const offsets = [];
      let partial = false, final = false, renamed = false, posts = 0;
      let refreshArmed = false, refreshAttempts = 0;
      const authResponses = [];
      page.on('response', async response => {
        if (!response.url().endsWith('/rpc/v1') || response.request().postDataJSON()?.method !== 'app.filebrowser-token') return;
        const body = await response.json().catch(() => ({}));
        authResponses.push({ status: response.status(), tokenPresent: !!body.result?.token,
          error: body.error ? String(body.error.message || body.error).slice(0, 180) : null });
      });
      await page.route('**/rpc/v1', async route => {
        const body = route.request().postDataJSON();
        if (refreshArmed && body?.method === 'app.filebrowser-token') {
          refreshAttempts++;
          if (refreshAttempts === 1) return route.abort('connectionreset');
        }
        return route.continue();
      });
      await page.route('**/api/tus/**', async route => {
        const req = route.request();
        if (req.method() === 'POST') posts++;
        if (req.method() !== 'PATCH') return route.continue();
        offsets.push(Number(req.headers()['upload-offset']));
        if (!partial) {
          partial = true;
          const response = await route.fetch({ postData: req.postDataBuffer().subarray(0, 123456) });
          if (response.status() !== 204) throw Error('Partial chunk rejected: ' + response.status());
          return route.abort('connectionreset');
        }
        if (!final && offsets[offsets.length - 1] + req.postDataBuffer().length === bytes.length) {
          final = true;
          const response = await route.fetch();
          if (response.status() !== 204) throw Error('Final chunk rejected: ' + response.status());
          return route.abort('connectionreset');
        }
        return route.continue();
      });
      await page.route('**/api/resources/**', async route => {
        if (route.request().method() === 'PATCH' && !renamed) {
          renamed = true;
          const response = await route.fetch();
          if (!response.ok()) throw Error('Rename rejected: ' + response.status());
          return route.abort('connectionreset');
        }
        return route.continue();
      });
      try {
        await page.locator('input[type=file]').setInputFiles({ name: names[0], mimeType: 'application/octet-stream', buffer: bytes });
        await page.getByText(`Connection interrupted · resuming ${names[0]}`, { exact: true }).waitFor({ timeout: 30000 });
        await ctx.setOffline(true);
        refreshArmed = true;
        await page.evaluate(() => { document.cookie = 'auth=; path=/; Max-Age=0' });
        await lifecycle.send('Page.setWebLifecycleState', { state: 'frozen' });
        await new Promise(resolve => setTimeout(resolve, 1200));
        await ctx.setOffline(false);
        await lifecycle.send('Page.setWebLifecycleState', { state: 'active' });
        await page.evaluate(async () => {
          const failure = await window.testRouter.push('/dashboard/apps');
          if (failure) throw Error('Apps navigation failed: ' + failure.message);
        });
        await expect(page).toHaveURL(/\/dashboard\/apps$/, { timeout: 15000 });
        await expect(page.getByRole('button', { name: 'Cancel upload', exact: true })).toHaveCount(0);
        await page.getByText(`Upload complete · ${names[0]}`, { exact: true }).waitFor({ timeout: 90000 });
        await page.getByRole('button', { name: 'View upload', exact: false }).click();
        await page.getByText(`Complete · ${names[0]}`, { exact: true }).waitFor();
        const height = await page.getByRole('button', { name: 'Dismiss upload', exact: true }).locator('..').evaluate(el => el.getBoundingClientRect().height);
        if (height !== 44 || posts !== 1 || offsets[1] !== 123456 || !final || !renamed || refreshAttempts < 2) throw Error('Resume/commit/auth-refresh/bar invariant failed');
        const actual = await page.evaluate(async ({ dest, name }) => {
          const url = await window.testCloud.fetchBlobUrl(dest.replace(/\/$/, '') + '/' + name);
          const data = await (await fetch(url)).arrayBuffer(); URL.revokeObjectURL(url);
          // crypto.subtle is unavailable on plain LAN HTTP; return base64 to hash in harness.
          let text = ''; for (const n of new Uint8Array(data)) text += String.fromCharCode(n);
          return btoa(text);
        }, { dest, name: names[0] });
        if (crypto.createHash('sha256').update(Buffer.from(actual, 'base64')).digest('hex') !== expected) throw Error('Saved bytes mismatch');
        await page.getByRole('button', { name: 'Dismiss upload', exact: true }).click();
        await page.unroute('**/api/tus/**'); await page.unroute('**/api/resources/**');
        let cancelPatch;
        const seen = new Promise(resolve => cancelPatch = resolve);
        await page.route('**/api/tus/**', async route => {
          if (route.request().method() === 'PATCH') { cancelPatch(); return route.abort('connectionreset') }
          return route.continue();
        });
        await page.locator('input[type=file]').setInputFiles(names.slice(1, 3).map(name => ({ name, mimeType: 'text/plain', buffer: Buffer.from('cancel fixture') })));
        await seen;
        await page.getByRole('button', { name: 'Cancel upload', exact: true }).click();
        await page.getByText('Upload stopped · 0/2 saved', { exact: true }).waitFor({ timeout: 15000 });
        await page.unroute('**/api/tus/**');
        await page.getByRole('button', { name: 'Dismiss upload', exact: true }).click();
        await page.locator('input[type=file]').setInputFiles({ name: names[3], mimeType: 'text/plain', buffer: Buffer.alloc(0) });
        await page.getByText(`Complete · ${names[3]}`, { exact: true }).waitFor({ timeout: 30000 });
        console.log('PASS real upload', width, 'partial-write resume at123456, offline frozen-page return, interrupted JWT refresh, final ACK loss, rename ACK loss, exact SHA256, encoded name, origin-only44px, notification, cancel queue, empty file');
      } catch (error) {
        console.error('Token refresh diagnostics:', { refreshAttempts, authResponses });
        console.error('Upload state at failure:', await page.evaluate(() => ({ route: window.testRouter.currentRoute.value.fullPath, origin: window.testCloud.upload?.originRoute, active: window.testCloud.upload?.active, error: window.testCloud.upload?.error, sent: window.testCloud.upload?.sent, paused: window.testCloud.upload?.paused })));
        throw error;
      } finally {
        await lifecycle.send('Page.setWebLifecycleState', { state: 'active' });
        await ctx.setOffline(false);
        await page.unrouteAll({ behavior: 'ignoreErrors' });
        await page.evaluate(async ({ dest, names }) => {
          window.testCloud.cancelUpload();
          for (const name of names) { try { await window.testCloud.deleteItem(dest.replace(/\/$/, '') + '/' + name) } catch {} }
        }, { dest, names });
        await ctx.close();
      }
    }
  } finally { await browser.close() }
})().catch(e => { console.error(e.message); process.exitCode = 1 });
