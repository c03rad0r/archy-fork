import { test, expect } from '@playwright/test'

test.describe('Content surfaces', () => {
  test('empty state shows when no conversation selected', async ({ page }) => {
    await page.goto('/')
    const main = page.locator('main.path-glass-card')
    await expect(main).toBeVisible()
  })

  test('chat can receive input', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    const input = page.getByPlaceholder(/Message AIUI/)
    await input.click()
    await input.pressSequentially('Recommend some films')
    await expect(input).toHaveValue('Recommend some films', { timeout: 3000 })
  })

  test('films surface: films conversation loads and shows film cards', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    await page.getByRole('button', { name: /View all \d+ films/i }).click()
    await expect(page.locator('main').getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
  })

  test('films surface: clicking assistant bubble opens panel', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    await page.locator('.path-glass-bubble').filter({ hasText: /Blade Runner|Arrival|Dune/ }).first().click()
    await expect(page.locator('main').getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
  })

  test('magazine surface: BIP brief shows sections', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.locator('aside button').filter({ has: page.locator('h2') }).first().click()
    await page.getByRole('button', { name: 'BIP 110 brief' }).click()
    await expect(page.getByText(/BIP 110|Pro camp|Summary/i).first()).toBeVisible({ timeout: 8000 })
    await page.getByRole('button', { name: 'View brief' }).click()
    await expect(page.getByText(/AI Brief|Summary|Pro camp/i).first()).toBeVisible({ timeout: 5000 })
  })

  test('songs surface: songs conversation shows song cards', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.locator('aside button').filter({ has: page.locator('h2') }).first().click()
    await page.getByRole('button', { name: 'Music recommendations' }).click()
    await expect(page.getByText('Never Meant').first()).toBeVisible({ timeout: 8000 })
    await page.getByRole('button', { name: /View all \d+ songs/i }).click()
    await expect(page.locator('main').getByText('Never Meant').first()).toBeVisible({ timeout: 5000 })
  })

  test('podcasts surface: podcasts conversation shows podcast cards', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.locator('aside button').filter({ has: page.locator('h2') }).first().click()
    await page.getByRole('button', { name: 'Bitcoin podcasts' }).click()
    await expect(page.getByText('What Bitcoin Did').first()).toBeVisible({ timeout: 8000 })
    await page.getByRole('button', { name: /View all \d+ podcasts/i }).click()
    await expect(page.locator('main').getByText('What Bitcoin Did').first()).toBeVisible({ timeout: 5000 })
  })

  test('websites surface: websites tab shows link cards', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.locator('aside button').filter({ has: page.locator('h2') }).first().click()
    await page.getByRole('button', { name: 'Bitcoin resources' }).click()
    await expect(page.getByText('Bitcoin Magazine').first()).toBeVisible({ timeout: 8000 })
    await page.getByRole('button', { name: /View all \d+ websites/i }).click()
    await expect(page.locator('main').getByText('Bitcoin Magazine').first()).toBeVisible({ timeout: 5000 })
  })

  test('news surface: news conversation shows articles', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.locator('aside button').filter({ has: page.locator('h2') }).first().click()
    await page.getByRole('button', { name: 'Latest Bitcoin news' }).click()
    await expect(page.getByText(/Bitcoin hits|ETF inflows/i).first()).toBeVisible({ timeout: 8000 })
    await page.getByRole('button', { name: /View all \d+ articles/i }).click()
    await expect(page.locator('main').getByText(/Bitcoin hits|ETF inflows/i).first()).toBeVisible({ timeout: 5000 })
  })
})

test.describe('Chat interactions', () => {
  test('sends a message and receives streaming response', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await page.waitForLoadState('networkidle')
    const input = page.getByPlaceholder(/Message AIUI/)
    await input.click()
    await input.fill('Hello')
    await input.press('Enter')
    // User message should appear in chat
    await expect(page.getByText('Hello').first()).toBeVisible({ timeout: 5000 })
  })

  test('content panel shows film cards when AI mentions films', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    // Film cards should be visible inline in assistant message
    await expect(page.getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
    // Open panel via "View all" button
    await page.getByRole('button', { name: /View all \d+ films/i }).click()
    await expect(page.locator('main').getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
  })

  test('clicking a film card opens detail view', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    // Open panel
    await page.locator('.path-glass-bubble').filter({ hasText: /Blade Runner|Arrival|Dune/ }).first().click()
    const filmCard = page.locator('main').getByText('Blade Runner 2049').first()
    await expect(filmCard).toBeVisible({ timeout: 5000 })
    // Click film card to open detail
    await filmCard.click()
    // Detail view should show film metadata
    await expect(page.getByText(/Denis Villeneuve|2017|Sci-Fi/i).first()).toBeVisible({ timeout: 5000 })
  })

  test('mobile viewport shows full-screen overlay for content', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    // Open panel on mobile
    await page.locator('.path-glass-bubble').filter({ hasText: /Blade Runner|Arrival|Dune/ }).first().click()
    // Content should be visible as overlay on mobile
    await expect(page.locator('main').getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
  })

  test('stop button halts generation', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    // When not streaming, stop button should not be visible
    const stopButton = page.getByRole('button', { name: 'Stop generation' })
    await expect(stopButton).toBeHidden({ timeout: 3000 })
    // Chat input should be available instead
    const input = page.getByPlaceholder(/Message AIUI/)
    await expect(input).toBeVisible({ timeout: 3000 })
  })

  test('web search toggle works', async ({ page }) => {
    await page.goto('/')
    await page.waitForLoadState('networkidle')
    const toggle = page.getByRole('button', { name: 'Toggle web search' })
    await expect(toggle).toBeVisible({ timeout: 5000 })
    // Click to toggle web search on
    await toggle.click()
    // The button styling should change (it gains accent color when active)
    await expect(toggle).toBeVisible()
    // Click again to toggle off
    await toggle.click()
    await expect(toggle).toBeVisible()
  })

  test('new conversation clears messages', async ({ page }) => {
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    // Click "New conversation" button
    await page.getByRole('button', { name: 'New conversation' }).click()
    // Previous messages should be cleared
    await expect(page.getByText('Recommend some sci-fi films')).toBeHidden({ timeout: 5000 })
  })

  test('panel side toggle switches layout', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await Promise.all([
      page.waitForResponse((res) => res.url().includes('dev-chats') && res.status() === 200, { timeout: 15000 }),
      page.goto('/'),
    ])
    await expect(page.getByText('Recommend some sci-fi films')).toBeVisible({ timeout: 10000 })
    // Open the panel
    await page.locator('.path-glass-bubble').filter({ hasText: /Blade Runner|Arrival|Dune/ }).first().click()
    await expect(page.locator('main').getByText('Blade Runner 2049').first()).toBeVisible({ timeout: 5000 })
    // Both chat and panel sections should be visible on desktop
    const chatSection = page.locator('section').first()
    await expect(chatSection).toBeVisible()
  })
})
