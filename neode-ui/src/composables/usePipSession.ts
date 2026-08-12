import { ref, readonly } from 'vue'

/** Singleton picture-in-picture session.
 *
 *  A video handed to `adopt()` is moved into a body-level custodial host so
 *  that it survives the unmount of whatever view rendered it — a Teleport
 *  and a KeepAlive'd view both move their subtree on deactivation, and a
 *  moved element is a removed element as far as the picture-in-picture spec
 *  is concerned. Owning the element at the document level sidesteps that
 *  entirely: nothing about where the video *used* to live can touch it once
 *  it has been adopted.
 *
 *  Module singleton, no Vue lifecycle hooks — a lifecycle hook in a bare
 *  composable silently no-ops outside a component's setup(). */

const active = ref(false)
const element = ref<HTMLVideoElement | null>(null)

let host: HTMLDivElement | null = null
let leaveHandler: (() => void) | null = null

function ensureHost(): HTMLDivElement {
  if (host) return host
  const el = document.createElement('div')
  el.setAttribute('data-pip-session-host', '')
  el.setAttribute('aria-hidden', 'true')
  el.style.position = 'fixed'
  el.style.top = '0'
  el.style.left = '0'
  el.style.width = '1px'
  el.style.height = '1px'
  el.style.overflow = 'hidden'
  el.style.opacity = '0'
  el.style.pointerEvents = 'none'
  el.style.zIndex = '-1'
  document.body.appendChild(el)
  host = el
  return el
}

function release(): void {
  const video = element.value
  if (video) {
    if (leaveHandler) video.removeEventListener('leavepictureinpicture', leaveHandler)
    video.pause()
    video.removeAttribute('src')
    video.src = ''
    video.load()
    if (video.parentNode) video.parentNode.removeChild(video)
  }
  leaveHandler = null
  element.value = null
  active.value = false
}

function adopt(video: HTMLVideoElement): void {
  if (active.value && element.value && element.value !== video) {
    release()
  }
  const hostEl = ensureHost()
  hostEl.appendChild(video)
  element.value = video
  active.value = true
  leaveHandler = () => release()
  video.addEventListener('leavepictureinpicture', leaveHandler)
}

export function usePipSession() {
  return {
    active: readonly(active),
    element: readonly(element),
    adopt,
    release,
  }
}
