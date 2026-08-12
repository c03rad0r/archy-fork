import { test, expect } from '@playwright/test'

test.describe('Visual Regression', () => {
  test('ChatPage renders correctly', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    await expect(page).toHaveScreenshot('chat-page.png', {
      maxDiffPixelRatio: 0.005,
      fullPage: true,
    })
  })

  test('ContentPanel renders correctly', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    // Open content panel by clicking a content tab
    const filmTab = page.getByRole('button', { name: /film/i }).first()
    if (await filmTab.isVisible()) {
      await filmTab.click()
      await page.waitForTimeout(500)
      await expect(page).toHaveScreenshot('content-panel-films.png', {
        maxDiffPixelRatio: 0.005,
      })
    }
  })

  test('PassphraseDialog renders correctly', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    // PassphraseDialog shows on first load if crypto is enabled
    const dialog = page.locator('.glass-card').filter({ hasText: 'Unlock AIUI' })
    if (await dialog.isVisible()) {
      await expect(dialog).toHaveScreenshot('passphrase-dialog.png', {
        maxDiffPixelRatio: 0.005,
      })
    }
  })

  test('BottomSheet renders correctly on mobile', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    await expect(page).toHaveScreenshot('mobile-view.png', {
      maxDiffPixelRatio: 0.005,
      fullPage: true,
    })
  })
})
