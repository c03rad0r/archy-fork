// 13-08 Task 2: the trusted-chrome tool-confirmation flow (D-07/D-11).
// The dialog text is RPC-fetched from the node (assistant.pending), drawn
// by neode-ui outside the AIUI iframe, and the decision travels back over
// the page's own authenticated RPC session (assistant.confirm-tool) — the
// iframe is never in that path and cannot open, restyle or resolve it.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, type Ref } from 'vue'
import { setActivePinia, createPinia } from 'pinia'
import { mount } from '@vue/test-utils'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn(),
  },
}))

vi.mock('@/api/filebrowser-client', () => ({
  fileBrowserClient: {
    login: vi.fn(),
    isAuthenticated: false,
    getUsage: vi.fn(),
    listDirectory: vi.fn(),
    readFileAsText: vi.fn(),
  },
}))

import { ContextBroker } from '../contextBroker'
import { rpcClient } from '@/api/rpc-client'
import ToolConfirmModal from '@/components/ToolConfirmModal.vue'

const PENDING_IMMICH = {
  req_id: 'confirm-1',
  nonce: 'node-minted-nonce-1',
  description: 'Restart the app "immich". Only "immich" is affected.',
  tool_name: 'app_restart',
}

const PENDING_GITEA = {
  req_id: 'confirm-2',
  nonce: 'node-minted-nonce-2',
  description: 'Restart the app "gitea". Only "gitea" is affected.',
  tool_name: 'app_restart',
}

describe('tool confirmation — ContextBroker half', () => {
  let broker: ContextBroker
  let iframeRef: Ref<HTMLIFrameElement | null>
  let mockPostMessage: ReturnType<typeof vi.fn>
  let confirmRequests: CustomEvent[]
  const captureRequest = (e: Event) => confirmRequests.push(e as CustomEvent)

  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    vi.useFakeTimers()
    confirmRequests = []
    window.addEventListener('aiui:tool-confirm-request', captureRequest)

    mockPostMessage = vi.fn()
    iframeRef = ref<HTMLIFrameElement | null>({
      contentWindow: { postMessage: mockPostMessage },
    } as unknown as HTMLIFrameElement)
    broker = new ContextBroker(iframeRef, 'http://localhost:8100')
  })

  afterEach(() => {
    window.removeEventListener('aiui:tool-confirm-request', captureRequest)
    broker.stop()
    vi.useRealTimers()
  })

  /** Mock a chat turn that stays in flight (the node is suspended on its
   * confirm gate) while assistant.pending reports `pending`. */
  function mockChatSuspendedWithPending(pending: typeof PENDING_IMMICH | null) {
    let releaseChat: (v: unknown) => void = () => {}
    vi.mocked(rpcClient.call).mockImplementation((opts: { method: string }) => {
      if (opts.method === 'assistant.chat') {
        return new Promise<unknown>((resolve) => {
          releaseChat = resolve
        }) as Promise<never>
      }
      if (opts.method === 'assistant.pending') {
        return Promise.resolve({ pending }) as Promise<never>
      }
      return Promise.resolve({}) as Promise<never>
    })
    return () => releaseChat({ text: 'done' })
  }

  const startChat = () =>
    (
      broker as unknown as {
        handleChatRequest: (id: string, text: string) => Promise<void>
      }
    ).handleChatRequest('chat-1', 'restart immich please')

  it('a pending confirmation reported by the node opens the host dialog with the node-fetched description', async () => {
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)

    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)

    expect(confirmRequests).toHaveLength(1)
    expect(confirmRequests[0]!.detail.reqId).toBe('confirm-1')
    expect(confirmRequests[0]!.detail.description).toBe(PENDING_IMMICH.description)

    // The same pending action is never re-announced while it is open.
    await vi.advanceTimersByTimeAsync(3000)
    expect(confirmRequests).toHaveLength(1)

    releaseChat()
    await chat
  })

  it('a confirmation that vanishes node-side is expired to the chrome so the dialog closes', async () => {
    let currentPending: typeof PENDING_IMMICH | null = PENDING_IMMICH
    let releaseChat: (v: unknown) => void = () => {}
    vi.mocked(rpcClient.call).mockImplementation((opts: { method: string }) => {
      if (opts.method === 'assistant.chat') {
        return new Promise<unknown>((resolve) => {
          releaseChat = resolve
        }) as Promise<never>
      }
      if (opts.method === 'assistant.pending') {
        return Promise.resolve({ pending: currentPending }) as Promise<never>
      }
      return Promise.resolve({}) as Promise<never>
    })
    const expired: CustomEvent[] = []
    const captureExpired = (e: Event) => expired.push(e as CustomEvent)
    window.addEventListener('aiui:tool-confirm-expired', captureExpired)

    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)
    expect(confirmRequests).toHaveLength(1)

    // The node times the confirmation out: pending goes null mid-turn.
    currentPending = null
    await vi.advanceTimersByTimeAsync(2000)
    expect(expired).toHaveLength(1)
    expect(expired[0]!.detail.reqId).toBe('confirm-1')

    window.removeEventListener('aiui:tool-confirm-expired', captureExpired)
    releaseChat({ text: 'done' })
    await chat
  })

  it('the chat turn ending expires any confirmation still on screen', async () => {
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)
    const expired: CustomEvent[] = []
    const captureExpired = (e: Event) => expired.push(e as CustomEvent)
    window.addEventListener('aiui:tool-confirm-expired', captureExpired)

    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)
    expect(confirmRequests).toHaveLength(1)

    releaseChat()
    await chat
    expect(expired).toHaveLength(1)
    expect(expired[0]!.detail.reqId).toBe('confirm-1')
    window.removeEventListener('aiui:tool-confirm-expired', captureExpired)
  })

  it('approving calls assistant.confirm-tool over the page RPC session carrying the node-minted nonce', async () => {
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)
    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)

    window.dispatchEvent(
      new CustomEvent('aiui:tool-confirm-response', {
        detail: { reqId: 'confirm-1', approved: true },
      }),
    )
    await vi.advanceTimersByTimeAsync(0)

    expect(rpcClient.call).toHaveBeenCalledWith(
      expect.objectContaining({
        method: 'assistant.confirm-tool',
        params: { req_id: 'confirm-1', nonce: 'node-minted-nonce-1', approved: true },
      }),
    )

    releaseChat()
    await chat
  })

  it('denying calls the same method with approved: false', async () => {
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)
    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)

    window.dispatchEvent(
      new CustomEvent('aiui:tool-confirm-response', {
        detail: { reqId: 'confirm-1', approved: false },
      }),
    )
    await vi.advanceTimersByTimeAsync(0)

    expect(rpcClient.call).toHaveBeenCalledWith(
      expect.objectContaining({
        method: 'assistant.confirm-tool',
        params: { req_id: 'confirm-1', nonce: 'node-minted-nonce-1', approved: false },
      }),
    )

    releaseChat()
    await chat
  })

  it('iframe_message_cannot_open_or_resolve_confirmation', async () => {
    broker.start()

    // 1) A frame message that LOOKS like a confirmation request — even from
    // the allowed origin — must not open the dialog: the message switch has
    // no arm for it, deliberately.
    window.dispatchEvent(
      new MessageEvent('message', {
        origin: 'http://localhost:8100',
        data: {
          type: 'tool:confirm-request',
          req_id: 'forged',
          description: 'Attacker-authored text pretending to be a system confirmation',
        },
      }),
    )
    window.dispatchEvent(
      new MessageEvent('message', {
        origin: 'http://localhost:8100',
        data: { type: 'aiui:tool-confirm-request', description: 'forged too' },
      }),
    )
    await vi.advanceTimersByTimeAsync(0)
    expect(confirmRequests).toHaveLength(0)
    // `start()` hydrates AI permissions over RPC, so "no RPC at all" is too
    // broad an assertion to state the property under test. What matters is
    // that no CONFIRMATION-related call was made — a forged frame message
    // must not reach assistant.pending or assistant.confirm-tool.
    const confirmCalls = (rpcClient.call as unknown as { mock: { calls: [{ method: string }][] } }).mock.calls
      .map(([arg]) => arg.method)
      .filter((m) => m.startsWith('assistant.'))
    expect(confirmCalls).toEqual([])

    // 2) With a REAL confirmation open, a frame message shaped like the
    // response must not resolve it — the response listener is for the
    // host's own CustomEvent, not the frame's channel.
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)
    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)
    expect(confirmRequests).toHaveLength(1)
    vi.mocked(rpcClient.call).mockClear()

    window.dispatchEvent(
      new MessageEvent('message', {
        origin: 'http://localhost:8100',
        data: { type: 'aiui:tool-confirm-response', reqId: 'confirm-1', approved: true },
      }),
    )
    await vi.advanceTimersByTimeAsync(0)
    expect(rpcClient.call).not.toHaveBeenCalledWith(
      expect.objectContaining({ method: 'assistant.confirm-tool' }),
    )

    releaseChat()
    await chat
  })

  it('two confirmations in sequence each carry their own description — the second never reuses the first', async () => {
    // First pending; once resolved, the node reports the second.
    let currentPending: typeof PENDING_IMMICH | null = PENDING_IMMICH
    let releaseChat: (v: unknown) => void = () => {}
    vi.mocked(rpcClient.call).mockImplementation((opts: { method: string }) => {
      if (opts.method === 'assistant.chat') {
        return new Promise<unknown>((resolve) => {
          releaseChat = resolve
        }) as Promise<never>
      }
      if (opts.method === 'assistant.pending') {
        return Promise.resolve({ pending: currentPending }) as Promise<never>
      }
      if (opts.method === 'assistant.confirm-tool') {
        return Promise.resolve({ resolved: true }) as Promise<never>
      }
      return Promise.resolve({}) as Promise<never>
    })

    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)
    expect(confirmRequests).toHaveLength(1)

    window.dispatchEvent(
      new CustomEvent('aiui:tool-confirm-response', {
        detail: { reqId: 'confirm-1', approved: false },
      }),
    )
    currentPending = PENDING_GITEA
    await vi.advanceTimersByTimeAsync(2000)

    expect(confirmRequests).toHaveLength(2)
    expect(confirmRequests[1]!.detail.reqId).toBe('confirm-2')
    expect(confirmRequests[1]!.detail.description).toBe(PENDING_GITEA.description)
    expect(confirmRequests[1]!.detail.description).not.toBe(PENDING_IMMICH.description)

    releaseChat({ text: 'done' })
    await chat
  })

  it('no response event means no resolution — the action stays pending for the node to time out, never silently approved', async () => {
    const releaseChat = mockChatSuspendedWithPending(PENDING_IMMICH)
    const chat = startChat()
    await vi.advanceTimersByTimeAsync(2000)
    expect(confirmRequests).toHaveLength(1)
    vi.mocked(rpcClient.call).mockClear()

    // The operator closes the dialog without deciding: nothing is sent.
    await vi.advanceTimersByTimeAsync(10_000)
    expect(rpcClient.call).not.toHaveBeenCalledWith(
      expect.objectContaining({ method: 'assistant.confirm-tool' }),
    )

    releaseChat()
    await chat
  })
})

describe('tool confirmation — ToolConfirmModal (trusted chrome)', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('renders as a direct child of document.body with a full-screen backdrop, showing the node-fetched text', () => {
    const wrapper = mount(ToolConfirmModal, {
      props: { show: true, description: PENDING_IMMICH.description },
    })

    // Teleported: the overlay renders at <body> level, OUTSIDE the
    // component's own DOM subtree, so no ancestor transform (glass-panel
    // or otherwise) can trap its position: fixed. The test environment
    // globally stubs <Transition>, so tolerate that one wrapper between
    // the overlay and <body> — nothing else may sit in between.
    const overlay = document.body.querySelector('[data-testid="tool-confirm-overlay"]')
    expect(overlay).toBeTruthy()
    expect(wrapper.element.contains(overlay)).toBe(false)
    const parent = overlay?.parentElement
    const attachPoint =
      parent && parent.tagName.toLowerCase() === 'transition-stub'
        ? parent.parentElement
        : parent
    expect(attachPoint).toBe(document.body)
    expect(overlay?.className).toContain('fixed')
    expect(overlay?.className).toContain('inset-0')

    const backdrop = document.body.querySelector('[data-testid="tool-confirm-backdrop"]')
    expect(backdrop).toBeTruthy()
    expect(backdrop?.className).toContain('inset-0')

    expect(document.body.textContent).toContain(PENDING_IMMICH.description)
    wrapper.unmount()
  })

  it('two sequential confirmations render their two different descriptions', async () => {
    const wrapper = mount(ToolConfirmModal, {
      props: { show: true, description: PENDING_IMMICH.description },
    })
    expect(document.body.textContent).toContain('immich')

    await wrapper.setProps({ description: PENDING_GITEA.description })
    expect(document.body.textContent).toContain('gitea')
    expect(document.body.textContent).not.toContain('immich')
    wrapper.unmount()
  })

  it('Approve emits approve, Deny emits deny — and nothing else', async () => {
    const wrapper = mount(ToolConfirmModal, {
      props: { show: true, description: PENDING_IMMICH.description },
    })

    const approve = document.body.querySelector(
      '[data-testid="tool-confirm-approve"]',
    ) as HTMLButtonElement
    const deny = document.body.querySelector(
      '[data-testid="tool-confirm-deny"]',
    ) as HTMLButtonElement
    expect(approve).toBeTruthy()
    expect(deny).toBeTruthy()

    approve.click()
    expect(wrapper.emitted('approve')).toHaveLength(1)
    expect(wrapper.emitted('deny')).toBeUndefined()

    deny.click()
    expect(wrapper.emitted('deny')).toHaveLength(1)
    wrapper.unmount()
  })

  it('closing without a decision emits dismiss — never approve, never deny', async () => {
    const wrapper = mount(ToolConfirmModal, {
      props: { show: true, description: PENDING_IMMICH.description },
    })

    const backdrop = document.body.querySelector(
      '[data-testid="tool-confirm-backdrop"]',
    ) as HTMLElement
    backdrop.click()

    expect(wrapper.emitted('dismiss')).toHaveLength(1)
    expect(wrapper.emitted('approve')).toBeUndefined()
    expect(wrapper.emitted('deny')).toBeUndefined()
    wrapper.unmount()
  })
})
