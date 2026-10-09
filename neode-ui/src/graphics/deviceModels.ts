import * as THREE from 'three'
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js'
import grapheneBootMaskUrl from './assets/grapheneos-boot-mask.png'

// Original, procedural display models. Dimensions/layouts follow the existing
// device illustrations and manufacturer references listed in device-models.md.
// These are recognition aids, not mechanical CAD or revision-specific pinouts.
type Kind = 'board' | 'echo' | 'deck' | 'card' | 'case' | 'router' | 'phone'
interface DeviceSpec {
  kind: Kind; name: string; w: number; h: number; d: number; color: number
  screen?: 'oled' | 'paper'; antenna?: boolean; battery?: boolean; pins?: number
}
export const deviceSpecs: Record<string, DeviceSpec> = {
  'heltec-v3': { kind: 'board', name: 'HELTEC V3', w: 2.9, h: 1.25, d: .07, color: 0xe8e7df, screen: 'oled', pins: 18 },
  'heltec_v4': { kind: 'board', name: 'HELTEC V4', w: 2.85, h: 1.2, d: .07, color: 0x181d20, screen: 'oled', pins: 18 },
  'heltec-wsl-v3': { kind: 'board', name: 'HELTEC WSL', w: 2.85, h: 1, d: .07, color: 0x182225, pins: 18 },
  'heltec-mesh-node-t114': { kind: 'board', name: 'HELTEC T114', w: 2.65, h: 1.25, d: .08, color: 0xe6e3dc, screen: 'oled' },
  'tlora-t3s3-v1': { kind: 'board', name: 'LILYGO T3-S3', w: 2.8, h: 1.25, d: .07, color: 0x1b2024, screen: 'oled', antenna: true, pins: 16 },
  'tlora-v2-1-1_6': { kind: 'board', name: 'LILYGO T-LoRa', w: 2.8, h: 1.2, d: .07, color: 0x25272a, screen: 'oled', antenna: true, pins: 16 },
  'tbeam': { kind: 'board', name: 'LILYGO T-Beam', w: 3.7, h: 1.2, d: .08, color: 0x242a2d, screen: 'oled', antenna: true, battery: true, pins: 16 },
  'tbeam-s3-core': { kind: 'board', name: 'T-Beam S3', w: 3.35, h: 1.05, d: .08, color: 0x22272a, screen: 'oled', antenna: true, battery: true },
  'rak4631': { kind: 'board', name: 'RAK4631', w: 1.4, h: 2.8, d: .08, color: 0x202629, pins: 12 },
  'seeed-xiao-s3': { kind: 'board', name: 'XIAO ESP32S3', w: 1.25, h: 1.5, d: .08, color: 0x152c28, pins: 7 },
  'wio-tracker-wm1110': { kind: 'board', name: 'Wio WM1110', w: 1.85, h: 2.7, d: .08, color: 0xb32130, pins: 12 },
  'rpipicow': { kind: 'board', name: 'Raspberry Pi Pico', w: 3.1, h: 1.2, d: .07, color: 0x16754d, pins: 20 },
  'diy': { kind: 'board', name: 'RNode', w: 2.8, h: 1.3, d: .07, color: 0x24272b, pins: 19 },
  't-deck': { kind: 'deck', name: 'LILYGO T-Deck', w: 2, h: 3.2, d: .28, color: 0x25262b, screen: 'oled' },
  't-echo': { kind: 'echo', name: 'LILYGO', w: 1.7, h: 1.9, d: .5, color: 0xe3e4dc, screen: 'paper', antenna: true },
  't-echo_plus': { kind: 'echo', name: 'LILYGO', w: 1.9, h: 2.15, d: .65, color: 0x343839, screen: 'paper', antenna: true },
  'tracker-t1000-e': { kind: 'card', name: 'SenseCAP T1000-E', w: 1.8, h: 2.9, d: .17, color: 0xc6c9c5 },
  'rak-wismeshtap': { kind: 'case', name: 'WisMesh Tap', w: 2.2, h: 1.7, d: .52, color: 0x33363a, screen: 'oled', antenna: true },
  'station-g2': { kind: 'case', name: 'Station G2', w: 1.7, h: 2.85, d: .65, color: 0x32363b, screen: 'oled' },
  'nano-g2-ultra': { kind: 'case', name: 'Nano G2 Ultra', w: 1.7, h: 2.8, d: .5, color: 0x36383d, screen: 'oled' },
  'thinknode_m1': { kind: 'case', name: 'ThinkNode M1', w: 1.7, h: 2.3, d: .65, color: 0x252c2c, screen: 'paper', antenna: true },
  'heltec-v3-case': { kind: 'case', name: 'HELTEC V3', w: 2.8, h: 1.25, d: .48, color: 0x13857e, screen: 'oled', antenna: true },
  'heltec-mesh-node-t114-case': { kind: 'case', name: 'HELTEC T114', w: 1.25, h: 2.4, d: .5, color: 0x10a9c4, screen: 'oled', antenna: true },
  'rak4631_case': { kind: 'case', name: 'RAK WisBlock', w: 1.4, h: 2.3, d: .55, color: 0xd5dad5, screen: 'oled', antenna: true },
  'unknown-new': { kind: 'case', name: 'LoRa mesh radio', w: 1.8, h: 2.6, d: .5, color: 0x182623, screen: 'oled' },
  'beryl-ax': { kind: 'router', name: 'Beryl AX · GL-MT3000', w: 3, h: .8, d: 2.05, color: 0x171b20 },
  'pixel-graphene': { kind: 'phone', name: 'Pixel 9 Pro · GrapheneOS', w: 1.8, h: 3.82, d: .213, color: 0x08090b },
}

export function createDeviceModel(id: string, invalidate: () => void = () => {}) {
  const spec = deviceSpecs[id] ?? deviceSpecs['unknown-new']!
  const root = new THREE.Group()
  const geometries = new Map<string, THREE.BufferGeometry>()
  const materials: THREE.Material[] = []
  const textures: THREE.Texture[] = []
  let disposed = false
  const material = (color: number, roughness = .5, metalness = .1) => {
    const m = new THREE.MeshStandardMaterial({ color, roughness, metalness })
    materials.push(m); return m
  }
  const body = material(spec.color, .43)
  const black = material(0x10151a, .58)
  const dark = material(0x03070a, .72)
  const silver = material(0x9ca9af, .3, .86)
  const gold = material(0xc9a454, .3, .82)
  const white = material(0xdce2de, .66)
  const pcb = material(0x193a30, .6)
  const glass = material(0x091923, .17, .26)
  const accent = material(0x76e6bf, .35)
  accent.emissive.set(0x1b704a); accent.emissiveIntensity = .5
  const grainCanvas = document.createElement('canvas'); grainCanvas.width = grainCanvas.height = 64
  const grainContext = grainCanvas.getContext('2d')!
  const pixels = grainContext.createImageData(64, 64)
  let seed = 173
  for (let i = 0; i < pixels.data.length; i += 4) {
    seed = (seed * 1664525 + 1013904223) >>> 0
    const value = 200 + seed % 45; pixels.data.set([value, value, value, 255], i)
  }
  grainContext.putImageData(pixels, 0, 0)
  const grain = new THREE.CanvasTexture(grainCanvas)
  grain.wrapS = grain.wrapT = THREE.RepeatWrapping; grain.repeat.set(7, 7); textures.push(grain)
  body.roughnessMap = grain; body.bumpMap = grain; body.bumpScale = .0018
  const geometry = (key: string, build: () => THREE.BufferGeometry) => {
    if (!geometries.has(key)) geometries.set(key, build())
    return geometries.get(key)!
  }
  function box(w: number, h: number, d: number, x: number, y: number, z: number, m = body, radius = .025, parent = root) {
    const r = Math.min(radius, w / 3, h / 3, d / 3)
    const g = geometry(`b${w},${h},${d},${r}`, () => new RoundedBoxGeometry(w, h, d, 2, r))
    const mesh = new THREE.Mesh(g, m); mesh.position.set(x, y, z); parent.add(mesh); return mesh
  }
  function cylinder(r: number, length: number, x: number, y: number, z: number, m = silver, parent = root) {
    const g = geometry(`c${r},${length}`, () => new THREE.CylinderGeometry(r, r, length, 20))
    const mesh = new THREE.Mesh(g, m); mesh.position.set(x, y, z); parent.add(mesh); return mesh
  }
  function disk(r: number, x: number, y: number, z: number, m = silver) {
    const mesh = cylinder(r, .018, x, y, z, m); mesh.rotation.x = Math.PI / 2; return mesh
  }
  function label(text: string, w: number, h: number, x: number, y: number, z: number, back = false, color = '#dbe4e3') {
    const canvas = document.createElement('canvas'); canvas.width = 512; canvas.height = 96
    const ctx = canvas.getContext('2d')!
    ctx.fillStyle = color; ctx.font = '500 40px monospace'; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'
    ctx.fillText(text, 256, 48, 500)
    const texture = new THREE.CanvasTexture(canvas); texture.colorSpace = THREE.SRGBColorSpace; textures.push(texture)
    const m = new THREE.MeshBasicMaterial({ map: texture, transparent: true, depthWrite: false }); materials.push(m)
    const mesh = new THREE.Mesh(geometry(`p${w},${h}`, () => new THREE.PlaneGeometry(w, h)), m)
    mesh.position.set(x, y, z); if (back) mesh.rotation.y = Math.PI; root.add(mesh); return mesh
  }
  function screws(w: number, h: number, z: number) {
    for (const x of [-w / 2 + .12, w / 2 - .12]) for (const y of [-h / 2 + .12, h / 2 - .12]) {
      disk(.055, x, y, z, silver)
      box(.058, .012, .012, x, y, z + Math.sign(z) * .013, dark, .001)
    }
  }
  function antenna(x: number, y: number, length = 1.3) {
    cylinder(.085, .2, x, y + .1, 0, gold)
    cylinder(.1, .18, x, y + .24, 0, black)
    cylinder(.068, length, x, y + .3 + length / 2, 0, black)
    for (let i = 0; i < 3; i++) cylinder(.074, .02, x, y + length + .1 + i * .05, 0, body)
  }
  function screen(w: number, h: number, x: number, y: number, z: number, paper = false) {
    box(w + .12, h + .12, .065, x, y, z, black)
    box(w, h, .02, x, y, z + .038, paper ? white : glass, .015)
    const ink = paper ? '#263834' : '#8ce4d5'
    label('MESH', w * .65, h * .28, x, y + h * .14, z + .052, false, ink)
    for (let i = 0; i < 4; i++) box(.028, .045 + .035 * i, .008, x + w * .3 + i * .045, y - h * .32, z + .053, paper ? black : accent, .002)
    label('LoRa', w * .48, h * .18, x - w * .08, y - h * .2, z + .054, false, ink)
  }
  function usb(x: number, y: number, z: number, back = false) {
    box(.28, .115, .14, x, y, z, silver)
    box(.22, .063, .015, x, y, z + (back ? -.077 : .077), dark)
    box(.15, .015, .018, x, y, z + (back ? -.084 : .084), black)
  }
  const { w, h, d } = spec
  if (spec.kind === 'router') {
    // Beryl AX: two hinged paddles, rear USB-C / WAN / LAN / vertical USB-A.
    box(w, h, d, 0, 0, 0, body, .25)
    for (const z of [-1.023, 1.023]) box(2.56, .013, .008, 0, -.12, z, black, .002)
    label('GL.iNet', .62, .14, 0, .423, .08, false, '#d5e9ed').rotation.x = -Math.PI / 2
    box(.22, .032, .022, 0, -.115, d / 2 + .012, white)
    for (const side of [-1, 1]) {
      const hinge = cylinder(.16, .12, side * 1.57, -.04, .36, body); hinge.rotation.z = Math.PI / 2
      const paddle = new THREE.Group(); paddle.position.set(side * 1.62, 0, .36); paddle.rotation.z = side * -.1; root.add(paddle)
      box(.27, 1.93, .19, 0, .86, 0, body, .085, paddle)
      box(.18, 1.65, .015, 0, .88, .102, body, .005, paddle)
      for (let i = 0; i < 24; i++) box(.014, .37, .028, side * 1.497, -.035, -.85 + i * .069, dark, .003)
      for (const z of [-.72, .72]) {
        cylinder(.14, .08, side * 1.12, -.43, z, black)
        for (let i = 0; i < 14; i++) box(.038, .012, .45, side * (.35 + i * .055), -.405, z * .6, dark, .002)
      }
    }
    box(2.46, .55, .022, 0, -.015, -1.031, body, .008)
    usb(1, .04, -1.035, true)
    label('POWER', .3, .075, 1, .2, -1.057, true)
    for (const [x, text] of [[.38, '2.5G WAN'], [-.27, 'LAN']] as const) {
      box(.51, .4, .03, x, -.025, -1.055, silver)
      box(.45, .32, .03, x, -.025, -1.074, dark)
      box(.22, .06, .04, x, -.19, -1.08, dark)
      for (let i = 0; i < 8; i++) box(.019, .08, .014, x - .15 + i * .043, .062, -1.095, gold, .001)
      label(text, .53, .075, x, .24, -1.06, true)
    }
    box(.22, .4, .035, -.93, -.04, -1.06, silver)
    box(.145, .32, .02, -.93, -.04, -1.086, dark)
    box(.038, .26, .022, -.92, -.04, -1.1, material(0x287ecc))
    label('USB 3.0', .42, .075, -.93, .24, -1.06, true)
    box(.018, .085, .18, -1.508, .14, -.5, black)
    box(.02, .05, .05, -1.51, .14, -.77, dark)
    const underside = label('GL-MT3000  /  BERYL AX', 1.25, .15, 0, -.413, 0)
    underside.rotation.x = Math.PI / 2
  } else if (spec.kind === 'phone') {
    // Rounded outlines are independent of thickness: a thin RoundedBox clamps
    // the corner radius to its depth and makes a handset look square.
    function outline(width: number, height: number, radius: number) {
      const x = -width / 2, y = -height / 2, r = radius
      const shape = new THREE.Shape()
      shape.moveTo(x + r, y); shape.lineTo(x + width - r, y)
      shape.quadraticCurveTo(x + width, y, x + width, y + r)
      shape.lineTo(x + width, y + height - r)
      shape.quadraticCurveTo(x + width, y + height, x + width - r, y + height)
      shape.lineTo(x + r, y + height)
      shape.quadraticCurveTo(x, y + height, x, y + height - r)
      shape.lineTo(x, y + r); shape.quadraticCurveTo(x, y, x + r, y)
      return shape
    }
    function panel(width: number, height: number, depth: number, radius: number, z: number, m: THREE.Material, x = 0, y = 0) {
      const bevel = Math.min(.012, depth / 4)
      const g = geometry(`phone${width},${height},${depth},${radius}`, () => {
        const shape = outline(width - bevel * 2, height - bevel * 2, radius - bevel)
        const result = new THREE.ExtrudeGeometry(shape, { depth: depth - bevel * 2, bevelEnabled: true, bevelThickness: bevel, bevelSize: bevel, bevelSegments: 3, steps: 1, curveSegments: 12 })
        result.translate(0, 0, -depth / 2 + bevel)
        return result
      })
      const mesh = new THREE.Mesh(g, m); mesh.position.set(x, y, z); root.add(mesh); return mesh
    }
    const frame = material(0x101113, .32, .72)
    const edge = material(0x24262a, .27, .8)
    const rear = material(0x08090b, .32, .12)
    const lensRing = material(0x303238, .25, .78)
    const lens = material(0x080d16, .12, .3)
    const lensCoating = material(0x172a38, .16, .45)
    const seal = material(0x030405, .85)
    panel(w, h, d, .265, 0, frame)
    panel(w - .024, h - .024, .022, .255, -d / 2, edge)
    panel(w - .048, h - .048, .018, .245, -d / 2 - .013, rear)
    panel(w - .02, h - .02, .02, .255, d / 2, seal)
    panel(w - .044, h - .044, .012, .244, d / 2 + .011, material(0x050609, .2, .08))
    const oled = new THREE.MeshBasicMaterial({ color: 0x000000, toneMapped: false }); materials.push(oled)
    panel(w - .108, h - .108, .004, .216, d / 2 + .02, oled)

    // Unmodified GrapheneOS boot mask, centered at its native Pixel 9 Pro
    // screen ratio (448 / 1280). Static white phase of the OS boot animation.
    const logoW = (w - .108) * 448 / 1280, logoH = logoW * 605 / 448
    const logoGeometry = geometry('graphene-boot-logo', () => new THREE.PlaneGeometry(logoW, logoH))
    const bootCanvas = document.createElement('canvas'); bootCanvas.width = 448; bootCanvas.height = 605
    const boot = bootCanvas.getContext('2d')!
    boot.fillStyle = '#000'; boot.fillRect(0, 0, 448, 605)
    const bootTexture = new THREE.CanvasTexture(bootCanvas)
    bootTexture.colorSpace = THREE.SRGBColorSpace; textures.push(bootTexture)
    new THREE.ImageLoader().load(grapheneBootMaskUrl, image => {
      if (disposed) return
      boot.fillStyle = '#f3f3f3'; boot.fillRect(0, 0, 448, 605)
      boot.drawImage(image, 0, 0); bootTexture.needsUpdate = true; invalidate()
    })
    const logoMaterial = new THREE.MeshBasicMaterial({ map: bootTexture, toneMapped: false }); materials.push(logoMaterial)
    const logo = new THREE.Mesh(logoGeometry, logoMaterial); logo.position.z = d / 2 + .026; root.add(logo)
    disk(.031, 0, h / 2 - .145, d / 2 + .028, seal)
    disk(.017, 0, h / 2 - .145, d / 2 + .039, lens)
    panel(.32, .014, .006, .007, d / 2 + .02, seal, 0, h / 2 - .044)

    // Pixel 9 Pro's raised camera island, glass pill, three layered optics,
    // flash and temperature sensor; all face outward on the rear.
    const cameraY = h / 2 - .68
    panel(1.57, .59, .105, .285, -d / 2 - .058, frame, 0, cameraY)
    panel(1.51, .53, .018, .255, -d / 2 - .117, edge, 0, cameraY)
    panel(1.14, .402, .012, .196, -d / 2 - .13, seal, .125, cameraY)
    for (const x of [-.245, .125, .49]) {
      disk(.154, x, cameraY, -d / 2 - .142, lensRing)
      disk(.13, x, cameraY, -d / 2 - .153, dark)
      disk(.103, x, cameraY, -d / 2 - .164, lens)
      disk(.064, x, cameraY, -d / 2 - .175, lensCoating)
      disk(.039, x, cameraY, -d / 2 - .186, dark)
      disk(.018, x - .029, cameraY + .038, -d / 2 - .197, material(0x46556c, .18, .3))
    }
    disk(.069, -.582, cameraY + .11, -d / 2 - .139, material(0xc8c4ae, .5))
    disk(.033, -.582, cameraY - .11, -d / 2 - .139, lens)
    disk(.009, -.45, cameraY - .12, -d / 2 - .14, seal)
    // Black buttons, antenna seams, bottom USB-C opening and speaker holes.
    box(.038, .28, .09, w / 2 + .012, .74, 0, frame, .012)
    box(.038, .45, .09, w / 2 + .012, .19, 0, frame, .012)
    for (const side of [-1, 1]) for (const y of [-1.35, 1.36]) box(.008, .027, d - .035, side * (w / 2 + .001), y, 0, seal, .002)
    const port = panel(.245, .071, .009, .032, 0, edge)
    port.rotation.x = Math.PI / 2; port.position.set(0, -h / 2 - .002, 0)
    const portHole = panel(.213, .047, .01, .021, 0, seal)
    portHole.rotation.x = Math.PI / 2; portHole.position.set(0, -h / 2 - .008, 0)
    box(.145, .008, .012, 0, -h / 2 - .015, 0, frame, .003)
    for (let i = 0; i < 5; i++) {
      const hole = disk(.016, 0, 0, 0, seal)
      hole.rotation.x = 0; hole.position.set(.34 + i * .067, -h / 2 - .003, 0)
    }
    const mic = disk(.015, 0, 0, 0, seal); mic.rotation.x = 0; mic.position.set(-.4, -h / 2 - .003, 0)
  } else if (spec.kind === 'board') {
    box(w, h, d, 0, 0, 0, body, .06)
    // Plated through holes, copper traces, solder joints and populated reverse.
    const horizontal = w > h; const count = spec.pins ?? 10
    for (const side of [-1, 1]) for (let i = 0; i < count; i++) {
      const a = ((i + .5) / count - .5) * ((horizontal ? w : h) - .28)
      const x = horizontal ? a : side * (w / 2 - .08)
      const y = horizontal ? side * (h / 2 - .08) : a
      for (const face of [-1, 1]) {
        disk(.037, x, y, face * (d / 2 + .008), gold)
        disk(.018, x, y, face * (d / 2 + .021), dark)
      }
    }
    const heltec = id.startsWith('heltec')
    const shieldX = spec.screen ? w * .28 : w * .05
    if (!heltec || !spec.screen) {
      box(w * .25, h * .43, .11, shieldX, .05, .095, silver)
      label(spec.name.split(' ')[0]!, w * .23, .08, shieldX, .06, .158, false, '#354047')
    } else {
      box(w * .32, h * .43, .11, shieldX, .05, -.12, silver)
      label('ESP32 · LoRa', w * .29, .08, shieldX, .06, -.18, true, '#354047')
    }
    for (let i = 0; i < 12; i++) {
      const x = -w * .38 + (i % 6) * w * .14, y = -h * .28 + Math.floor(i / 6) * h * .56
      box(.095, .043, .035, x, y, .065, i % 3 ? black : silver, .003)
      box(.15, .023, .008, x, y, -.05, gold, .002)
      box(.022, .18, .008, x, y + .07, -.047, pcb, .001)
    }
    box(w * .28, h * .34, .055, -w * .15, 0, -.08, black)
    label(spec.name, w * .65, .12, 0, -h * .31, -.114, true)
    // SMD passives with metal end caps, short traces and silkscreen outlines.
    for (const face of [-1, 1]) for (let i = 0; i < 26; i++) {
      const x = -w * .4 + (i % 13) * w * .065, y = (i < 13 ? -.33 : .33) * h
      box(.065, .029, .021, x, y, face * .061, i % 4 ? black : material(0x8b7856), .002)
      for (const end of [-1, 1]) box(.013, .032, .024, x + end * .032, y, face * .063, silver, .001)
      box(.09, .007, .004, x, y + .03, face * .043, white, .001)
    }
    if (spec.screen) screen(w * (heltec ? .52 : .36), h * .51, w * (heltec ? .15 : -.15), 0, .145)
    else {
      box(w * .23, h * .28, .055, -w * .26, .03, .1, black)
      for (let i = 0; i < 6; i++) box(.028, h * .32, .015, -w * .36 + i * .04, .03, .085, silver, .001)
    }
    // USB connector faces outward at the short edge.
    const port = new THREE.Group(); root.add(port)
    box(.29, .17, .14, 0, 0, 0, silver, .02, port)
    box(.22, .11, .015, 0, 0, .078, dark, .018, port)
    if (horizontal) { port.position.set((heltec ? -1 : 1) * (w / 2 + .025), 0, .12); port.rotation.y = (heltec ? -1 : 1) * Math.PI / 2 }
    else { port.position.set(0, h / 2 + .025, .12); port.rotation.x = -Math.PI / 2 }
    disk(.055, -w * .4, .12, .076, gold)
    box(.105, .105, .055, -w * .4, -.14, .09, silver)
    box(.052, .052, .024, -w * .4, -.14, .13, black)
    if (spec.antenna) antenna(w * .27, h / 2, 1.25)
    if (spec.battery) {
      box(w * .82, .85, .5, 0, 0, -.31, black, .05)
      const cell = cylinder(.34, w * .7, 0, 0, -.52, material(0x8461a5)); cell.rotation.z = Math.PI / 2
      for (const x of [-w * .36, w * .36]) { const end = cylinder(.29, .05, x, 0, -.52); end.rotation.z = Math.PI / 2 }
    }
    if (id === 'wio-tracker-wm1110') for (const x of [-.66, .66]) for (let i = 0; i < 4; i++) box(.21, .2, .15, x, -.87 + i * .52, .1, white)
    if (id === 'rak4631') {
      box(1.08, .96, .035, 0, .45, .18, material(0x187b65))
      box(.86, .73, .04, 0, .45, .22, silver)
      label('RAK', .52, .19, 0, .56, .246, false, '#126d95')
      label('4631', .45, .1, 0, .33, .247, false, '#354047')
      for (const x of [-.43, 0, .43]) box(.21, .52, .06, x, -.74, .12, black)
    }
  } else {
    box(w, h, d, 0, 0, 0, body, .17)
    // A recessed rear panel preserves the rounded enclosure silhouette.
    box(w * .88, h * .91, .025, 0, 0, -d / 2 - .012, body, .008)
    screws(w * .94, h * .95, -d / 2 - .031)
    label(spec.name, w * .66, .11, 0, -.2, -d / 2 - .045, true)
    label('LoRa · wireless', w * .52, .07, 0, -.34, -d / 2 - .045, true)
    if (spec.kind === 'deck') {
      screen(w * .79, h * .38, 0, h * .19, d / 2 + .015)
      const keys = ['QWERTYUIOP', 'ASDFGHJKL', 'ZXCVBNM']
      keys.forEach((row, r) => [...row].forEach((key, i) => {
        const x = (i - (row.length - 1) / 2) * .165, y = -.55 - r * .19
        box(.146, .15, .07, x, y, d / 2 + .03, black, .02)
        label(key, .065, .06, x, y, d / 2 + .069)
      }))
      box(.63, .13, .065, 0, -1.12, d / 2 + .03, black)
      disk(.11, 0, -.28, d / 2 + .04, silver)
      disk(.072, 0, -.28, d / 2 + .056, white)
      label('LILYGO', .48, .09, 0, 1.47, d / 2 + .02)
      usb(0, -h / 2 + .05, -.015)
      // Exposed rear PCB on the original T-Deck development board.
      box(w * .85, h * .83, .045, 0, 0, -d / 2 - .05, pcb)
      box(.7, .65, .1, .35, .7, -d / 2 - .11, silver)
      label('ESP32-S3', .55, .08, .35, .7, -d / 2 - .165, true, '#354047')
      box(1.35, 1.1, .06, 0, -.65, -d / 2 - .09, black)
      for (let i = 0; i < 8; i++) box(.07, .12, .04, -.65 + i * .18, .04, -d / 2 - .09, gold)
    } else if (spec.kind === 'echo') {
      box(w * .49, .42, d * .85, w * .25, h / 2 + .12, 0, body, .08)
      label('LILYGO', w * .37, .13, w * .25, h / 2 + .14, d * .43, false, '#67b6d8')
      screen(w * .57, h * .57, 0, -.05, d / 2 + .017, true)
      screws(w, h, d / 2 + .024)
      antenna(w * .25, h / 2 + .33, 1.15)
      for (const y of [-.3, 0, .3]) box(.05, .14, .17, -w / 2 - .02, y, 0, white)
      usb(0, -h / 2 + .1, -d / 2, true)
      if (id === 't-echo_plus') {
        const loop = new THREE.Mesh(geometry('loop', () => new THREE.TorusGeometry(.31, .065, 10, 28, Math.PI)), silver)
        loop.position.set(-.25, h / 2 + .22, -.08); root.add(loop)
      }
    } else if (spec.kind === 'card') {
      disk(.31, 0, -.3, d / 2 + .015, gold)
      disk(.24, 0, -.3, d / 2 + .029, body)
      disk(.06, 0, .92, d / 2 + .018, dark)
      label('SenseCAP', 1.03, .17, 0, .35, d / 2 + .021, false, '#59645f')
      box(.46, .085, .025, 0, h / 2 - .12, d / 2 + .008, dark)
      for (let i = 0; i < 4; i++) disk(.045, -.22 + i * .145, -.95, -d / 2 - .047, gold)
    } else {
      if (spec.screen) screen(w * .6, Math.min(h * .3, .7), 0, h * -.04, d / 2 + .025, spec.screen === 'paper')
      label(spec.name, w * .72, .13, 0, -h * .35, d / 2 + .025)
      screws(w, h, d / 2 + .016)
      for (let i = 0; i < 7; i++) box(.045, h * .16, .013, (i - 3) * w * .08, h * .3, d / 2 + .012, dark, .004)
      usb(0, -h / 2 + .09, -d / 2, true)
      if (spec.antenna) antenna(w * .25, h / 2, 1.05)
      if (id === 'unknown-new') {
        for (const x of [-.48, .48]) disk(.12, x, -.85, d / 2 + .02, accent)
      }
    }
  }
  // Batch repeated opaque hardware. Every device owns/disposes its resources.
  function batch(parent: THREE.Object3D) {
    const groups = new Map<string, THREE.Mesh[]>()
    for (const child of [...parent.children]) {
      if (child.children.length) batch(child)
      if (!(child instanceof THREE.Mesh) || Array.isArray(child.material) || child.material.transparent) continue
      const key = child.geometry.uuid + child.material.uuid
      const list = groups.get(key) ?? []; list.push(child); groups.set(key, list)
    }
    for (const meshes of groups.values()) if (meshes.length > 1) {
      const first = meshes[0]!
      const instance = new THREE.InstancedMesh(first.geometry, first.material, meshes.length)
      meshes.forEach((mesh, i) => { mesh.updateMatrix(); instance.setMatrixAt(i, mesh.matrix); parent.remove(mesh) })
      parent.add(instance)
    }
  }
  batch(root)
  const bounds = new THREE.Box3().setFromObject(root)
  root.position.sub(bounds.getCenter(new THREE.Vector3()))
  const pivot = new THREE.Group(); pivot.add(root)
  const radius = bounds.getBoundingSphere(new THREE.Sphere()).radius
  root.traverse(object => { object.updateMatrix(); object.matrixAutoUpdate = false })
  return {
    pivot, radius, name: spec.name,
    dispose() {
      disposed = true
      root.traverse(object => { if (object instanceof THREE.InstancedMesh) object.dispose() })
      geometries.forEach(g => g.dispose()); materials.forEach(m => m.dispose()); textures.forEach(t => t.dispose())
    },
  }
}
