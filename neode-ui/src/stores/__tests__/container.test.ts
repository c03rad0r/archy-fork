import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('@/api/container-client', () => ({
  containerClient: {
    listContainers: vi.fn(),
    getHealthStatus: vi.fn(),
    installApp: vi.fn(),
    startContainer: vi.fn(),
    stopContainer: vi.fn(),
    removeContainer: vi.fn(),
    getContainerLogs: vi.fn(),
    getContainerStatus: vi.fn(),
    startBundledApp: vi.fn(),
    stopBundledApp: vi.fn(),
  },
}))

import { useContainerStore } from '../container'
import { containerClient } from '@/api/container-client'

const mockedClient = vi.mocked(containerClient)

const mockContainers = [
  { id: '1', name: 'bitcoin-knots', state: 'running' as const, status: 'Up 2 hours', image: 'bitcoinknots:29', created: '2026-01-01', ports: ['8332'], lan_address: 'http://localhost:8332' },
  { id: '2', name: 'lnd', state: 'stopped' as const, status: 'Exited (0)', image: 'lnd:v0.18.4', created: '2026-01-01', ports: ['9735'], lan_address: undefined },
  { id: '3', name: 'mempool', state: 'running' as const, status: 'Up 1 hour', image: 'mempool:latest', created: '2026-01-01', ports: ['8080'], lan_address: 'http://localhost:8080' },
]

describe('useContainerStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('fetchContainers loads container list', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.containers).toEqual(mockContainers)
    expect(store.loading).toBe(false)
    expect(store.error).toBeNull()
  })

  it('fetchContainers sets error on failure', async () => {
    mockedClient.listContainers.mockRejectedValue(new Error('Connection refused'))
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.error).toBe('Connection refused')
    expect(store.loading).toBe(false)
  })

  it('runningContainers filters correctly', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.runningContainers).toHaveLength(2)
    expect(store.runningContainers.map(c => c.name)).toEqual(['bitcoin-knots', 'mempool'])
  })

  it('stoppedContainers filters correctly', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.stoppedContainers).toHaveLength(1)
    expect(store.stoppedContainers[0]!.name).toBe('lnd')
  })

  it('startContainer calls client and refreshes', async () => {
    mockedClient.startContainer.mockResolvedValue(undefined)
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    mockedClient.getHealthStatus.mockResolvedValue({ 'bitcoin-knots': 'healthy' })
    const store = useContainerStore()

    await store.startContainer('bitcoin-knots')

    expect(mockedClient.startContainer).toHaveBeenCalledWith('bitcoin-knots')
    expect(mockedClient.listContainers).toHaveBeenCalled()
    expect(mockedClient.getHealthStatus).toHaveBeenCalled()
    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('stopContainer calls client and refreshes', async () => {
    mockedClient.stopContainer.mockResolvedValue(undefined)
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    await store.stopContainer('lnd')

    expect(mockedClient.stopContainer).toHaveBeenCalledWith('lnd')
    expect(mockedClient.listContainers).toHaveBeenCalled()
    expect(store.isAppLoading('lnd')).toBe(false)
  })

  it('getAppState returns correct states', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.getAppState('bitcoin-knots')).toBe('running')
    expect(store.getAppState('lnd')).toBe('stopped')
    expect(store.getAppState('nonexistent')).toBe('not-installed')
  })

  it('isAppLoading tracks per-app loading state', async () => {
    let resolveStart: (() => void) | undefined
    mockedClient.startContainer.mockImplementation(() => new Promise(r => { resolveStart = r }))
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    mockedClient.getHealthStatus.mockResolvedValue({})
    const store = useContainerStore()

    const startPromise = store.startContainer('bitcoin-knots')

    expect(store.isAppLoading('bitcoin-knots')).toBe(true)
    expect(store.isAppLoading('lnd')).toBe(false)

    resolveStart!()
    await startPromise

    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('fetchHealthStatus loads health data', async () => {
    mockedClient.getHealthStatus.mockResolvedValue({ 'bitcoin-knots': 'healthy', 'lnd': 'degraded' })
    const store = useContainerStore()

    await store.fetchHealthStatus()

    expect(store.getHealthStatus('bitcoin-knots')).toBe('healthy')
    expect(store.getHealthStatus('lnd')).toBe('degraded')
    expect(store.getHealthStatus('unknown-app')).toBe('unknown')
  })

  it('fetchHealthStatus handles errors silently', async () => {
    mockedClient.getHealthStatus.mockRejectedValue(new Error('fail'))
    const store = useContainerStore()

    await store.fetchHealthStatus()
    // Should not throw, healthStatus stays empty
    expect(store.healthStatus).toEqual({})
  })

  it('installApp installs and refreshes containers', async () => {
    mockedClient.installApp.mockResolvedValue('new-app')
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()

    const result = await store.installApp('/path/to/manifest')

    expect(result).toBe('new-app')
    expect(mockedClient.installApp).toHaveBeenCalledWith('/path/to/manifest')
    expect(mockedClient.listContainers).toHaveBeenCalled()
    expect(store.loading).toBe(false)
  })

  it('installApp sets error and rethrows on failure', async () => {
    mockedClient.installApp.mockRejectedValue(new Error('Install failed'))
    const store = useContainerStore()

    await expect(store.installApp('/bad/manifest')).rejects.toThrow('Install failed')
    expect(store.error).toBe('Install failed')
    expect(store.loading).toBe(false)
  })

  it('startContainer sets error on failure', async () => {
    mockedClient.startContainer.mockRejectedValue(new Error('Start failed'))
    const store = useContainerStore()

    await expect(store.startContainer('bitcoin-knots')).rejects.toThrow('Start failed')
    expect(store.error).toBe('Start failed')
    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('stopContainer sets error on failure', async () => {
    mockedClient.stopContainer.mockRejectedValue(new Error('Stop failed'))
    const store = useContainerStore()

    await expect(store.stopContainer('lnd')).rejects.toThrow('Stop failed')
    expect(store.error).toBe('Stop failed')
    expect(store.isAppLoading('lnd')).toBe(false)
  })

  it('removeContainer removes and refreshes', async () => {
    mockedClient.removeContainer.mockResolvedValue(undefined)
    mockedClient.listContainers.mockResolvedValue([])
    const store = useContainerStore()

    await store.removeContainer('old-app')

    expect(mockedClient.removeContainer).toHaveBeenCalledWith('old-app')
    expect(mockedClient.listContainers).toHaveBeenCalled()
    expect(store.loading).toBe(false)
  })

  it('removeContainer sets error on failure', async () => {
    mockedClient.removeContainer.mockRejectedValue(new Error('Remove failed'))
    const store = useContainerStore()

    await expect(store.removeContainer('old-app')).rejects.toThrow('Remove failed')
    expect(store.error).toBe('Remove failed')
  })

  it('getContainerLogs returns logs', async () => {
    mockedClient.getContainerLogs.mockResolvedValue(['line1', 'line2', 'line3'])
    const store = useContainerStore()

    const logs = await store.getContainerLogs('bitcoin-knots', 50)

    expect(logs).toEqual(['line1', 'line2', 'line3'])
    expect(mockedClient.getContainerLogs).toHaveBeenCalledWith('bitcoin-knots', 50)
  })

  it('getContainerLogs defaults to 100 lines', async () => {
    mockedClient.getContainerLogs.mockResolvedValue(['log output'])
    const store = useContainerStore()

    await store.getContainerLogs('bitcoin-knots')

    expect(mockedClient.getContainerLogs).toHaveBeenCalledWith('bitcoin-knots', 100)
  })

  it('getContainerLogs sets error on failure', async () => {
    mockedClient.getContainerLogs.mockRejectedValue(new Error('Log error'))
    const store = useContainerStore()

    await expect(store.getContainerLogs('bitcoin-knots')).rejects.toThrow('Log error')
    expect(store.error).toBe('Log error')
  })

  it('getContainerStatus returns status', async () => {
    const status = { name: 'bitcoin-knots', state: 'running', uptime: '5h' }
    mockedClient.getContainerStatus.mockResolvedValue(status as never)
    const store = useContainerStore()

    const result = await store.getContainerStatus('bitcoin-knots')

    expect(result).toEqual(status)
  })

  it('getContainerStatus sets error on failure', async () => {
    mockedClient.getContainerStatus.mockRejectedValue(new Error('Status error'))
    const store = useContainerStore()

    await expect(store.getContainerStatus('bitcoin-knots')).rejects.toThrow('Status error')
    expect(store.error).toBe('Status error')
  })

  it('startBundledApp starts and refreshes', async () => {
    mockedClient.startBundledApp.mockResolvedValue(undefined)
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    mockedClient.getHealthStatus.mockResolvedValue({})
    const store = useContainerStore()

    const app = { id: 'bitcoin-knots', name: 'Bitcoin Knots', image: 'btc:29', description: '', icon: '', ports: [], volumes: [], category: 'bitcoin' as const }
    await store.startBundledApp(app)

    expect(mockedClient.startBundledApp).toHaveBeenCalledWith(app)
    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('startBundledApp sets error on failure', async () => {
    mockedClient.startBundledApp.mockRejectedValue(new Error('Start failed'))
    const store = useContainerStore()

    const app = { id: 'test', name: 'Test', image: 'test:1', description: '', icon: '', ports: [], volumes: [], category: 'other' as const }
    await expect(store.startBundledApp(app)).rejects.toThrow('Start failed')
    expect(store.error).toBe('Start failed')
    expect(store.isAppLoading('test')).toBe(false)
  })

  it('stopBundledApp stops and refreshes', async () => {
    mockedClient.stopBundledApp.mockResolvedValue(undefined)
    mockedClient.listContainers.mockResolvedValue([])
    const store = useContainerStore()

    await store.stopBundledApp('bitcoin-knots')

    expect(mockedClient.stopBundledApp).toHaveBeenCalledWith('bitcoin-knots')
    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('stopBundledApp sets error on failure', async () => {
    mockedClient.stopBundledApp.mockRejectedValue(new Error('Stop failed'))
    const store = useContainerStore()

    await expect(store.stopBundledApp('bitcoin-knots')).rejects.toThrow('Stop failed')
    expect(store.error).toBe('Stop failed')
    expect(store.isAppLoading('bitcoin-knots')).toBe(false)
  })

  it('getContainerById finds by name substring', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()
    await store.fetchContainers()

    expect(store.getContainerById('bitcoin')?.name).toBe('bitcoin-knots')
    expect(store.getContainerById('nonexistent')).toBeUndefined()
  })

  it('getContainerForApp matches by exact name', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()
    await store.fetchContainers()

    expect(store.getContainerForApp('bitcoin-knots')?.name).toBe('bitcoin-knots')
    expect(store.getContainerForApp('lnd')?.name).toBe('lnd')
  })

  it('enrichedBundledApps includes lan_address from matching containers', async () => {
    mockedClient.listContainers.mockResolvedValue(mockContainers)
    const store = useContainerStore()
    await store.fetchContainers()

    const enriched = store.enrichedBundledApps
    const btc = enriched.find(a => a.id === 'bitcoin-knots')
    expect(btc?.lan_address).toBe('http://localhost:8332')
  })

  it('fetchContainers handles non-Error exceptions', async () => {
    mockedClient.listContainers.mockRejectedValue('string error')
    const store = useContainerStore()

    await store.fetchContainers()

    expect(store.error).toBe('Failed to fetch containers')
  })
})
