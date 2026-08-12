import { describe, it, expect } from 'vitest'
import {
  registerPlugin,
  unregisterPlugin,
  getPlugin,
  getPluginsByType,
  registerRenderer,
  getRendererForContentType,
  getAllRenderers,
} from './plugins/registry'
import type { AIUIPlugin } from './types/plugin'
import type { RendererDefinition } from './types/renderer'

function createMockPlugin(overrides: Partial<AIUIPlugin> = {}): AIUIPlugin {
  return {
    id: 'test-plugin',
    name: 'Test Plugin',
    version: '1.0.0',
    type: 'ai-provider',
    async init() {},
    async destroy() {},
    async isAvailable() { return true },
    ...overrides,
  }
}

describe('core exports', () => {
  it('exports plugin registry functions', () => {
    expect(registerPlugin).toBeTypeOf('function')
    expect(unregisterPlugin).toBeTypeOf('function')
    expect(getPlugin).toBeTypeOf('function')
    expect(getPluginsByType).toBeTypeOf('function')
  })

  it('exports renderer registry functions', () => {
    expect(registerRenderer).toBeTypeOf('function')
    expect(getRendererForContentType).toBeTypeOf('function')
    expect(getAllRenderers).toBeTypeOf('function')
  })
})

describe('plugin registry', () => {
  it('registers and retrieves a plugin', () => {
    const plugin = createMockPlugin({ id: 'reg-test' })
    registerPlugin(plugin)

    const retrieved = getPlugin('reg-test')
    expect(retrieved).toBeDefined()
    expect(retrieved?.id).toBe('reg-test')

    unregisterPlugin('reg-test')
  })

  it('unregisters a plugin', () => {
    const plugin = createMockPlugin({ id: 'unreg-test' })
    registerPlugin(plugin)
    unregisterPlugin('unreg-test')

    expect(getPlugin('unreg-test')).toBeUndefined()
  })

  it('filters plugins by type', () => {
    const p1 = createMockPlugin({ id: 'type-a', type: 'ai-provider' })
    const p2 = createMockPlugin({ id: 'type-b', type: 'storage' })
    registerPlugin(p1)
    registerPlugin(p2)

    const providers = getPluginsByType('ai-provider')
    expect(providers.some(p => p.id === 'type-a')).toBe(true)
    expect(providers.some(p => p.id === 'type-b')).toBe(false)

    unregisterPlugin('type-a')
    unregisterPlugin('type-b')
  })

  it('skips duplicate registration', () => {
    const plugin = createMockPlugin({ id: 'dup-test' })
    registerPlugin(plugin)
    registerPlugin(plugin) // should warn but not throw

    unregisterPlugin('dup-test')
  })
})
