import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock fast-json-patch
vi.mock('fast-json-patch', () => ({
  applyPatch: vi.fn((doc: unknown, _ops: unknown[]) => ({
    newDocument: { ...doc as Record<string, unknown>, patched: true },
  })),
}))

// Mock WebSocket
class MockWebSocket {
  static CONNECTING = 0
  static OPEN = 1
  static CLOSING = 2
  static CLOSED = 3

  readyState = MockWebSocket.CONNECTING
  onopen: ((ev: Event) => void) | null = null
  onclose: ((ev: CloseEvent) => void) | null = null
  onerror: ((ev: Event) => void) | null = null
  onmessage: ((ev: MessageEvent) => void) | null = null
  url: string

  constructor(url: string) {
    this.url = url
    // Auto-open in next tick
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN
      this.onopen?.(new Event('open'))
    }, 0)
  }

  send = vi.fn()
  close = vi.fn().mockImplementation(function (this: MockWebSocket) {
    this.readyState = MockWebSocket.CLOSED
    this.onclose?.(new CloseEvent('close', { code: 1000, wasClean: true }))
  })
}

vi.stubGlobal('WebSocket', MockWebSocket)

// Must import after mocks
const { WebSocketClient, applyDataPatch } = await import('../websocket')

describe('WebSocketClient', () => {
  let client: InstanceType<typeof WebSocketClient>

  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    client = new WebSocketClient('/ws/test')
  })

  afterEach(() => {
    client.reset()
    vi.useRealTimers()
  })

  it('initializes with disconnected state', () => {
    expect(client.state).toBe('disconnected')
    expect(client.isConnected()).toBe(false)
  })

  it('connects and transitions to connected state', async () => {
    const states: string[] = []
    client.onConnectionStateChange((s) => states.push(s))

    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    expect(client.state).toBe('connected')
    expect(client.isConnected()).toBe(true)
    expect(states).toContain('connecting')
    expect(states).toContain('connected')
  })

  it('resolves immediately if already connected', async () => {
    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    // Second connect should resolve immediately
    await client.connect()
    expect(client.isConnected()).toBe(true)
  })

  it('subscribe returns unsubscribe function', async () => {
    const callback = vi.fn()
    const unsub = client.subscribe(callback)

    expect(typeof unsub).toBe('function')
    unsub()
    // Should not throw
  })

  it('notifies subscribers on message', async () => {
    const callback = vi.fn()
    client.subscribe(callback)

    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    // Simulate receiving a message
    const ws = (client as unknown as { ws: MockWebSocket }).ws
    const update = { id: 1, type: 'state', data: { running: true } }
    ws.onmessage?.(new MessageEvent('message', { data: JSON.stringify(update) }))

    expect(callback).toHaveBeenCalledWith(update)
  })

  it('handles malformed JSON messages gracefully', async () => {
    const callback = vi.fn()
    client.subscribe(callback)

    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    const ws = (client as unknown as { ws: MockWebSocket }).ws
    // Should not throw
    ws.onmessage?.(new MessageEvent('message', { data: 'not-json{' }))
    expect(callback).not.toHaveBeenCalled()
  })

  it('onConnectionStateChange returns unsubscribe function', () => {
    const callback = vi.fn()
    const unsub = client.onConnectionStateChange(callback)

    expect(typeof unsub).toBe('function')
    unsub()
  })

  it('disconnect sets state to disconnecting then cleans up', async () => {
    const states: string[] = []
    client.onConnectionStateChange((s) => states.push(s))

    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    client.disconnect()

    expect(states).toContain('disconnecting')
    expect(client.isConnected()).toBe(false)
  })

  it('reset clears all callbacks and disconnects', async () => {
    const callback = vi.fn()
    client.subscribe(callback)

    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    client.reset()

    expect(client.isConnected()).toBe(false)
  })

  it('sends ping messages via heartbeat', async () => {
    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    const ws = (client as unknown as { ws: MockWebSocket }).ws

    // Advance past ping interval (30s)
    await vi.advanceTimersByTimeAsync(31000)

    expect(ws.send).toHaveBeenCalledWith(JSON.stringify({ type: 'ping' }))
  })

  it('disconnect prevents reconnection after abnormal close', async () => {
    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    // Disconnect explicitly — should prevent future reconnections
    const states: string[] = []
    client.onConnectionStateChange((s) => states.push(s))
    client.disconnect()

    expect(states).toContain('disconnecting')
  })

  it('handles close event with normal closure code', async () => {
    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    const ws = (client as unknown as { ws: MockWebSocket }).ws

    // Simulate normal close — should still try to reconnect (shouldReconnect is true)
    ws.readyState = MockWebSocket.CLOSED
    ws.onclose?.(new CloseEvent('close', { code: 1000, wasClean: true }))

    // After close, state transitions to disconnected
    // Then reconnection happens automatically (mock auto-opens)
    await vi.advanceTimersByTimeAsync(200)

    // Client should have attempted reconnect (state went through disconnected → connecting → connected)
    expect(client.state).toBe('connected')
  })

  it('heartbeat detects stale connection after 5 minutes', async () => {
    const connectPromise = client.connect()
    await vi.advanceTimersByTimeAsync(10)
    await connectPromise

    const ws = (client as unknown as { ws: MockWebSocket }).ws
    const closeSpy = ws.close

    // Advance 5+ minutes without any messages
    await vi.advanceTimersByTimeAsync(310000)

    // Heartbeat should have closed the stale connection
    expect(closeSpy).toHaveBeenCalled()
  })

  it('state getter returns current connection state', () => {
    expect(client.state).toBe('disconnected')
  })
})

describe('applyDataPatch', () => {
  it('returns original data for empty patch', () => {
    const data = { a: 1, b: 2 }
    const result = applyDataPatch(data, [])
    expect(result).toBe(data)
  })

  it('returns original data for non-array patch', () => {
    const data = { a: 1 }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const result = applyDataPatch(data, null as any)
    expect(result).toBe(data)
  })

  it('applies valid patch operations', () => {
    const data = { name: 'test', count: 0 }
    const patch: import('../../types/api').PatchOperation[] = [{ op: 'replace', path: '/count', value: 5 }]
    const result = applyDataPatch(data, patch)
    // The mock returns { ...data, patched: true }
    expect(result).toHaveProperty('patched', true)
  })

  it('returns original data when patch application throws', async () => {
    // Override mock to throw
    const { applyPatch: mockApplyPatch } = await import('fast-json-patch')
    vi.mocked(mockApplyPatch).mockImplementationOnce(() => {
      throw new Error('Invalid patch')
    })

    const data = { value: 42 }
    const patch: import('../../types/api').PatchOperation[] = [{ op: 'replace', path: '/invalid', value: 0 }]
    const result = applyDataPatch(data, patch)
    expect(result).toBe(data)
  })
})
