/**
 * Best-effort mapping from a detected serial port's USB identity to a board
 * image (SVGs vendored from the Meshtastic web-flasher, GPL-3.0 —
 * github.com/meshtastic/web-flasher, public/img/devices/).
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
  /** Path under public/ */
  image: string
  /** Human name shown in the modal */
  label: string
  /** True when the match is exact (product string), false for chip-level guesses */
  exact: boolean
}

const IMG = '/assets/img/mesh-devices'

/** product-string keyword → image (checked in order, case-insensitive) */
const PRODUCT_MATCHES: Array<{ match: RegExp; image: string; label: string }> = [
  { match: /t-?deck/i, image: `${IMG}/t-deck.svg`, label: 'LILYGO T-Deck' },
  { match: /t-?echo\s*plus/i, image: `${IMG}/t-echo_plus.svg`, label: 'LILYGO T-Echo Plus' },
  { match: /t-?echo/i, image: `${IMG}/t-echo.svg`, label: 'LILYGO T-Echo' },
  { match: /t-?beam\s*s3/i, image: `${IMG}/tbeam-s3-core.svg`, label: 'LILYGO T-Beam S3' },
  { match: /t-?beam/i, image: `${IMG}/tbeam.svg`, label: 'LILYGO T-Beam' },
  { match: /t3.?s3/i, image: `${IMG}/tlora-t3s3-v1.svg`, label: 'LILYGO T3-S3' },
  { match: /t-?lora|tlora/i, image: `${IMG}/tlora-v2-1-1_6.svg`, label: 'LILYGO T-LoRa' },
  { match: /rak.?4631|wisblock/i, image: `${IMG}/rak4631.svg`, label: 'RAK WisBlock 4631' },
  { match: /wismesh|wistap/i, image: `${IMG}/rak-wismeshtap.svg`, label: 'RAK WisMesh Tap' },
  { match: /t1000|tracker.?t1000/i, image: `${IMG}/tracker-t1000-e.svg`, label: 'Seeed Card Tracker T1000-E' },
  { match: /xiao/i, image: `${IMG}/seeed-xiao-s3.svg`, label: 'Seeed XIAO S3' },
  { match: /wio.?tracker|wm1110/i, image: `${IMG}/wio-tracker-wm1110.svg`, label: 'Seeed Wio Tracker' },
  { match: /station\s*g2/i, image: `${IMG}/station-g2.svg`, label: 'B&Q Station G2' },
  { match: /nano\s*g2/i, image: `${IMG}/nano-g2-ultra.svg`, label: 'B&Q Nano G2 Ultra' },
  { match: /t114/i, image: `${IMG}/heltec-mesh-node-t114.svg`, label: 'Heltec Mesh Node T114' },
  { match: /wireless\s*stick|wsl/i, image: `${IMG}/heltec-wsl-v3.svg`, label: 'Heltec Wireless Stick Lite V3' },
  { match: /heltec.*v4/i, image: `${IMG}/heltec_v4.svg`, label: 'Heltec V4' },
  { match: /heltec/i, image: `${IMG}/heltec-v3.svg`, label: 'Heltec LoRa 32 V3' },
  { match: /thinknode/i, image: `${IMG}/thinknode_m1.svg`, label: 'Elecrow ThinkNode' },
  { match: /pico/i, image: `${IMG}/rpipicow.svg`, label: 'Raspberry Pi Pico' },
  { match: /rnode/i, image: `${IMG}/diy.svg`, label: 'RNode' },
]

/** vid:pid → most-likely board (bridge chips = chip-level guess) */
const VIDPID_MATCHES: Record<string, { image: string; label: string; exact: boolean }> = {
  // Silicon Labs CP210x — Heltec V3 family ships this bridge
  '10c4:ea60': { image: `${IMG}/heltec-v3.svg`, label: 'LoRa radio (CP2102 serial)', exact: false },
  // WCH CH340 — most T-Beam / T-LoRa boards
  '1a86:7523': { image: `${IMG}/tbeam.svg`, label: 'LoRa radio (CH340 serial)', exact: false },
  '1a86:55d4': { image: `${IMG}/tbeam.svg`, label: 'LoRa radio (CH9102 serial)', exact: false },
  // Espressif native USB (S3 boards: T3-S3, T-Deck without product str)
  '303a:1001': { image: `${IMG}/tlora-t3s3-v1.svg`, label: 'ESP32-S3 LoRa board', exact: false },
  // RAK4631 nRF52 native USB
  '239a:8029': { image: `${IMG}/rak4631.svg`, label: 'RAK WisBlock 4631', exact: true },
  // Raspberry Pi (Pico W)
  '2e8a:0005': { image: `${IMG}/rpipicow.svg`, label: 'Raspberry Pi Pico', exact: false },
}

const FALLBACK: MeshDeviceImage = {
  image: `${IMG}/unknown-new.svg`,
  label: 'LoRa mesh radio',
  exact: false,
}

export function resolveMeshDeviceImage(info: DetectedDeviceInfo | undefined): MeshDeviceImage {
  if (!info) return FALLBACK
  const product = `${info.manufacturer ?? ''} ${info.product ?? ''}`.trim()
  if (product) {
    for (const m of PRODUCT_MATCHES) {
      if (m.match.test(product)) return { image: m.image, label: m.label, exact: true }
    }
  }
  if (info.vid && info.pid) {
    const hit = VIDPID_MATCHES[`${info.vid}:${info.pid}`.toLowerCase()]
    if (hit) return hit
  }
  return FALLBACK
}
