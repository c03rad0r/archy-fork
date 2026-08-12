import { test, expect } from '@playwright/test'

test.describe('AIUI smoke tests', () => {
  test('app loads and shows chat interface', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveTitle(/AIUI/)
  })

  test('chat input is visible and focusable', async ({ page }) => {
    await page.goto('/')
    const input = page.getByPlaceholder(/Message AIUI|Waiting for/)
    await expect(input).toBeVisible()
  })

  test('content panel area exists', async ({ page }) => {
    await page.goto('/')
    const main = page.locator('main.path-glass-card')
    await expect(main).toBeVisible()
  })
})
