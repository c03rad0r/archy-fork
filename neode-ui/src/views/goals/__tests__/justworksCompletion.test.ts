import { describe, expect, it } from 'vitest'
import { isJustworksCompletion } from '../justworksCompletion'
import { GOALS } from '@/data/goals'

describe('Just Works verified setup handoff', () => {
  const frame = {} as Window
  const url = 'https://node.local:8340/setup'
  const message = (changes = {}) => ({
    source: frame, origin: 'https://node.local:8340',
    data: { type: 'justworks:site-connected', site: { id: 'my-bakery', published: true } },
    ...changes,
  }) as MessageEvent

  it('accepts a published page from the active Just Works frame', () => {
    expect(isJustworksCompletion(message(), 'justworks', url, frame)).toBe(true)
  })

  it('rejects other windows, origins, apps, and incomplete results', () => {
    for (const event of [message({ source: {} }), message({ origin: 'https://evil.test' }),
      message({ origin: 'http://node.local:8340' }), message({ data: null }),
      message({ data: { type: 'justworks:site-connected', site: { id: '../x', published: true } } }),
      message({ data: { type: 'justworks:site-connected', site: { id: 'my-bakery', published: false } } }),
    ]) expect(isJustworksCompletion(event, 'justworks', url, frame)).toBe(false)
    expect(isJustworksCompletion(message(), 'filebrowser', url, frame)).toBe(false)
    expect(isJustworksCompletion(message(), 'justworks', url, null)).toBe(false)
  })

  it('requires verified app completion, with no payment-stack dependency', () => {
    const goal = GOALS.find(g => g.id === 'instant-business-site')!
    expect(goal.requiredApps).toEqual(['justworks'])
    const configure = goal.steps.find(s => s.action === 'configure')!
    expect(configure.completion).toBe('app')
    expect(configure.appPath).toBeUndefined()
  })

  it('never completes real setup from demo progress', () => {
    const demo = message({ data: { type: 'justworks:demo-site-connected', site: { id: 'corner-bakery-demo', published: false } } })
    expect(isJustworksCompletion(demo, 'justworks', url, frame)).toBe(false)
    expect(isJustworksCompletion(demo, 'justworks', url, frame, true)).toBe(true)
  })
})
