import { describe, it, expect, afterEach, beforeEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import { usePipSession } from '../usePipSession'
import { isPipSupported } from '../../utils/pip'

// jsdom has no picture-in-picture implementation at all, so these three
// document/element members don't exist until stubbed — matching the plan's
// note that `pipSupported` (module-level) can't be restubbed after import,
// which is exactly why `isPipSupported()` exists as a call-time check.
function definePipStub(enabled: boolean) {
  Object.defineProperty(document, 'pictureInPictureEnabled', {
    value: enabled,
    configurable: true,
  })
}

beforeEach(() => {
  definePipStub(true)
  if (!('pictureInPictureElement' in document)) {
    Object.defineProperty(document, 'pictureInPictureElement', {
      value: null,
      configurable: true,
    })
  }
  if (!HTMLVideoElement.prototype.requestPictureInPicture) {
    HTMLVideoElement.prototype.requestPictureInPicture = async function () {
      return null as unknown as PictureInPictureWindow
    }
  }
  if (!document.exitPictureInPicture) {
    document.exitPictureInPicture = async () => {}
  }
})

function hostEl(): HTMLElement | null {
  return document.querySelector('[data-pip-session-host]')
}

describe('usePipSession', () => {
  afterEach(() => {
    usePipSession().release()
  })

  it('adopts a video into a body-level host that survives the owner unmounting', () => {
    const Owner = defineComponent({
      setup() {
        return () => h('div', [h('video')])
      },
    })
    const wrapper = mount(Owner, { attachTo: document.body })
    const video = wrapper.find('video').element as HTMLVideoElement

    const session = usePipSession()
    session.adopt(video)

    expect(session.active.value).toBe(true)
    expect(session.element.value).toBe(video)
    expect(document.body.contains(video)).toBe(true)

    wrapper.unmount()

    // The owning component is gone; the video must still be connected.
    expect(document.body.contains(video)).toBe(true)
    expect(hostEl()?.contains(video)).toBe(true)
  })

  it('release() removes the adopted element and leaves the host empty', () => {
    const video = document.createElement('video')
    const session = usePipSession()
    session.adopt(video)
    expect(hostEl()?.contains(video)).toBe(true)

    session.release()

    expect(session.active.value).toBe(false)
    expect(session.element.value).toBeNull()
    expect(hostEl()?.contains(video)).toBe(false)
    expect(hostEl()?.childElementCount).toBe(0)
  })

  it('creates exactly one host node no matter how many times the composable is called', () => {
    const video = document.createElement('video')
    usePipSession().adopt(video)
    usePipSession()
    usePipSession()
    expect(document.querySelectorAll('[data-pip-session-host]').length).toBe(1)
  })

  it('a leavepictureinpicture event on the adopted element releases the session', () => {
    const video = document.createElement('video')
    const session = usePipSession()
    session.adopt(video)

    video.dispatchEvent(new Event('leavepictureinpicture'))

    expect(session.active.value).toBe(false)
    expect(session.element.value).toBeNull()
  })

  it('adopting a second element while one is active releases the first rather than leaking it', () => {
    const first = document.createElement('video')
    const second = document.createElement('video')
    const session = usePipSession()

    session.adopt(first)
    session.adopt(second)

    expect(session.element.value).toBe(second)
    expect(hostEl()?.contains(first)).toBe(false)
    expect(hostEl()?.contains(second)).toBe(true)
  })
})

describe('isPipSupported', () => {
  it('reads document state at call time, not at import time', () => {
    definePipStub(false)
    expect(isPipSupported()).toBe(false)

    definePipStub(true)
    expect(isPipSupported()).toBe(true)
  })
})
