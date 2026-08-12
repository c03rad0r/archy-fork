// WebSocket handler for real-time updates

import type { Update, PatchOperation } from '../types/api'
import { applyPatch, type Operation } from 'fast-json-patch'

export type ConnectionState = 'connecting' | 'connected' | 'disconnecting' | 'disconnected'

type WebSocketCallback = (update: Update) => void
type ConnectionStateCallback = (state: ConnectionState) => void

export class WebSocketClient {
  private ws: WebSocket | null = null
  private callbacks: Set<WebSocketCallback> = new Set()
  private connectionStateCallbacks: Set<ConnectionStateCallback> = new Set()
  private reconnectAttempts = 0
  private maxReconnectAttempts = 10
  private reconnectDelay = 1000
  private maxReconnectDelay = 30000
  private shouldReconnect = true
  private url: string
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private visibilityChangeHandler: (() => void) | null = null
  private onlineHandler: (() => void) | null = null
  private heartbeatTimer: ReturnType<typeof setTimeout> | null = null
  private pingTimer: ReturnType<typeof setInterval> | null = null
  private lastMessageTime: number = Date.now()
  private heartbeatInterval = 10000 // Check connection every 10 seconds
  private pingInterval = 30000 // Send ping every 30 seconds
  private _state: ConnectionState = 'disconnected'
  private isReconnecting = false
  private parseErrorCount = 0
  private connectCheckInterval: ReturnType<typeof setInterval> | null = null

  constructor(url: string = '/ws/db') {
    this.url = url
    this.setupBrowserEventHandlers()
  }

  private setupBrowserEventHandlers(): void {
    if (typeof window === 'undefined') return

    // Handle page visibility changes (tab switching, browser minimizing)
    this.visibilityChangeHandler = () => {
      if (document.visibilityState === 'visible') {
        if (import.meta.env.DEV) console.log('[WebSocket] Page became visible, checking connection...')
        // Only reconnect if we haven't been explicitly disconnected
        if (this.shouldReconnect && (!this.ws || this.ws.readyState !== WebSocket.OPEN)) {
          if (import.meta.env.DEV) console.log('[WebSocket] Connection lost while hidden, reconnecting...')
          this.reconnectAttempts = 0
          this.connect().catch(err => {
            if (import.meta.env.DEV) console.error('[WebSocket] Failed to reconnect on visibility change:', err)
          })
        }
      }
    }
    document.addEventListener('visibilitychange', this.visibilityChangeHandler)

    // Handle network online/offline events
    this.onlineHandler = () => {
      // Only reconnect if we haven't been explicitly disconnected
      if (!this.shouldReconnect) return
      if (import.meta.env.DEV) console.log('[WebSocket] Network came online, reconnecting...')
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        this.reconnectAttempts = 0
        this.connect().catch(err => {
          if (import.meta.env.DEV) console.error('[WebSocket] Failed to reconnect when network came online:', err)
        })
      }
    }
    window.addEventListener('online', this.onlineHandler)
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      // If already connected, resolve immediately
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        if (import.meta.env.DEV) console.log('[WebSocket] Already connected, skipping')
        resolve()
        return
      }
      
      // If connecting, wait for it
      if (this.ws && this.ws.readyState === WebSocket.CONNECTING) {
        if (import.meta.env.DEV) console.log('[WebSocket] Already connecting, waiting...')
        this.clearConnectCheck()
        this.connectCheckInterval = setInterval(() => {
          if (this.ws) {
            if (this.ws.readyState === WebSocket.OPEN) {
              this.clearConnectCheck()
              resolve()
            } else if (this.ws.readyState === WebSocket.CLOSED || this.ws.readyState === WebSocket.CLOSING) {
              this.clearConnectCheck()
              // Connection failed or closing, will be handled by onclose
              reject(new Error('Connection closed during connect'))
            }
          } else {
            this.clearConnectCheck()
            reject(new Error('WebSocket was cleared'))
          }
        }, 100)

        // Timeout after 5 seconds
        setTimeout(() => {
          this.clearConnectCheck()
          if (this.ws && this.ws.readyState !== WebSocket.OPEN) {
            reject(new Error('Connection timeout'))
          }
        }, 5000)
        return
      }
      
      // Don't close existing connection if it's still active
      // Only close if it's in CLOSING or CLOSED state
      if (this.ws && (this.ws.readyState === WebSocket.CLOSING || this.ws.readyState === WebSocket.CLOSED)) {
        this.ws = null
      }
      
      // If we have an active WebSocket, don't create a new one
      if (this.ws) {
        if (import.meta.env.DEV) console.log('[WebSocket] Connection exists, reusing it')
        resolve()
        return
      }
      
      // Only enable reconnect if not explicitly disconnected
      // (shouldReconnect is set to false by disconnect())
      if (this.shouldReconnect !== false) {
        this.shouldReconnect = true
      }
      
      // In development, Vite proxies /ws to the backend
      // In production, use the same host as the page
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      const host = window.location.host
      const wsUrl = `${protocol}//${host}${this.url}`
      
      if (import.meta.env.DEV) console.log('[WebSocket] Connecting to:', wsUrl)

      this.setConnectionState('connecting')
      this.ws = new WebSocket(wsUrl)
      
      // Timeout handler in case connection hangs
      const connectionTimeout = setTimeout(() => {
        if (this.ws && this.ws.readyState === WebSocket.CONNECTING) {
          if (import.meta.env.DEV) console.warn('WebSocket connection timeout, retrying...')
          this.ws.close()
          reject(new Error('Connection timeout'))
        }
      }, 3000) // 3 second timeout

      this.ws.onopen = () => {
        clearTimeout(connectionTimeout)
        this.reconnectAttempts = 0
        this.lastMessageTime = Date.now()
        if (import.meta.env.DEV) console.log('[WebSocket] Connected successfully')
        this.setConnectionState('connected')
        this.startHeartbeat()
        resolve()
      }

      this.ws.onerror = (error) => {
        clearTimeout(connectionTimeout)
        if (import.meta.env.DEV) console.error('[WebSocket] Connection error:', error)
        // Don't reject immediately - let onclose handle reconnection
        // This prevents errors from blocking reconnection
      }

      this.ws.onmessage = (event) => {
        this.lastMessageTime = Date.now()
        try {
          const update: Update = JSON.parse(event.data)
          this.parseErrorCount = 0
          this.callbacks.forEach((callback) => callback(update))
        } catch (error) {
          this.parseErrorCount++
          if (import.meta.env.DEV) console.error(`Failed to parse WebSocket message (${this.parseErrorCount} consecutive):`, error)
          if (this.parseErrorCount > 3) {
            if (import.meta.env.DEV) console.warn('[WebSocket] Too many parse errors, closing to trigger reconnection')
            this.ws?.close()
          }
        }
      }

      this.ws.onclose = (event) => {
        clearTimeout(connectionTimeout)
        this.stopHeartbeat()
        if (import.meta.env.DEV) console.log('[WebSocket] Closed', { code: event.code, reason: event.reason, wasClean: event.wasClean })
        
        // Notify connection state changed
        this.setConnectionState('disconnected')
        
        // Clear the WebSocket reference
        this.ws = null
        
        // Don't reconnect if we explicitly disconnected
        if (!this.shouldReconnect) {
          if (import.meta.env.DEV) console.log('[WebSocket] Reconnection disabled')
          return
        }
        
        // Always try to reconnect unless we've exceeded max attempts
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
          const isHMR = event.code === 1001
          const isNormalClosure = event.code === 1000 || event.code === 1001
          const isServiceRestart = event.code === 1012
          
          // Immediate reconnection for HMR, service restarts, and first attempt after abnormal closure
          const needsImmediateReconnect = isHMR || isServiceRestart || (event.code === 1006 && this.reconnectAttempts === 0)
          
          const delay = needsImmediateReconnect ? 0 :
                       (this.reconnectAttempts === 0 ? 100 :
                        Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts), this.maxReconnectDelay))
          
          if (import.meta.env.DEV) console.log(`[WebSocket] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts + 1}/${this.maxReconnectAttempts}, code: ${event.code})`)
          
          // Clear any existing reconnect timer
          if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer)
            this.reconnectTimer = null
          }
          
          const doReconnect = () => {
            // Check again if we should reconnect (might have been disabled)
            if (!this.shouldReconnect) {
              return
            }

            // Prevent parallel reconnections from duplicate onclose events
            if (this.isReconnecting) {
              if (import.meta.env.DEV) console.log('[WebSocket] Reconnection already in progress, skipping')
              return
            }

            // Don't increment attempts for expected disconnects (HMR, normal closure)
            if (!isHMR && !isNormalClosure) {
              this.reconnectAttempts++
            }

            if (import.meta.env.DEV) console.log('[WebSocket] Attempting reconnection...')
            this.isReconnecting = true
            this.connect().then(() => {
              this.isReconnecting = false
            }).catch((err) => {
              this.isReconnecting = false
              if (import.meta.env.DEV) console.error('[WebSocket] Reconnection failed:', err)
              // onclose will be called again and will retry
            })
          }
          
          if (delay === 0) {
            // Immediate reconnection
            doReconnect()
          } else {
            this.reconnectTimer = setTimeout(() => {
              this.reconnectTimer = null
              doReconnect()
            }, delay)
          }
        } else {
          if (import.meta.env.DEV) console.warn('[WebSocket] Max reconnection attempts reached')
          this.shouldReconnect = false
        }
      }
    })
  }

  subscribe(callback: WebSocketCallback): () => void {
    this.callbacks.add(callback)
    return () => {
      this.callbacks.delete(callback)
    }
  }

  get state(): ConnectionState {
    return this._state
  }

  onConnectionStateChange(callback: ConnectionStateCallback): () => void {
    this.connectionStateCallbacks.add(callback)
    return () => {
      this.connectionStateCallbacks.delete(callback)
    }
  }

  private setConnectionState(state: ConnectionState): void {
    this._state = state
    this.connectionStateCallbacks.forEach((callback) => callback(state))
  }

  private clearConnectCheck(): void {
    if (this.connectCheckInterval) {
      clearInterval(this.connectCheckInterval)
      this.connectCheckInterval = null
    }
  }

  private startHeartbeat(): void {
    this.stopHeartbeat()

    // Send ping messages every 30s
    this.pingTimer = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        try {
          this.ws.send(JSON.stringify({ type: 'ping' }))
        } catch {
          // Send failed, connection likely broken
        }
      }
    }, this.pingInterval)

    // Check connection health every 10s
    this.heartbeatTimer = setInterval(() => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        if (import.meta.env.DEV) console.warn('[WebSocket] Heartbeat detected closed connection')
        this.stopHeartbeat()
        return
      }

      const timeSinceLastMessage = Date.now() - this.lastMessageTime

      // If no message for more than 5 minutes, assume connection is stale
      if (timeSinceLastMessage > 300000) {
        if (import.meta.env.DEV) console.warn('[WebSocket] No messages for 5m, reconnecting...')
        this.ws.close()
        return
      }
    }, this.heartbeatInterval)
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer)
      this.heartbeatTimer = null
    }
    if (this.pingTimer) {
      clearInterval(this.pingTimer)
      this.pingTimer = null
    }
  }

  disconnect(): void {
    this.shouldReconnect = false
    this.reconnectAttempts = 0
    this.setConnectionState('disconnecting')
    this.stopHeartbeat()
    this.clearConnectCheck()

    // Clear reconnect timer
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    
    if (this.ws) {
      // Remove handlers to prevent reconnection
      this.ws.onclose = null
      this.ws.onerror = null
      try {
        this.ws.close()
      } catch (e) {
        if (import.meta.env.DEV) console.warn('WebSocket close error', e)
      }
      this.ws = null
    }
  }
  
  reset(): void {
    this.disconnect()
    this.callbacks.clear()
    
    // Clean up browser event handlers
    if (this.visibilityChangeHandler) {
      document.removeEventListener('visibilitychange', this.visibilityChangeHandler)
      this.visibilityChangeHandler = null
    }
    if (this.onlineHandler) {
      window.removeEventListener('online', this.onlineHandler)
      this.onlineHandler = null
    }
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN
  }
}

// Create singleton that persists across HMR
let wsClientInstance: WebSocketClient | null = null

function getWebSocketClient(): WebSocketClient {
  if (typeof window === 'undefined') {
    // SSR - create new instance
    if (!wsClientInstance) {
      wsClientInstance = new WebSocketClient()
    }
    return wsClientInstance
  }
  
  // Check if we have a persisted instance from HMR
  const existing = (window as unknown as Record<string, unknown>).__archipelago_ws_client
  if (existing && existing instanceof WebSocketClient) {
    // Check if the WebSocket is still valid
    if (existing.isConnected()) {
      if (import.meta.env.DEV) console.log('[WebSocket] Using existing connected client from HMR')
      wsClientInstance = existing
      return existing
    }
  }
  
  // Create new instance
  if (!wsClientInstance) {
    wsClientInstance = new WebSocketClient()
    if (typeof window !== 'undefined') {
      ;(window as unknown as Record<string, unknown>).__archipelago_ws_client = wsClientInstance
    }
    if (import.meta.env.DEV) console.debug('[WebSocket] Created new client instance')
  }
  
  return wsClientInstance
}

// Lazy initialization - only create when accessed
let _wsClient: WebSocketClient | null = null

export const wsClient: WebSocketClient = (() => {
  if (_wsClient) {
    return _wsClient
  }
  try {
    _wsClient = getWebSocketClient()
    return _wsClient
  } catch (error) {
    if (import.meta.env.DEV) console.error('[WebSocket] Error initializing client:', error)
    // Fallback to new instance
    _wsClient = new WebSocketClient()
    return _wsClient
  }
})()

// Helper to apply patches to data
export function applyDataPatch<T>(data: T, patch: PatchOperation[]): T {
  // Validate patch is an array before applying
  if (!Array.isArray(patch) || patch.length === 0) {
    if (import.meta.env.DEV) console.warn('Invalid or empty patch received, returning original data')
    return data
  }
  
  try {
  const result = applyPatch(data, patch as Operation[], false, false)
  return result.newDocument as T
  } catch (error) {
    if (import.meta.env.DEV) console.error('Failed to apply patch:', error, 'Patch:', patch)
    return data // Return original data on error
  }
}

