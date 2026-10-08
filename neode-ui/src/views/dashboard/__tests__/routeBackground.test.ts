import { describe, expect, it } from 'vitest'
import { backgroundForRoute } from '../useRouteTransitions'

describe('dashboard route backgrounds', () => {
  it('keeps the Network tab backdrop while opening firewall settings and returning', () => {
    const routes = ['/dashboard/server', '/dashboard/server/firewall-tunnels', '/dashboard/server']
    expect(routes.map(backgroundForRoute)).toEqual(['bg-network.jpg', 'bg-network.jpg', 'bg-network.jpg'])
  })
  it('inherits other Network detail screens without overriding the explicit Web5 federation backdrop', () => {
    expect(backgroundForRoute('/dashboard/server/openwrt')).toBe('bg-network.jpg')
    expect(backgroundForRoute('/dashboard/server/federation')).toBe('bg-web5.jpg')
  })
  it('retains other parent sections and app-detail behavior', () => {
    expect(backgroundForRoute('/dashboard/cloud/folder')).toBe('bg-cloud.webp')
    expect(backgroundForRoute('/dashboard/settings/account')).toBe('bg-settings.webp')
    expect(backgroundForRoute('/dashboard/web5/credentials')).toBe('bg-web5.jpg')
    expect(backgroundForRoute('/dashboard/apps/bitcoin')).toBe('bg-intro.jpg')
    expect(backgroundForRoute('/dashboard')).toBe('bg-home.webp')
  })
})
