import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock the rpc-client module
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

import { containerClient } from '../container-client'
import { rpcClient } from '@/api/rpc-client'

const mockedRpc = vi.mocked(rpcClient)

describe('containerClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('installApp calls container-install with manifest path', async () => {
    mockedRpc.call.mockResolvedValue('container-abc123')

    const result = await containerClient.installApp('/apps/bitcoin/manifest.yml')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-install',
      params: { manifest_path: '/apps/bitcoin/manifest.yml' },
    })
    expect(result).toBe('container-abc123')
  })

  it('startContainer calls container-start with app_id', async () => {
    mockedRpc.call.mockResolvedValue(undefined)

    await containerClient.startContainer('bitcoin-knots')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-start',
      params: { app_id: 'bitcoin-knots' },
    })
  })

  it('stopContainer calls container-stop with app_id', async () => {
    mockedRpc.call.mockResolvedValue(undefined)

    await containerClient.stopContainer('lnd')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-stop',
      params: { app_id: 'lnd' },
    })
  })

  it('removeContainer calls container-remove with app_id', async () => {
    mockedRpc.call.mockResolvedValue(undefined)

    await containerClient.removeContainer('mempool')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-remove',
      params: { app_id: 'mempool' },
    })
  })

  it('getContainerStatus returns status for a container', async () => {
    const mockStatus = {
      id: '1',
      name: 'bitcoin-knots',
      state: 'running' as const,
      image: 'bitcoinknots:29',
      created: '2026-01-01',
      ports: ['8332'],
      lan_address: 'http://localhost:8332',
    }
    mockedRpc.call.mockResolvedValue(mockStatus)

    const result = await containerClient.getContainerStatus('bitcoin-knots')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-status',
      params: { app_id: 'bitcoin-knots' },
    })
    expect(result).toEqual(mockStatus)
  })

  it('getContainerLogs returns log lines with default line count', async () => {
    const mockLogs = ['Starting bitcoin...', 'Block 850000 synced', 'Peer connected']
    mockedRpc.call.mockResolvedValue(mockLogs)

    const result = await containerClient.getContainerLogs('bitcoin-knots')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-logs',
      params: { app_id: 'bitcoin-knots', lines: 100 },
    })
    expect(result).toEqual(mockLogs)
  })

  it('getContainerLogs respects custom line count', async () => {
    mockedRpc.call.mockResolvedValue([])

    await containerClient.getContainerLogs('lnd', 50)

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-logs',
      params: { app_id: 'lnd', lines: 50 },
    })
  })

  it('listContainers returns all containers', async () => {
    const mockContainers = [
      { id: '1', name: 'bitcoin-knots', state: 'running', image: 'bitcoinknots:29', created: '2026-01-01', ports: ['8332'] },
      { id: '2', name: 'lnd', state: 'stopped', image: 'lnd:v0.18', created: '2026-01-01', ports: ['9735'] },
    ]
    mockedRpc.call.mockResolvedValue(mockContainers)

    const result = await containerClient.listContainers()

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-list',
      params: {},
    })
    expect(result).toHaveLength(2)
  })

  it('getHealthStatus returns health map', async () => {
    const mockHealth = { 'bitcoin-knots': 'healthy', lnd: 'unhealthy' }
    mockedRpc.call.mockResolvedValue(mockHealth)

    const result = await containerClient.getHealthStatus()

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'container-health',
      params: {},
    })
    expect(result).toEqual(mockHealth)
  })

  it('startBundledApp sends full app config', async () => {
    mockedRpc.call.mockResolvedValue(undefined)
    const app = {
      id: 'filebrowser',
      name: 'FileBrowser',
      image: 'filebrowser/filebrowser:v2',
      ports: [{ host: 8083, container: 80 }],
      volumes: [{ host: '/var/lib/archipelago/filebrowser', container: '/srv' }],
    }

    await containerClient.startBundledApp(app)

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'bundled-app-start',
      params: {
        app_id: 'filebrowser',
        image: 'filebrowser/filebrowser:v2',
        ports: [{ host: 8083, container: 80 }],
        volumes: [{ host: '/var/lib/archipelago/filebrowser', container: '/srv' }],
      },
    })
  })

  it('stopBundledApp calls bundled-app-stop', async () => {
    mockedRpc.call.mockResolvedValue(undefined)

    await containerClient.stopBundledApp('filebrowser')

    expect(mockedRpc.call).toHaveBeenCalledWith({
      method: 'bundled-app-stop',
      params: { app_id: 'filebrowser' },
    })
  })

  it('propagates RPC errors from the client', async () => {
    mockedRpc.call.mockRejectedValue(new Error('Connection refused'))

    await expect(containerClient.startContainer('bitcoin-knots')).rejects.toThrow('Connection refused')
  })
})
