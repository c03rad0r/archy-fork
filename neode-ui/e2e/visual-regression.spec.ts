import { test, type Page } from '@playwright/test'

const SCREENSHOT_DIR = './e2e/screenshots'
const PASSWORD = 'password123'

/** Set localStorage values to skip splash screen and onboarding */
async function skipSplashAndOnboarding(page: Page) {
  await page.goto('/login')
  await page.evaluate(() => {
    localStorage.setItem('neode_intro_seen', '1')
    localStorage.setItem('neode_onboarding_complete', '1')
  })
}

async function login(page: Page) {
  await skipSplashAndOnboarding(page)
  await page.goto('/login')
  await page.waitForLoadState('networkidle')

  // Wait for the password input to appear (server health check may delay it)
  const passwordInput = page.locator('input[type="password"]').first()
  await passwordInput.waitFor({ timeout: 15_000 })
  await passwordInput.fill(PASSWORD)

  // Click the login/submit button
  const submitBtn = page
    .locator(
      'button:has-text("Login"), button:has-text("Unlock"), button:has-text("Continue"), button[type="submit"]',
    )
    .first()
  await submitBtn.click()

  // Wait for navigation to dashboard
  await page.waitForURL('**/dashboard**', { timeout: 15_000 })
  await page.waitForLoadState('networkidle')
  // Wait for home page content to confirm dashboard is loaded
  await page.locator('text=Welcome Noderunner').waitFor({ timeout: 10_000 })
  await page.waitForTimeout(1500)
}

/** Navigate to a dashboard child route via sidebar link click */
async function navigateTo(page: Page, path: string, waitForText: string) {
  // Use in-page navigation to avoid full SPA reload
  await page.evaluate((p) => {
    window.history.pushState({}, '', p)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }, path)
  // Wait for the page-specific content to appear
  await page.locator(`text=${waitForText}`).first().waitFor({ timeout: 10_000 })
  // Let content settle after route change
  await page.waitForTimeout(800)
}

async function screenshot(page: Page, name: string) {
  // Wait for any animations to settle
  await page.waitForTimeout(1000)
  await page.screenshot({
    path: `${SCREENSHOT_DIR}/${name}.png`,
    fullPage: true,
  })
}

test.describe('Visual Regression — Public Pages', () => {
  test('login page', async ({ page }) => {
    await skipSplashAndOnboarding(page)
    await page.goto('/login')
    await page.waitForLoadState('networkidle')
    // Wait for server health check and form to become active
    await page.locator('input[type="password"]').first().waitFor({ timeout: 15_000 })
    await page.waitForTimeout(1000)
    await screenshot(page, '01-login')
  })
})

test.describe('Visual Regression — Dashboard Pages', () => {
  test.beforeEach(async ({ page }) => {
    await login(page)
  })

  test('home / dashboard', async ({ page }) => {
    // Already on home after login
    await screenshot(page, '02-dashboard-home')
  })

  test('apps list', async ({ page }) => {
    await navigateTo(page, '/dashboard/apps', 'My Apps')
    await screenshot(page, '03-apps-list')
  })

  test('marketplace', async ({ page }) => {
    await navigateTo(page, '/dashboard/marketplace', 'App Store')
    await screenshot(page, '04-marketplace')
  })

  test('cloud storage', async ({ page }) => {
    await navigateTo(page, '/dashboard/cloud', 'Cloud')
    await screenshot(page, '05-cloud')
  })

  test('server', async ({ page }) => {
    await navigateTo(page, '/dashboard/server', 'Network')
    await screenshot(page, '06-server')
  })

  test('web5', async ({ page }) => {
    await navigateTo(page, '/dashboard/web5', 'Web5')
    await screenshot(page, '07-web5')
  })

  test('settings', async ({ page }) => {
    await navigateTo(page, '/dashboard/settings', 'Settings')
    await screenshot(page, '08-settings')
  })

  test('chat', async ({ page }) => {
    await navigateTo(page, '/dashboard/chat', 'AI Assistant')
    await screenshot(page, '09-chat')
  })

  test('federation', async ({ page }) => {
    await navigateTo(page, '/dashboard/server/federation', 'Federation')
    await screenshot(page, '10-federation')
  })

  test('credentials', async ({ page }) => {
    await navigateTo(page, '/dashboard/web5/credentials', 'Credentials')
    await screenshot(page, '11-credentials')
  })

  test('system update', async ({ page }) => {
    await navigateTo(page, '/dashboard/settings/update', 'System Update')
    await screenshot(page, '12-system-update')
  })
})
