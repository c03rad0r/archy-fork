import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import path from 'path'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src')
    }
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./vitest.setup.ts'],
    root: '.',
    passWithNoTests: true,
    exclude: ['e2e/**', 'node_modules/**', '**/._*'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'text-summary'],
      include: [
        'src/api/*.ts',
        'src/stores/*.ts',
        'src/composables/*.ts',
        'src/utils/*.ts',
        'src/services/*.ts',
        'src/router/*.ts',
      ],
      exclude: ['src/**/__tests__/**', 'src/**/*.d.ts', 'src/main.ts'],
      thresholds: {
        branches: 80,
      },
    },
  }
})
