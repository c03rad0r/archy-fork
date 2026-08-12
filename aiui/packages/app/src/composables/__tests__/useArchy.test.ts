/**
 * 13-11: requestArchyLibrary (sibling of 13-06's requestArchyContent) and
 * the init-time auto-trigger that fires both over the real bridge — the
 * GAP-FOUND fix. 13-06 built requestArchyContent/content:request/
 * content:push and unit-tested all of it, but nothing in the live UI ever
 * called it (see 13-06-SUMMARY.md's Known Limitations); this pins that the
 * fetch now fires from a real init-time event, not merely from a direct
 * unit-test call.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

type PostedMessage = { type: string; kind?: string; scope?: string; [key: string]: unknown }

describe('useArchy: requestArchyLibrary + init-time auto-trigger (13-11)', () => {
  let originalParent: Window

  beforeEach(() => {
    vi.resetModules()
    originalParent = window.parent
    Object.defineProperty(window, 'parent', {
      value: { postMessage: vi.fn() },
      writable: true,
      configurable: true,
    })
    ;(window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__ = true
  })

  afterEach(() => {
    Object.defineProperty(window, 'parent', {
      value: originalParent,
      writable: true,
      configurable: true,
    })
    delete (window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__
    vi.restoreAllMocks()
  })

  it('requestArchyLibrary sends a content:request with kind "library"', async () => {
    const { useArchy } = await import('@/composables/useArchy')
    const archy = useArchy()
    archy.init()

    archy.requestArchyLibrary('own').catch(() => {})

    expect(window.parent.postMessage).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'content:request', kind: 'library', scope: 'own' }),
      window.location.origin,
    )
    archy.destroy()
  })

  it('init() fires both requestArchyContent and requestArchyLibrary as a live init-time event — not merely callable (GAP-FOUND)', async () => {
    const { useArchy } = await import('@/composables/useArchy')
    const archy = useArchy()

    archy.init()

    const postMessage = window.parent.postMessage as unknown as ReturnType<typeof vi.fn>
    const contentRequests = postMessage.mock.calls
      .map(([msg]) => msg as PostedMessage)
      .filter((msg) => msg.type === 'content:request')

    expect(contentRequests.some((msg) => msg.kind === 'all')).toBe(true)
    expect(contentRequests.some((msg) => msg.kind === 'library')).toBe(true)
    archy.destroy()
  })

  it('exposes requestArchyLibrary on the composable\'s returned API', async () => {
    const { useArchy } = await import('@/composables/useArchy')
    const archy = useArchy()
    expect(typeof archy.requestArchyLibrary).toBe('function')
  })
})
