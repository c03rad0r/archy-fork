/**
 * AES-256-GCM encryption utilities using Web Crypto API.
 * PBKDF2 key derivation with 100K+ iterations.
 */

const PBKDF2_ITERATIONS = 100_000
const SALT_LENGTH = 16
const IV_LENGTH = 12

export async function generateSalt(): Promise<Uint8Array> {
  return crypto.getRandomValues(new Uint8Array(SALT_LENGTH))
}

export async function deriveKey(password: string, salt: Uint8Array): Promise<CryptoKey> {
  const enc = new TextEncoder()
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    enc.encode(password),
    'PBKDF2',
    false,
    ['deriveKey'],
  )

  return crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt,
      iterations: PBKDF2_ITERATIONS,
      hash: 'SHA-256',
    },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  )
}

export interface EncryptedPayload {
  ciphertext: ArrayBuffer
  iv: Uint8Array
}

export async function encrypt(data: string, key: CryptoKey): Promise<EncryptedPayload> {
  const enc = new TextEncoder()
  const iv = crypto.getRandomValues(new Uint8Array(IV_LENGTH))
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    enc.encode(data),
  )
  return { ciphertext, iv }
}

export async function decrypt(
  ciphertext: ArrayBuffer,
  iv: Uint8Array,
  key: CryptoKey,
): Promise<string> {
  const plaintext = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv },
    key,
    ciphertext,
  )
  return new TextDecoder().decode(plaintext)
}

/**
 * Convenience: encrypt a string and return a storable format.
 * Returns base64-encoded JSON with iv + ciphertext.
 */
export async function encryptToString(data: string, key: CryptoKey): Promise<string> {
  const { ciphertext, iv } = await encrypt(data, key)
  const combined = new Uint8Array(iv.length + ciphertext.byteLength)
  combined.set(iv)
  combined.set(new Uint8Array(ciphertext), iv.length)
  return bufferToBase64(combined)
}

/**
 * Convenience: decrypt a base64-encoded encrypted string.
 */
export async function decryptFromString(encoded: string, key: CryptoKey): Promise<string> {
  const combined = base64ToBuffer(encoded)
  const iv = combined.slice(0, IV_LENGTH)
  const ciphertext = combined.slice(IV_LENGTH)
  return decrypt(ciphertext.buffer, iv, key)
}

function bufferToBase64(buffer: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < buffer.length; i++) {
    binary += String.fromCharCode(buffer[i])
  }
  return btoa(binary)
}

function base64ToBuffer(base64: string): Uint8Array {
  const binary = atob(base64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i)
  }
  return bytes
}

/**
 * Check if encryption should be enabled.
 * Disabled in dev mode with VITE_DISABLE_CRYPTO=true, or when
 * Web Crypto API is unavailable (HTTP on non-localhost origins).
 */
export function isCryptoEnabled(): boolean {
  try {
    if (import.meta.env.VITE_DISABLE_CRYPTO === 'true') return false
    // crypto.subtle is only available in secure contexts (HTTPS or localhost)
    if (typeof globalThis.crypto?.subtle === 'undefined') return false
    return true
  } catch {
    return false
  }
}

/**
 * Session key management — held in memory only.
 */
let sessionKey: CryptoKey | null = null
let sessionSalt: Uint8Array | null = null

export function setSessionKey(key: CryptoKey, salt: Uint8Array): void {
  sessionKey = key
  sessionSalt = salt
}

export function getSessionKey(): CryptoKey | null {
  return sessionKey
}

export function getSessionSalt(): Uint8Array | null {
  return sessionSalt
}

export function clearSessionKey(): void {
  sessionKey = null
  sessionSalt = null
}

export function hasSessionKey(): boolean {
  return sessionKey !== null
}
