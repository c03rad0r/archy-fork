import { describe, expect, it } from 'vitest'
import { PackageState, type PackageDataEntry } from '@/types/api'
import { parseSideloadPortMapping, validateSideloadRequest } from '../sideloadValidation'

function makePkg(id: string, title: string, lanAddress?: string): PackageDataEntry {
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
    },
    installed: lanAddress
      ? {
          'current-dependents': {},
          'current-dependencies': {},
          'last-backup': null,
          status: 'running',
          'interface-addresses': {
            main: {
              'lan-address': lanAddress,
              'tor-address': '',
            },
          },
        }
      : undefined,
  }
}

describe('sideloadValidation', () => {
  it('parses host and container port mappings', () => {
    expect(parseSideloadPortMapping('3009:80')).toEqual({ host: 3009, container: 80 })
    expect(parseSideloadPortMapping('')).toBeNull()
  })

  it('rejects malformed port mappings', () => {
    expect(() => parseSideloadPortMapping('3009')).toThrow('host:container')
    expect(() => parseSideloadPortMapping('99999:80')).toThrow('between 1 and 65535')
  })

  it('rejects duplicate app IDs', () => {
    const packages = { excalidraw: makePkg('excalidraw', 'Excalidraw') }
    expect(validateSideloadRequest('excalidraw', '3009:80', packages)).toContain('already installed')
  })

  it('rejects reserved host ports', () => {
    expect(validateSideloadRequest('demo', '9000:80', {})).toContain('reserved')
  })

  it('rejects host ports already used by installed apps', () => {
    const packages = { filebrowser: makePkg('filebrowser', 'File Browser', 'http://localhost:8083') }
    expect(validateSideloadRequest('demo', '8083:80', packages)).toContain('File Browser')
  })

  it('accepts available host ports', () => {
    const packages = { filebrowser: makePkg('filebrowser', 'File Browser', 'http://localhost:8083') }
    expect(validateSideloadRequest('demo', '3018:80', packages)).toBeNull()
  })
})
