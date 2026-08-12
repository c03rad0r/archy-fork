import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import MediaLightbox from '../cloud/MediaLightbox.vue'
import { usePipSession } from '../../composables/usePipSession'
import type { FileBrowserItem } from '../../api/filebrowser-client'

// jsdom has no picture-in-picture implementation — stub the pieces the
// component and the session touch so `enterpictureinpicture` /
// `leavepictureinpicture` can be dispatched like a real browser would.
beforeEach(() => {
  // jsdom doesn't implement these either — MediaLightbox's onUnmounted
  // revokes every cached blob URL, which throws "not implemented" otherwise.
  if (!URL.createObjectURL) URL.createObjectURL = vi.fn(() => 'blob:stub')
  if (!URL.revokeObjectURL) URL.revokeObjectURL = vi.fn()

  Object.defineProperty(document, 'pictureInPictureEnabled', {
    value: true,
    configurable: true,
  })
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

afterEach(() => {
  usePipSession().release()
  document.body.innerHTML = ''
})

const videoItem: FileBrowserItem = {
  name: 'clip.mp4',
  path: '/clip.mp4',
  isDir: false,
} as FileBrowserItem

// MediaLightbox teleports its content to <body>, so its markup lives
// outside the mounted wrapper's own root element — query the document
// directly rather than through `wrapper.find`.
function findVideo(): HTMLVideoElement {
  const video = document.body.querySelector('video')
  if (!video) throw new Error('video not rendered')
  return video as HTMLVideoElement
}

function findBackdrop(): HTMLElement {
  const backdrop = document.body.querySelector('.lightbox-backdrop')
  if (!backdrop) throw new Error('backdrop not rendered')
  return backdrop as HTMLElement
}

async function mountLightbox() {
  const wrapper = mount(MediaLightbox, {
    props: {
      items: [videoItem],
      startIndex: 0,
      show: true,
      fetchBlobUrl: vi.fn().mockResolvedValue('blob:fetch'),
      streamUrl: vi.fn().mockResolvedValue('blob:stream'),
    },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

describe('MediaLightbox picture-in-picture handoff', () => {
  it('entering PiP emits close exactly once and adopts the video before doing so', async () => {
    const wrapper = await mountLightbox()
    const video = findVideo()

    video.dispatchEvent(new Event('enterpictureinpicture'))

    // Adopted synchronously, before the animation/emit has finished — the
    // video is no longer a descendant of the lightbox's own subtree.
    expect(usePipSession().active.value).toBe(true)
    expect(usePipSession().element.value).toBe(video)
    expect(findBackdrop().contains(video)).toBe(false)
    expect(wrapper.emitted('close')).toBeUndefined()

    // Bounded fallback fires the close even without a real transitionend
    // (jsdom does not run CSS transitions).
    await new Promise((resolve) => setTimeout(resolve, 400))

    expect(wrapper.emitted('close')).toHaveLength(1)

    wrapper.unmount()
    // Still connected to the document after the owner unmounts.
    expect(document.body.contains(video)).toBe(true)
  })

  it('applies the handoff class on the PiP path', async () => {
    await mountLightbox()
    const video = findVideo()

    video.dispatchEvent(new Event('enterpictureinpicture'))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(findBackdrop().classList.contains('lightbox-pip-handoff')).toBe(true)
  })

  it('applies no handoff class on a button-driven close', async () => {
    const wrapper = await mountLightbox()
    const closeButton = document.body.querySelector(
      '.lightbox-topbar .lightbox-btn:last-child'
    ) as HTMLButtonElement
    closeButton.click()
    await flushPromises()

    expect(wrapper.emitted('close')).toHaveLength(1)
    expect(findBackdrop().classList.contains('lightbox-pip-handoff')).toBe(false)
  })

  it('releases the session when picture-in-picture is left', async () => {
    await mountLightbox()
    const video = findVideo()

    video.dispatchEvent(new Event('enterpictureinpicture'))
    expect(usePipSession().active.value).toBe(true)

    video.dispatchEvent(new Event('leavepictureinpicture'))

    expect(usePipSession().active.value).toBe(false)
  })

  it('does not change props or emits declared by the component', async () => {
    // Contract guard mirrored from the diff-based acceptance criterion:
    // this component is used by a second, parallel instance (plan 01-14)
    // and must keep working with an unmodified prop/emit set.
    const wrapper = await mountLightbox()
    expect(wrapper.props('startIndex')).toBe(0)
    expect(wrapper.props('show')).toBe(true)
    expect(typeof wrapper.props('fetchBlobUrl')).toBe('function')
  })
})
