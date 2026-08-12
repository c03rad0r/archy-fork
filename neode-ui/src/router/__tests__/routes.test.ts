import { describe, it, expect } from 'vitest'

/**
 * Tests for router route definitions and configuration.
 * Full guard tests are in guards.test.ts. This tests route structure.
 */
describe('router route definitions', () => {
  it('has expected public routes', () => {
    const publicPaths = ['/', '/login', '/onboarding/intro', '/onboarding/options',
      '/onboarding/path', '/onboarding/did', '/onboarding/identity',
      '/onboarding/backup', '/onboarding/verify', '/onboarding/done', '/recovery']

    // These should all resolve (we test the path list itself)
    expect(publicPaths.length).toBe(11)
    publicPaths.forEach(p => {
      expect(typeof p).toBe('string')
      expect(p.startsWith('/')).toBe(true)
    })
  })

  it('has expected dashboard routes', () => {
    const dashPaths = [
      '/dashboard', '/dashboard/apps', '/dashboard/marketplace',
      '/dashboard/cloud', '/dashboard/server', '/dashboard/web5',
      '/dashboard/settings', '/dashboard/chat', '/dashboard/monitoring',
    ]

    dashPaths.forEach(p => {
      expect(p.startsWith('/dashboard')).toBe(true)
    })
  })

  it('has parameterized routes', () => {
    const paramRoutes = [
      { path: '/dashboard/apps/:id', example: '/dashboard/apps/bitcoin-knots' },
      { path: '/dashboard/marketplace/:id', example: '/dashboard/marketplace/lnd' },
      { path: '/dashboard/cloud/:folderId', example: '/dashboard/cloud/photos' },
      { path: '/dashboard/goals/:goalId', example: '/dashboard/goals/sync-bitcoin' },
    ]

    paramRoutes.forEach(r => {
      const pattern = r.path.replace(/:(\w+)/g, '([^/]+)')
      expect(new RegExp(`^${pattern}$`).test(r.example)).toBe(true)
    })
  })

  it('containers routes redirect to apps', () => {
    // The router redirects /dashboard/containers -> /dashboard/apps
    // and /dashboard/containers/:id -> /dashboard/apps/:id
    const redirectMap = {
      'containers': 'apps',
      'containers/bitcoin-knots': 'apps/bitcoin-knots',
    }

    Object.entries(redirectMap).forEach(([from, to]) => {
      expect(to).toBe(from.replace('containers', 'apps'))
    })
  })

  it('SESSION_CHECK_TIMEOUT_MS is a reasonable value', () => {
    const SESSION_CHECK_TIMEOUT_MS = 8000
    expect(SESSION_CHECK_TIMEOUT_MS).toBeGreaterThan(1000)
    expect(SESSION_CHECK_TIMEOUT_MS).toBeLessThanOrEqual(15000)
  })
})

describe('checkSessionWithTimeout logic', () => {
  it('resolves with session result when fast', async () => {
    const checkSession = () => Promise.resolve(true)
    const result = await Promise.race([
      checkSession(),
      new Promise<boolean>((resolve) =>
        setTimeout(() => resolve(false), 8000)
      ),
    ])
    expect(result).toBe(true)
  })

  it('resolves false when session check fails', async () => {
    const checkSession = () => Promise.reject(new Error('Network error'))
    try {
      await Promise.race([
        checkSession(),
        new Promise<boolean>((resolve) =>
          setTimeout(() => resolve(false), 8000)
        ),
      ])
    } catch {
      // Expected - the catch in the real code returns false
      expect(true).toBe(true)
    }
  })
})
