import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { expect, test } from '@playwright/test'

const providerSource = readFileSync(
  resolve(process.cwd(), 'public/nostr-provider.js'),
  'utf8',
)

test('mobile Chromium returns to the live app after the signer is hidden', async ({ context, page }) => {
  await page.setViewportSize({ width: 390, height: 844 })

  await context.route('**/*', async (route) => {
    const url = new URL(route.request().url())
    if (url.pathname === '/nostr-provider.js') {
      await route.fulfill({ contentType: 'application/javascript', body: providerSource })
      return
    }
    if (url.port === '' && url.pathname === '/nostr-signer') {
      await route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><html><body style="margin:0;background:#111827">
          <script>
            addEventListener('message', (event) => {
              if (event.data && event.data.type === 'archipelago:signer-init') {
                parent.postMessage({ type: 'archipelago:signer-show' }, event.origin)
                setTimeout(() => {
                  parent.postMessage({
                    type: 'archipelago:signer-identity',
                    identity: { nostr_pubkey: 'browser-handoff-key' },
                  }, event.origin)
                  parent.postMessage({ type: 'archipelago:signer-hide' }, event.origin)
                }, 50)
              }
            })
            parent.postMessage({ type: 'archipelago:signer-ready' }, '*')
          </script>
        </body></html>`,
      })
      return
    }
    if (url.port === '7778') {
      await route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><html><head><title>IndeedHub</title>
          <script src="/nostr-provider.js" data-no-nip98></script>
          <style>html,body{margin:0;background:#166534}#app{height:100vh;color:white;display:grid;place-items:center}</style>
        </head><body><main id="app">app-ready</main><script>
          addEventListener('load', () => {
            archipelagoNostr.onIdentitySelected((identity) => {
              document.querySelector('#app').textContent = 'signed-in:' + identity.nostr_pubkey
            })
          })
        </script></body></html>`,
      })
      return
    }
    await route.abort()
  })

  await page.goto('http://app.test:7778/')
  await expect(page.locator('#app')).toHaveText('signed-in:browser-handoff-key')

  const broker = page.locator('#archipelago-nostr-signer')
  await expect(broker).toHaveCount(1)
  await expect(broker).toHaveCSS('width', '1px')
  await expect(broker).toHaveCSS('height', '1px')
  await expect(broker).toHaveCSS('opacity', '0')
  await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(22, 101, 52)')
  expect(await page.evaluate(() => document.elementFromPoint(195, 422)?.id)).toBe('app')
})
