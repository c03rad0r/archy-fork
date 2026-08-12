// Container management API client
// Extends RPC client with container-specific methods

import { rpcClient } from './rpc-client'

export interface ContainerStatus {
  id: string
  name: string
  state:
    | 'created'
    | 'running'
    | 'stopped'
    | 'exited'
    | 'paused'
    | 'unknown'
    | 'stopping'
    | 'starting'
    | 'restarting'
    | 'installing'
    | 'updating'
    | 'removing'
    | 'installed'
  image: string
  created: string
  ports: string[]
  lan_address?: string // Launch URL for the app's UI
}

export interface ContainerAppInfo {
  id: string
  name: string
  version: string
  status: ContainerStatus
  health: 'healthy' | 'unhealthy' | 'unknown' | 'starting'
}

export interface BundledAppConfig {
  id: string
  name: string
  image: string
  ports: { host: number; container: number }[]
  volumes: { host: string; container: string }[]
}

export const containerClient = {
  /**
   * Install a container app from a manifest file
   */
  async installApp(manifestPath: string): Promise<string> {
    return rpcClient.call<string>({
      method: 'container-install',
      params: { manifest_path: manifestPath },
    })
  },

  /**
   * Start a container
   */
  async startContainer(appId: string): Promise<void> {
    return rpcClient.call<void>({
      method: 'container-start',
      params: { app_id: appId },
    })
  },

  /**
   * Stop a container
   */
  async stopContainer(appId: string): Promise<void> {
    return rpcClient.call<void>({
      method: 'container-stop',
      params: { app_id: appId },
    })
  },

  /**
   * Restart a container (async; returns immediately with restarting state)
   */
  async restartContainer(appId: string): Promise<void> {
    return rpcClient.call<void>({
      method: 'container-restart',
      params: { app_id: appId },
    })
  },

  /**
   * Remove a container
   */
  async removeContainer(appId: string): Promise<void> {
    return rpcClient.call<void>({
      method: 'container-remove',
      params: { app_id: appId },
    })
  },

  /**
   * Get container status
   */
  async getContainerStatus(appId: string): Promise<ContainerStatus> {
    return rpcClient.call<ContainerStatus>({
      method: 'container-status',
      params: { app_id: appId },
    })
  },

  /**
   * Get container logs
   */
  async getContainerLogs(appId: string, lines: number = 100): Promise<string[]> {
    return rpcClient.call<string[]>({
      method: 'container-logs',
      params: { app_id: appId, lines },
    })
  },

  /**
   * List all containers
   */
  async listContainers(): Promise<ContainerStatus[]> {
    return rpcClient.call<ContainerStatus[]>({
      method: 'container-list',
      params: {},
    })
  },

  /**
   * Get health status for all containers
   */
  async getHealthStatus(): Promise<Record<string, string>> {
    return rpcClient.call<Record<string, string>>({
      method: 'container-health',
      params: {},
    })
  },

  /**
   * Start a bundled app (creates container if needed, then starts it)
   */
  async startBundledApp(app: BundledAppConfig): Promise<void> {
    return rpcClient.call<void>({
      method: 'bundled-app-start',
      params: {
        app_id: app.id,
        image: app.image,
        ports: app.ports,
        volumes: app.volumes,
      },
    })
  },

  /**
   * Stop a bundled app
   */
  async stopBundledApp(appId: string): Promise<void> {
    return rpcClient.call<void>({
      method: 'bundled-app-stop',
      params: { app_id: appId },
    })
  },
}
