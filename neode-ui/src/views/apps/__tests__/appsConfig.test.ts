import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { canLaunch, filterEntriesForTab, hasFrontendUi, isServiceContainer, isServicePackage, isWebsitePackage, launchBlockedReason, resolveAppIcon, useCategoriesWithApps, DEFAULT_APP_ICON } from '../appsConfig'

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
