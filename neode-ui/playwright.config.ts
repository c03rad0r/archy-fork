import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  outputDir: './e2e/test-results',
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  use: {
    baseURL: process.env.ARCHY_BASE_URL ?? 'http://localhost:8100',
    viewport: { width: 1440, height: 900 },
    screenshot: 'only-on-failure',
    trace: 'off',
    ignoreHTTPSErrors: true,
    launchOptions: process.env.ARCHY_CHROMIUM_EXECUTABLE
      ? { executablePath: process.env.ARCHY_CHROMIUM_EXECUTABLE }
      : undefined,
  },
  projects: [
    {
      name: 'chromium',
      use: { browserName: 'chromium' },
    },
  ],
})
