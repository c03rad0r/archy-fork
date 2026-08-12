import { chromium } from './node_modules/playwright/index.mjs';

// Node under test + credentials come from the environment; never commit literals.
//   ARCHY_NODE_URL=https://<node> ARCHY_NODE_PW=… node test-openwrt.mjs
const BASE  = process.env.ARCHY_NODE_URL || 'https://127.0.0.1';
const PASS  = process.env.ARCHY_NODE_PW;
const DIR   = process.env.ARCHY_OUT_DIR || '/tmp/openwrt-test';

if (!PASS) {
  console.error('ERROR: ARCHY_NODE_PW must be set in the environment.');
  process.exit(2);
}

// Find the OpenWrt router IP from the Tailscale/LAN
const { execSync } = await import('child_process');
let routerIp = '192.168.1.1';
try {
  const sshTarget = process.env.ARCHY_NODE_SSH || `archipelago@${new URL(BASE).hostname}`;
  const route = execSync(`ssh ${sshTarget} 'ip route | grep default'`, { encoding: 'utf8' }).trim();
  const match = route.match(/default via ([\d.]+)/);
  if (match) routerIp = match[1];
} catch {}
console.log('Detected router IP:', routerIp);

const browser = await chromium.launch({ headless: true });
const ctx = await browser.newContext({ ignoreHTTPSErrors: true });
const page = await ctx.newPage();

// Login
await page.goto(`${BASE}/login`, { waitUntil: 'networkidle', timeout: 20000 });
await page.fill('input[type="password"]', PASS);
await page.keyboard.press('Enter');
await page.waitForURL(/dashboard/, { timeout: 20000 });

// Server page
await page.goto(`${BASE}/dashboard/server`, { waitUntil: 'networkidle', timeout: 20000 });
await page.screenshot({ path: `${DIR}/01-server.png` });

// Click OpenWrt Gateway
await page.getByText('OpenWrt Gateway').click();
await page.waitForURL(/openwrt/, { timeout: 10000 });
await page.waitForTimeout(2000);
await page.screenshot({ path: `${DIR}/02-connect-form.png` });
console.log('Connect form visible');

// Fill in the connect form
await page.fill('input[placeholder="192.168.1.1"]', routerIp);
await page.screenshot({ path: `${DIR}/03-form-filled.png` });
await page.getByRole('button', { name: 'Connect' }).click();

// Wait for SSH connection + UCI read (can take up to 30s)
console.log('Connecting to router, waiting for data...');
await page.waitForTimeout(15000);
await page.screenshot({ path: `${DIR}/04-result.png` });
console.log('Result page text:\n', (await page.innerText('body')).replace(/\s+/g, ' ').substring(0, 800));

await browser.close();
