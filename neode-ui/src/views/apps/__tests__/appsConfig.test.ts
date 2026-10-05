import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { APP_CATEGORY_MAP, canLaunch, filterEntriesForTab, hasFrontendUi, isServiceContainer, isServicePackage, isWebsitePackage, isAppReadyForLaunch, launchBlockedReason, resolveAppIcon, resolveAppTitle, useCategoriesWithApps, DEFAULT_APP_ICON } from '../appsConfig'

function makePkg(id: string, title: string, category: string): PackageDataEntry {
  return {
    state: PackageState.Running,
    manifest: {
      id,
      title,
      version: '1.0.0',
      description: { short: '', long: '' },
      'release-notes': '',
      license: '',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': '',
      'donation-url': null,
      category,
    } as unknown as PackageDataEntry['manifest'],
    'static-files': { license: '', instructions: '', icon: '' },
  }
}

describe('appsConfig service filtering', () => {
  it('keeps standalone Angor APIs in Services without a launch button', () => {
    for (const id of ['angor-indexer', 'angor-relay']) {
      const pkg = makePkg(id, id, 'money')
      expect(filterEntriesForTab([[id, pkg]], 'services', 'all')).toHaveLength(1)
      expect(filterEntriesForTab([[id, pkg]], 'apps', 'all')).toHaveLength(0)
      expect(canLaunch(pkg)).toBe(false)
    }
  })

  it('treats bitcoin stack UI sidecars as services', () => {
    expect(isServiceContainer('bitcoin-ui')).toBe(true)
    expect(isServiceContainer('lnd-ui')).toBe(true)
    expect(isServiceContainer('electrs-ui')).toBe(true)
  })

  it('treats container aliases as services even with non-service keys', () => {
    const aliasPkg = makePkg('bitcoin-ui', 'Bitcoin UI', 'money')
    expect(isServicePackage('core-lnd-ui', aliasPkg)).toBe(true)
  })

  it('removes service-only categories from app category tabs', () => {
    const packages = ref<Record<string, PackageDataEntry>>({
      'core-bitcoin-ui': makePkg('bitcoin-ui', 'Bitcoin UI', 'money'),
      'filebrowser': makePkg('filebrowser', 'File Browser', 'data'),
    })

    const allCategories = ref([
      { id: 'all', name: 'All' },
      { id: 'money', name: 'Money' },
      { id: 'data', name: 'Data' },
    ])

    const visible = useCategoriesWithApps(packages, allCategories)
    expect(visible.value.map(c => c.id)).toEqual(['all', 'data'])
  })

  it('filters apps tab by category using manifest-aware service checks', () => {
    const entries: Array<[string, PackageDataEntry]> = [
      ['core-bitcoin-ui', makePkg('bitcoin-ui', 'Bitcoin UI', 'money')],
      ['filebrowser', makePkg('filebrowser', 'File Browser', 'data')],
      ['btcpay-server', makePkg('btcpay-server', 'BTCPay', 'commerce')],
    ]

    const appsAll = filterEntriesForTab(entries, 'apps', 'all')
    expect(appsAll.map(([id]) => id)).toEqual(['filebrowser', 'btcpay-server'])

    const appsData = filterEntriesForTab(entries, 'apps', 'data')
    expect(appsData.map(([id]) => id)).toEqual(['filebrowser'])
  })

  it('routes service aliases into services tab and excludes user apps', () => {
    const entries: Array<[string, PackageDataEntry]> = [
      ['core-lnd-ui', makePkg('lnd-ui', 'LND UI', 'money')],
      ['grafana', makePkg('grafana', 'Grafana', 'data')],
    ]

    const services = filterEntriesForTab(entries, 'services', 'all')
    expect(services.map(([id]) => id)).toEqual(['core-lnd-ui'])
  })

  it('shows Cuprate as one My Apps entry while hiding its daemon dependency', () => {
    const entries: Array<[string, PackageDataEntry]> = [
      ['cuprate', makePkg('cuprate', 'Cuprate', 'money')],
      ['archy-cuprate-ui', makePkg('archy-cuprate-ui', 'Cuprate UI companion', 'money')],
      ['cuprate-ui', makePkg('cuprate-ui', 'Cuprate UI', 'money')],
    ]
    expect(filterEntriesForTab(entries, 'apps', 'all').map(([id]) => id)).toEqual(['cuprate'])
    expect(filterEntriesForTab(entries, 'services', 'all').map(([id]) => id)).toEqual([])
  })

  it('groups NetBird parts and BTCPay aliases without hiding a legacy-only installation', () => {
    const entries: Array<[string, PackageDataEntry]> = [
      ['netbird', makePkg('netbird', 'NetBird', 'networking')],
      ['netbird-dashboard', makePkg('netbird-dashboard', 'NetBird Dashboard', 'networking')],
      ['netbird-server', makePkg('netbird-server', 'NetBird Server', 'networking')],
      ['btcpay', makePkg('btcpay', 'BTCPay', 'money')],
      ['btcpay-server', makePkg('btcpay-server', 'BTCPay Server', 'commerce')],
    ]
    expect(filterEntriesForTab(entries, 'apps', 'all').map(([id]) => id)).toEqual(['netbird', 'btcpay-server'])
    expect(filterEntriesForTab(entries, 'services', 'all')).toEqual([])
    expect(filterEntriesForTab([entries[3]!], 'apps', 'all').map(([id]) => id)).toEqual(['btcpay'])
  })

  it('falls back to packaged app icon when static icon token is not a path', () => {
    const pkg = makePkg('gitea', 'Gitea', 'dev')
    pkg['static-files']!.icon = 'git-branch'
    expect(resolveAppIcon('gitea', pkg)).toBe('/assets/img/app-icons/gitea.svg')
  })

  it('an unmapped id gets the A mark, not a guessed png that 404s', () => {
    // strfry 404'd live on 2026-08-07: no curated entry, no fallback entry,
    // no service prefix — the old `${id}.png` guess produced a broken tile.
    const pkg = makePkg('strfry', 'strfry', 'nostr')
    expect(resolveAppIcon('strfry', pkg)).toBe(DEFAULT_APP_ICON)
  })

  it('classifies an unknown app by whether its manifest declares a UI (#45)', () => {
    // Headless: a LAN address but no declared UI → Website.
    const headless = makePkg('some-backend', 'Some Backend', 'other')
    headless.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:9000' } } } as unknown as PackageDataEntry['installed']
    expect(hasFrontendUi(headless)).toBe(false)
    expect(isWebsitePackage('some-backend', headless)).toBe(true)

    // Front-end app: declares interfaces.main.ui → My Apps even when not in the
    // curated category map.
    const uiApp = makePkg('some-ui-app', 'Some UI App', 'other')
    ;(uiApp.manifest as unknown as Record<string, unknown>).interfaces = { main: { ui: 'http://localhost:9001' } }
    uiApp.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:9001' } } } as unknown as PackageDataEntry['installed']
    expect(hasFrontendUi(uiApp)).toBe(true)
    expect(isWebsitePackage('some-ui-app', uiApp)).toBe(false)
  })

  it('keeps GitWorkshop in My Apps while installing metadata is minimal', () => {
    const installing = makePkg('archipelago-source', 'GitWorkshop', 'development')
    installing.state = PackageState.Installing
    expect(isWebsitePackage('archipelago-source', installing)).toBe(false)
    expect(filterEntriesForTab([['archipelago-source', installing]], 'apps', 'all'))
      .toEqual([['archipelago-source', installing]])
    expect(filterEntriesForTab([['archipelago-source', installing]], 'services', 'all'))
      .toEqual([])
  })

  it('keeps every curated app placeholder out of Services during install', () => {
    for (const id of Object.keys(APP_CATEGORY_MAP)) {
      if (isServiceContainer(id)) continue
      const placeholder = makePkg(id, id, APP_CATEGORY_MAP[id] ?? 'other')
      placeholder.state = PackageState.Installing
      expect(isWebsitePackage(id, placeholder), id).toBe(false)
    }
  })

  it('never offers Launch for an unknown container with a bare exposed port', () => {
    // A self-deployed compose stack (e.g. podsteadr) publishes a port, so it
    // has a runtime lan-address — but no manifest-declared or probed UI. It
    // must classify as a service and must NOT get a Launch button.
    const selfDeployed = makePkg('podsteadr', 'podsteadr', 'other')
    selfDeployed.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:8095' } }, status: 'running' } as unknown as PackageDataEntry['installed']
    expect(isWebsitePackage('podsteadr', selfDeployed)).toBe(true)
    expect(canLaunch(selfDeployed)).toBe(false)
  })

  it('offers Launch for an unknown container once the backend confirms a UI', () => {
    const confirmedUi = makePkg('podsteadr', 'podsteadr', 'other')
    ;(confirmedUi.manifest as unknown as Record<string, unknown>).interfaces = { main: { ui: 'true' } }
    confirmedUi.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:8095' } }, status: 'running' } as unknown as PackageDataEntry['installed']
    expect(canLaunch(confirmedUi)).toBe(true)
  })

  it('does not launch a health-checked app during the running-before-ready race', () => {
    const pkg = makePkg('archipelago-source', 'GitWorkshop', 'development')
    ;(pkg.manifest as unknown as Record<string, unknown>).interfaces = { main: { ui: 'true' } }
    ;(pkg.manifest as unknown as Record<string, unknown>).health_check = { path: '/healthz' }
    pkg.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:8337' } }, status: 'running' } as unknown as PackageDataEntry['installed']
    pkg.health = null
    expect(isAppReadyForLaunch(pkg)).toBe(false)
    expect(canLaunch(pkg)).toBe(false)
    expect(launchBlockedReason(pkg.manifest.id, pkg)).toBe('Web UI not ready: GitWorkshop')
    pkg.health = 'healthy'
    expect(canLaunch(pkg)).toBe(true)
  })

  it('never offers Launch for curated service containers even with a UI flag', () => {
    const service = makePkg('indeedhub-api', 'IndeeHub API', 'media')
    ;(service.manifest as unknown as Record<string, unknown>).interfaces = { main: { ui: 'true' } }
    service.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:9100' } }, status: 'running' } as unknown as PackageDataEntry['installed']
    expect(canLaunch(service)).toBe(false)
  })

  it('keeps Launch for curated apps that rely on a runtime address alone', () => {
    const known = makePkg('jellyfin', 'Jellyfin', 'media')
    known.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:8096' } }, status: 'running' } as unknown as PackageDataEntry['installed']
    expect(canLaunch(known)).toBe(true)
  })

  it('explains that Fedimint waits for Bitcoin sync before Guardian starts', () => {
    const pkg = makePkg('fedimint', 'Fedimint', 'money')
    pkg.state = PackageState.Starting
    pkg.installed = { 'interface-addresses': { main: { 'lan-address': 'http://localhost:8175' } } } as unknown as PackageDataEntry['installed']
    expect(launchBlockedReason('fedimint', pkg)).toContain('Bitcoin')
    expect(canLaunch(pkg)).toBe(true)
  })
})

describe('headless service readiness messages', () => {
  it('does not treat an API-only service as waiting for its own nonexistent UI', () => {
    for (const id of ['phoenixd', 'angor-indexer', 'angor-relay', 'custom-api']) {
      const pkg = makePkg(id, id, 'other')
      pkg['ui-ready'] = false
      pkg.health = 'healthy'
      expect(canLaunch(pkg)).toBe(false)
      expect(launchBlockedReason(id, pkg)).toBe('')
    }
  })
})

describe('HTTP readiness independent of container health', () => {
  it('blocks fixed launch URLs while the HTTP upstream is unavailable', () => {
    for (const id of ['gitea', 'filebrowser', 'fedimint', 'lnd']) {
      const pkg = makePkg(id, id, 'other')
      pkg['ui-ready'] = false
      pkg.health = 'healthy'
      expect(canLaunch(pkg)).toBe(false)
      expect(isAppReadyForLaunch(pkg)).toBe(false)
      expect(launchBlockedReason(id, pkg)).toBe(`Web UI not ready: ${resolveAppTitle(id, pkg)}`)
      pkg['ui-ready'] = true
      expect(isAppReadyForLaunch(pkg)).toBe(true)
    }
  })
  it('allows a ready companion while its backend is syncing', () => {
    const pkg = makePkg('lnd', 'Lightning', 'bitcoin')
    pkg.health = 'starting'
    pkg['ui-ready'] = true
    expect(isAppReadyForLaunch(pkg)).toBe(true)
  })
})

describe('manifest-generated service names', () => {
  it('gives a new headless service its declared name without a hardcoded curated entry', () => {
    const pkg = makePkg('angor-relay', 'angor-relay', 'money')
    expect(resolveAppTitle('angor-relay', pkg)).toBe('Angor Relay')
    pkg.manifest.title = 'My project relay'
    expect(resolveAppTitle('angor-relay', pkg)).toBe('My project relay')
    expect(resolveAppTitle('unknown-service', makePkg('unknown-service', 'unknown-service', ''))).toBe('unknown-service')
  })
})

it('does not display a stale Mempool frontend alias beside its live package', () => {
  const main = makePkg('mempool', 'Mempool', 'money')
  const alias = makePkg('mempool-web', 'Mempool', 'money')
  alias.state = PackageState.Restarting
  const entries: Array<[string, PackageDataEntry]> = [['mempool', main], ['mempool-web', alias]]
  const shown = [...filterEntriesForTab(entries, 'apps', 'all'), ...filterEntriesForTab(entries, 'services', 'all')]
  expect(shown.map(([id]) => id)).toEqual(['mempool'])
  expect(filterEntriesForTab([['mempool-web', alias]], 'apps', 'all').map(([id]) => id)).toEqual(['mempool-web'])
})

it('shows Immich as one app without internal database/cache cards in either tab', () => {
  const entries: [string, PackageDataEntry][] = ['immich', 'immich-postgres', 'immich-redis', 'immich_postgres', 'immich_redis'].map(id => [id, makePkg(id, id, 'media')])
  expect(filterEntriesForTab(entries, 'apps', 'all').map(([id]) => id)).toEqual(['immich'])
  expect(filterEntriesForTab(entries, 'services', 'all')).toEqual([])
})
