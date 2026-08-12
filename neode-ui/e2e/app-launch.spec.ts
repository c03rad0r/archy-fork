import { expect, test, type Page } from '@playwright/test'

const PASSWORD = process.env.ARCHY_PASSWORD ?? 'password123'
const APP_ID = process.env.ARCHY_APP_ID ?? 'lnd'
const APP_TITLE = process.env.ARCHY_APP_TITLE ?? APP_ID
const APP_CARD_TITLE = process.env.ARCHY_APP_CARD_TITLE ?? APP_TITLE
const EXPECTED_URL = process.env.ARCHY_EXPECTED_LAUNCH_URL
const EXPECTED_URL_PATTERN = process.env.ARCHY_EXPECTED_LAUNCH_URL_PATTERN
const EXPECTED_BODY_PATTERN = process.env.ARCHY_EXPECTED_BODY_PATTERN ?? 'Connect Your Wallet|lndconnect|REST|gRPC'
const EXPECTED_MODE = process.env.ARCHY_EXPECTED_LAUNCH_MODE ?? 'popup'

async function login(page: Page) {
  await page.goto('/login', { waitUntil: 'domcontentloaded' })
  await page.evaluate(() => {
    localStorage.setItem('neode_intro_seen', '1')
    localStorage.setItem('neode_onboarding_complete', '1')
  })
  await page.goto('/login', { waitUntil: 'networkidle' })

  const passwordInput = page.locator('input[type="password"]').first()
  await passwordInput.waitFor({ timeout: 15_000 })
  await passwordInput.fill(PASSWORD)
  await page.locator('button:has-text("Login"), button:has-text("Unlock"), button:has-text("Continue"), button[type="submit"]').first().click()
  await page.waitForURL('**/dashboard**', { timeout: 20_000 })
}

test('installed app launch opens reachable app URL', async ({ page, context, baseURL }) => {
  test.skip(!EXPECTED_URL, 'Set ARCHY_EXPECTED_LAUNCH_URL for launch qualification')

  await login(page)
  await page.goto('/dashboard/apps', { waitUntil: 'domcontentloaded' })

  const appCard = page.locator('[data-controller-container]', {
    has: page.getByRole('heading', { name: APP_CARD_TITLE, exact: true }),
  }).first()
  await appCard.waitFor({ timeout: 30_000 })
  const launchButton = appCard.locator('[data-controller-launch-btn], button:has-text("Launch")').first()
  await launchButton.waitFor({ timeout: 20_000 })

  if (EXPECTED_MODE === 'panel') {
    await launchButton.click()
    const expected = new URL(EXPECTED_URL!, baseURL)
    const frameSelector = `iframe[src^="${expected.toString().replace(/\/$/, '')}"]`
    await expect(page.locator(frameSelector).first()).toBeVisible({ timeout: 20_000 })
    const frame = page.frameLocator(frameSelector).first()
    await expect(frame.locator('body')).toContainText(new RegExp(EXPECTED_BODY_PATTERN, 'i'), { timeout: 30_000 })
    return
  }

  const popupPromise = context.waitForEvent('page', { timeout: 15_000 })
  await launchButton.click()
  const popup = await popupPromise
  await popup.waitForLoadState('domcontentloaded', { timeout: 20_000 })

  assertLaunchUrl(popup.url(), baseURL)
  await expect(popup.locator('body')).toContainText(new RegExp(EXPECTED_BODY_PATTERN, 'i'), { timeout: 20_000 })
})

function assertLaunchUrl(actual: string, baseURL: string | undefined) {
  if (EXPECTED_URL_PATTERN) {
    expect(actual).toMatch(new RegExp(EXPECTED_URL_PATTERN))
  } else {
    const expected = new URL(EXPECTED_URL!, baseURL)
    expect(actual).toBe(expected.toString())
  }
}
