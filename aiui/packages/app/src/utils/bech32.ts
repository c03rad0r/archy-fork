const CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'
const CHARSET_MAP: Record<string, number> = {}
for (let i = 0; i < CHARSET.length; i++) CHARSET_MAP[CHARSET[i]] = i

function polymod(values: number[]): number {
  const GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
  let chk = 1
  for (const v of values) {
    const b = chk >> 25
    chk = ((chk & 0x1ffffff) << 5) ^ v
    for (let i = 0; i < 5; i++) {
      if ((b >> i) & 1) chk ^= GEN[i]
    }
  }
  return chk
}

function hrpExpand(hrp: string): number[] {
  const ret: number[] = []
  for (let i = 0; i < hrp.length; i++) ret.push(hrp.charCodeAt(i) >> 5)
  ret.push(0)
  for (let i = 0; i < hrp.length; i++) ret.push(hrp.charCodeAt(i) & 31)
  return ret
}

function convertBits(data: number[], fromBits: number, toBits: number, pad: boolean): number[] | null {
  let acc = 0
  let bits = 0
  const ret: number[] = []
  const maxv = (1 << toBits) - 1

  for (const value of data) {
    if (value < 0 || value >> fromBits) return null
    acc = (acc << fromBits) | value
    bits += fromBits
    while (bits >= toBits) {
      bits -= toBits
      ret.push((acc >> bits) & maxv)
    }
  }

  if (pad) {
    if (bits > 0) ret.push((acc << (toBits - bits)) & maxv)
  } else if (bits >= fromBits || ((acc << (toBits - bits)) & maxv)) {
    return null
  }

  return ret
}

export interface Bech32Decoded {
  hrp: string
  data: Uint8Array
}

export function bech32Decode(str: string): Bech32Decoded | null {
  const lower = str.toLowerCase()
  const sepPos = lower.lastIndexOf('1')
  if (sepPos < 1 || sepPos + 7 > lower.length || lower.length > 90) return null

  const hrp = lower.slice(0, sepPos)
  const dataChars = lower.slice(sepPos + 1)

  const values: number[] = []
  for (const c of dataChars) {
    const v = CHARSET_MAP[c]
    if (v === undefined) return null
    values.push(v)
  }

  if (polymod([...hrpExpand(hrp), ...values]) !== 1) return null

  const data5bit = values.slice(0, -6)
  const bytes = convertBits(data5bit, 5, 8, false)
  if (!bytes) return null

  return { hrp, data: new Uint8Array(bytes) }
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
}

export function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2)
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.slice(i, i + 2), 16)
  }
  return bytes
}

function createChecksum(hrp: string, data: number[]): number[] {
  const values = [...hrpExpand(hrp), ...data, 0, 0, 0, 0, 0, 0]
  const mod = polymod(values) ^ 1
  const ret: number[] = []
  for (let i = 0; i < 6; i++) ret.push((mod >> (5 * (5 - i))) & 31)
  return ret
}

export function bech32Encode(hrp: string, data: Uint8Array): string {
  const fiveBit = convertBits(Array.from(data), 8, 5, true)
  if (!fiveBit) throw new Error('Failed to convert bits')
  const checksum = createChecksum(hrp, fiveBit)
  return hrp + '1' + [...fiveBit, ...checksum].map(d => CHARSET[d]).join('')
}

/** Encode a hex public key as npub1... */
export function encodeNpub(hexPubkey: string): string {
  return bech32Encode('npub', hexToBytes(hexPubkey))
}

/** Decode an npub1... string to a hex public key */
export function decodeNpub(npubStr: string): string {
  const decoded = bech32Decode(npubStr)
  if (!decoded || decoded.hrp !== 'npub' || decoded.data.length !== 32) {
    throw new Error('Invalid npub')
  }
  return bytesToHex(decoded.data)
}

export interface NIP19Decoded {
  type: 'npub' | 'note' | 'nevent' | 'nprofile' | 'unknown'
  hex: string
  relays?: string[]
}

export function decodeNIP19(bech32Str: string): NIP19Decoded | null {
  const decoded = bech32Decode(bech32Str)
  if (!decoded) return null

  const { hrp, data } = decoded

  if (hrp === 'npub' && data.length === 32) {
    return { type: 'npub', hex: bytesToHex(data) }
  }

  if (hrp === 'note' && data.length === 32) {
    return { type: 'note', hex: bytesToHex(data) }
  }

  // TLV decoding for nevent/nprofile
  if (hrp === 'nevent' || hrp === 'nprofile') {
    let hex = ''
    const relays: string[] = []
    let i = 0
    while (i < data.length) {
      const tag = data[i]
      const len = data[i + 1]
      if (len === undefined) break
      const value = data.slice(i + 2, i + 2 + len)
      if (tag === 0) hex = bytesToHex(value)
      if (tag === 1) relays.push(new TextDecoder().decode(value))
      i += 2 + len
    }
    return {
      type: hrp === 'nevent' ? 'nevent' : 'nprofile',
      hex,
      relays: relays.length > 0 ? relays : undefined,
    }
  }

  return { type: 'unknown', hex: bytesToHex(data) }
}
