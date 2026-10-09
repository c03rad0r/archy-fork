import * as THREE from 'three'
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
import { createDeviceModel } from './deviceModels'

export function createDeviceScene(host: HTMLElement, id: string) {
  const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true, powerPreference: 'low-power' })
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.75))
  renderer.setClearColor(0x000000, 0)
  renderer.outputColorSpace = THREE.SRGBColorSpace
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.toneMappingExposure = .94
  renderer.domElement.setAttribute('aria-hidden', 'true')
  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(33, 1, .1, 100)
  let model = createDeviceModel(id)
  scene.add(model.pivot)
  model.pivot.rotation.set(id === 'beryl-ax' ? .27 : -.08, -.4, 0)
  const studio = new RoomEnvironment()
  const pmrem = new THREE.PMREMGenerator(renderer)
  const environment = pmrem.fromScene(studio, .04, .1, 100, { size: 128 })
  scene.environment = environment.texture; scene.environmentIntensity = 1.05
  studio.dispose(); pmrem.dispose()
  scene.add(new THREE.HemisphereLight(0xe4eeff, 0x354052, 2.25))
  for (const [color, intensity, x, y, z] of [
    [0xffffff, 1.15, 3, 5, 6], [0x91c6e8, .85, -4, 1, -3], [0xe8c5ac, .65, 4, 0, -4],
  ]) {
    const light = new THREE.DirectionalLight(color, intensity)
    light.position.set(x!, y!, z!); scene.add(light)
  }
  host.append(renderer.domElement)
  let disposed = false
  function render() { if (!disposed) renderer.render(scene, camera) }
  function resize() {
    if (disposed) return
    const { width, height } = host.getBoundingClientRect()
    if (!width || !height) return
    renderer.setSize(width, height, false)
    camera.aspect = width / height
    const halfFov = Math.atan(Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) * Math.min(1, camera.aspect))
    camera.position.set(0, 0, model.radius / Math.sin(halfFov) * 1.09)
    camera.lookAt(0, 0, 0); camera.updateProjectionMatrix(); render()
  }
  resize()
  return {
    canvas: renderer.domElement, get rotation() { return model.pivot.rotation }, render, resize,
    setModel(next: string) {
      const replacement = createDeviceModel(next)
      scene.remove(model.pivot); model.dispose(); model = replacement
      model.pivot.rotation.set(next === 'beryl-ax' ? .27 : -.08, -.4, 0)
      scene.add(model.pivot); resize()
    },
    dispose() {
      if (disposed) return
      disposed = true; model.dispose(); environment.dispose(); renderer.dispose()
      renderer.forceContextLoss(); renderer.domElement.remove()
    },
  }
}
