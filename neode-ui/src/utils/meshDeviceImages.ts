/**
 * Best-effort mapping from a detected serial port's USB identity to an
 * original procedural Three.js model.
 *
 * Native-USB boards (T-Deck, RAK4631, T1000-E…) report their name in the USB
 * product string, so those match precisely. UART-bridge boards only identify
 * the bridge chip (CP2102/CH340), so vid:pid picks the most common board for
 * that chip and the label stays honest ("LoRa radio").
 */

export interface DetectedDeviceInfo {
  path: string
  vid?: string | null
  pid?: string | null
  product?: string | null
  manufacturer?: string | null
}

export interface MeshDeviceImage {
  /** Procedural model key in graphics/deviceModels.ts. */
  model: string
  /** Human name shown in the modal */
  label: string
  /** True when the match is exact (product string), false for chip-level guesses */
  exact: boolean
}

/** product-string keyword → image (checked in order, case-insensitive) */
const PRODUCT_MATCHES: Array<{ match: RegExp; model: string; label: string }> = [
  { match: /t-?deck/i, model: 't-deck', label: 'LILYGO T-Deck' },
  { match: /t-?echo\s*plus/i, model: 't-echo_plus', label: 'LILYGO T-Echo Plus' },
  { match: /t-?echo/i, model: 't-echo', label: 'LILYGO T-Echo' },
  { match: /t-?beam\s*s3/i, model: 'tbeam-s3-core', label: 'LILYGO T-Beam S3' },
  { match: /t-?beam/i, model: 'tbeam', label: 'LILYGO T-Beam' },
  { match: /t3.?s3/i, model: 'tlora-t3s3-v1', label: 'LILYGO T3-S3' },
  { match: /t-?lora|tlora/i, model: 'tlora-v2-1-1_6', label: 'LILYGO T-LoRa' },
  { match: /rak.?4631|wisblock/i, model: 'rak4631', label: 'RAK WisBlock 4631' },
  { match: /wismesh|wistap/i, model: 'rak-wismeshtap', label: 'RAK WisMesh Tap' },
  { match: /t1000|tracker.?t1000/i, model: 'tracker-t1000-e', label: 'Seeed Card Tracker T1000-E' },
  { match: /xiao/i, model: 'seeed-xiao-s3', label: 'Seeed XIAO S3' },
  { match: /wio.?tracker|wm1110/i, model: 'wio-tracker-wm1110', label: 'Seeed Wio Tracker' },
  { match: /station\s*g2/i, model: 'station-g2', label: 'B&Q Station G2' },
  { match: /nano\s*g2/i, model: 'nano-g2-ultra', label: 'B&Q Nano G2 Ultra' },
  { match: /t114/i, model: 'heltec-mesh-node-t114', label: 'Heltec Mesh Node T114' },
  { match: /wireless\s*stick|wsl/i, model: 'heltec-wsl-v3', label: 'Heltec Wireless Stick Lite V3' },
  { match: /heltec.*v4/i, model: 'heltec_v4', label: 'Heltec V4' },
  { match: /heltec/i, model: 'heltec-v3', label: 'Heltec LoRa 32 V3' },
  { match: /thinknode/i, model: 'thinknode_m1', label: 'Elecrow ThinkNode' },
  { match: /pico/i, model: 'rpipicow', label: 'Raspberry Pi Pico' },
  { match: /rnode/i, model: 'diy', label: 'RNode' },
]

/** vid:pid → most-likely board (bridge chips = chip-level guess) */
const VIDPID_MATCHES: Record<string, { model: string; label: string; exact: boolean }> = {
  // Silicon Labs CP210x — Heltec V3 family ships this bridge
  '10c4:ea60': { model: 'heltec-v3', label: 'LoRa radio (CP2102 serial)', exact: false },
  // WCH CH340 — most T-Beam / T-LoRa boards
  '1a86:7523': { model: 'tbeam', label: 'LoRa radio (CH340 serial)', exact: false },
  '1a86:55d4': { model: 'tbeam', label: 'LoRa radio (CH9102 serial)', exact: false },
  // Espressif native USB (S3 boards: T3-S3, T-Deck without product str)
  '303a:1001': { model: 'tlora-t3s3-v1', label: 'ESP32-S3 LoRa board', exact: false },
  // RAK4631 nRF52 native USB
  '239a:8029': { model: 'rak4631', label: 'RAK WisBlock 4631', exact: true },
  // Raspberry Pi (Pico W)
  '2e8a:0005': { model: 'rpipicow', label: 'Raspberry Pi Pico', exact: false },
}

const FALLBACK: MeshDeviceImage = {
  model: 'unknown-new',
  label: 'LoRa mesh radio',
  exact: false,
}

export function resolveMeshDeviceImage(info: DetectedDeviceInfo | undefined): MeshDeviceImage {
  if (!info) return FALLBACK
  const product = `${info.manufacturer ?? ''} ${info.product ?? ''}`.trim()
  if (product) {
    for (const m of PRODUCT_MATCHES) {
      if (m.match.test(product)) return { model: m.model, label: m.label, exact: true }
    }
  }
  if (info.vid && info.pid) {
    const hit = VIDPID_MATCHES[`${info.vid}:${info.pid}`.toLowerCase()]
    if (hit) return hit
  }
  return FALLBACK
}
