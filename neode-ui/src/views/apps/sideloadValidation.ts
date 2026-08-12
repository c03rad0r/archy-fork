import type { PackageDataEntry } from '@/types/api'

const RESERVED_HOST_PORTS = new Set([
  80, 443, 81,
  8332, 8333, 8334,
  9735, 10009, 8080,
  18083,
  4080, 8999, 50001,
  23000,
  8173, 8174, 8175,
  8123,
  3000,
  11434,
  9980, 9001,
  8240,
  9000,
  3001, 3002,
  8888,
  8096, 2342, 2283,
  8443,
])

export interface ParsedPortMapping {
  host: number
  container: number
}

export function parseSideloadPortMapping(value: string): ParsedPortMapping | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const match = trimmed.match(/^(\d{1,5}):(\d{1,5})$/)
  if (!match) {
    throw new Error('Port mapping must use host:container format, for example 3009:80.')
  }

  const host = Number(match[1])
  const container = Number(match[2])
  if (!Number.isInteger(host) || host < 1 || host > 65535 || !Number.isInteger(container) || container < 1 || container > 65535) {
    throw new Error('Ports must be between 1 and 65535.')
  }
  return { host, container }
}

export function packageUsesHostPort(pkg: PackageDataEntry, hostPort: number): boolean {
  const addresses = pkg.installed?.['interface-addresses'] || {}
  return Object.values(addresses).some((addr) => {
    const lan = addr?.['lan-address']
    if (!lan) return false
    try {
      const parsed = new URL(lan)
      return Number(parsed.port || (parsed.protocol === 'https:' ? '443' : '80')) === hostPort
    } catch {
      const match = lan.match(/:(\d+)(?:\/|$)/)
      return match ? Number(match[1]) === hostPort : false
    }
  })
}

export function validateSideloadRequest(
  id: string,
  portMapping: string,
  packages: Record<string, PackageDataEntry>,
): string | null {
  if (packages[id]) return `An app with ID "${id}" is already installed.`

  let parsed: ParsedPortMapping | null = null
  try {
    parsed = parseSideloadPortMapping(portMapping)
  } catch (err) {
    return err instanceof Error ? err.message : 'Invalid port mapping.'
  }
  if (!parsed) return null

  if (RESERVED_HOST_PORTS.has(parsed.host)) {
    return `Host port ${parsed.host} is reserved by Archipelago or a packaged app. Choose another host port.`
  }

  const existing = Object.entries(packages).find(([, pkg]) => packageUsesHostPort(pkg, parsed.host))
  if (existing) {
    const title = existing[1].manifest?.title || existing[0]
    return `Host port ${parsed.host} is already used by ${title}. Choose another host port.`
  }

  return null
}
