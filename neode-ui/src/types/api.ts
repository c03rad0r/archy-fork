// API Types ported from Angular codebase

export interface DataModel {
  'server-info': ServerInfo
  'package-data': { [id: string]: PackageDataEntry }
  'peer-health'?: { [onion: string]: boolean }
  notifications?: AppNotification[]
  ui: UIData
}

export interface AppNotification {
  id: string
  level: 'info' | 'warning' | 'error'
  title: string
  message: string
  timestamp: string
  app_id?: string
}

export interface ServerInfo {
  id: string
  version: string
  name: string | null
  pubkey: string
  'status-info': StatusInfo
  'lan-address': string | null
  'tor-address': string | null
  /** Live probe of the Tor daemon. NOT the same as `tor-address`, which is read
   *  from the hidden-service hostname file and outlives a dead daemon.
   *  Optional because daemons older than 2026-08-09 do not send it — consumers
   *  must treat absent as "unknown", and only `=== true` as connected. */
  'tor-running'?: boolean
  'node-address'?: string
  unread: number
  'wifi-ssids': string[]
  'zram-enabled': boolean
  'seed-backed': boolean
  lat?: number | null
  lon?: number | null
  'share-location'?: boolean
}

export interface StatusInfo {
  restarting: boolean
  'shutting-down': boolean
  'updated': boolean
  'backup-progress': number | null
  'update-progress': number | null
  'containers-scanned'?: boolean
}

export type UIMode = 'gamer' | 'easy' | 'chat'

export interface UIData {
  name: string | null
  'ack-welcome': string
  marketplace: UIMarketplaceData
  theme: string
  mode?: UIMode
}

export interface UIMarketplaceData {
  'selected-hosts': string[]
  'known-hosts': Record<string, MarketplaceHost>
}

export interface MarketplaceHost {
  name: string
  url: string
}

export const PackageState = {
  Installing: 'installing',
  Installed: 'installed',
  Stopping: 'stopping',
  Stopped: 'stopped',
  Exited: 'exited',
  Starting: 'starting',
  Running: 'running',
  Restarting: 'restarting',
  Creating: 'creating-backup',
  Restoring: 'restoring-backup',
  Removing: 'removing',
  BackingUp: 'backing-up',
  Updating: 'updating',
} as const

export type PackageState = typeof PackageState[keyof typeof PackageState]

export interface PackageDataEntry {
  state: PackageState
  health?: string | null // "healthy", "unhealthy", "starting", or null
  'exit-code'?: number | null // container exit code: 0 = clean stop, non-zero = crash
  'static-files'?: {
    license: string
    instructions: string
    icon: string
    screenshots?: AppScreenshot[]
  }
  manifest: Manifest
  installed?: InstalledPackageDataEntry
  'install-progress'?: InstallProgress
  /** Live label for the current uninstall step ("Stopping containers (2/5)", …). */
  'uninstall-stage'?: string | null
  'available-update'?: string | null
}

export interface Manifest {
  id: string
  title: string
  version: string
  description: {
    short: string
    long: string
  }
  'release-notes': string
  license: string
  'wrapper-repo': string
  'upstream-repo': string
  'support-site': string
  'marketing-site': string
  'donation-url': string | null
  author?: string
  website?: string
  tier?: string
  interfaces?: {
    main?: {
      ui?: string
      'tor-config'?: string
      'lan-config'?: string
    }
  }
  screenshots?: AppScreenshot[]
}

export type AppScreenshot = string | {
  src: string
  alt?: string
}

export interface InstalledPackageDataEntry {
  'current-dependents': Record<string, CurrentDependencyInfo>
  'current-dependencies': Record<string, CurrentDependencyInfo>
  'last-backup': string | null
  'interface-addresses': Record<string, InterfaceAddress>
  status: ServiceStatus
}

export interface CurrentDependencyInfo {
  'health-checks': string[]
}

export interface InterfaceAddress {
  'tor-address': string
  'lan-address': string | null
}

export interface AppCredential {
  label: string
  value: string
  sensitive?: boolean
}

export interface AppCredentialsResponse {
  title?: string
  description?: string
  credentials: AppCredential[]
}

export const ServiceStatus = {
  Stopped: 'stopped',
  Starting: 'starting',
  Running: 'running',
  Stopping: 'stopping',
  Restarting: 'restarting',
} as const

export type ServiceStatus = typeof ServiceStatus[keyof typeof ServiceStatus]

export type InstallPhase =
  | 'preparing'
  | 'pulling-image'
  | 'creating-container'
  | 'starting-container'
  | 'waiting-healthy'
  | 'post-install'
  | 'done'

export interface InstallProgress {
  size: number
  downloaded: number
  /** High-level pipeline phase. Preferred by the UI over the byte
   *  counters — podman pull doesn't emit parseable progress when
   *  stderr is piped, so byte counters are usually (0,0). */
  phase?: InstallPhase
  /** Optional explicit message — surfaced on install failures so the
   *  UI can show what went wrong instead of silently removing the card. */
  message?: string
}

// RPC Request/Response types
export namespace RR {
  // Auth
  export interface LoginReq {
    password: string
    metadata: SessionMetadata
  }
  export type LoginRes = null

  export interface SessionMetadata {
    // Add session metadata fields
  }

  export interface LogoutReq {}
  export type LogoutRes = null

  export interface ResetPasswordReq {
    'old-password': string
    'new-password': string
  }
  export type ResetPasswordRes = null

  // Server
  export interface EchoReq {
    message: string
    timeout?: number
  }
  export type EchoRes = string

  export interface GetSystemTimeReq {}
  export interface GetSystemTimeRes {
    now: string
    uptime: number
  }

  export interface GetServerMetricsReq {}
  export interface GetServerMetricsRes {
    cpu: number
    disk: DiskInfo
    memory: MemoryInfo
  }

  export interface DiskInfo {
    used: number
    total: number
  }

  export interface MemoryInfo {
    used: number
    total: number
  }

  export interface UpdateServerReq {
    'marketplace-url': string
  }
  export type UpdateServerRes = 'updating' | 'no-updates'

  export interface RestartServerReq {}
  export type RestartServerRes = null

  export interface ShutdownServerReq {}
  export type ShutdownServerRes = null

  // Packages
  export interface InstallPackageReq {
    id: string
    'marketplace-url': string
    version: string
  }
  export type InstallPackageRes = string // guid

  export interface UninstallPackageReq {
    id: string
  }
  export type UninstallPackageRes = null

  export interface StartPackageReq {
    id: string
  }
  export type StartPackageRes = null

  export interface StopPackageReq {
    id: string
  }
  export type StopPackageRes = null

  export interface RestartPackageReq {
    id: string
  }
  export type RestartPackageRes = null
}

// JSON Patch types
export interface PatchOperation {
  op: 'add' | 'remove' | 'replace' | 'move' | 'copy' | 'test'
  path: string
  value?: unknown
  from?: string
}

export interface Update {
  sequence: number
  patch: PatchOperation[]
}
