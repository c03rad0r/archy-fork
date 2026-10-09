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
    // Vitest 5's process-fork pool can time out while starting workers on a
    // busy two-core development node. Threads retain per-file isolation and
    // avoid that expensive process startup; CI itself remains containerized.
    pool: 'threads',
    maxWorkers: 4,
    // Vitest's 5s default is not a statement about these tests — the whole
    // 1000-test suite runs in ~70s on an idle box. It is a statement about
    // the machine. This one also runs a live node, so a release gate can
    // collide with a cargo build or a container churn, and starved workers
    // blow 5s on tests that normally take milliseconds: on 2026-08-20 four
    // unrelated tests timed out at once (one after 36s of wall clock) purely
    // from CPU contention, failing the gate with nothing actually broken.
    // 20s keeps real hangs bounded while surviving a busy box.
    testTimeout: 20_000,
    hookTimeout: 20_000,
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
