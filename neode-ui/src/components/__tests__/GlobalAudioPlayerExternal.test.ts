import { mount } from '@vue/test-utils'
import { ref, nextTick } from 'vue'
import { describe, expect, it, vi } from 'vitest'
const fixture = vi.hoisted(() => ({ player: {} as any }))
vi.mock('@/composables/useAudioPlayer', () => ({ useAudioPlayer: () => fixture.player }))
import GlobalAudioPlayer from '../GlobalAudioPlayer.vue'
describe('native app player controls', () => {
  it('renders app artwork and synchronized controls without a second audio source', async () => {
    fixture.player = {
      currentName: ref('Song · Artist'), externalVisible: ref(false), externalApp: ref(true), externalArtwork: ref('https://node.test:7475/media/cover.jpg'), externalArtworkOrigin: ref('https://node.test:7475'), externalShuffle: ref(true),
      playing: ref(true), loading: ref(false), error: ref(null), currentTime: ref(4), duration: ref(90), progress: ref(5), currentSrc: ref('app:session'),
      previousExternal: vi.fn(), nextExternal: vi.fn(), shuffleExternal: vi.fn(), openExternal: vi.fn(), pause: vi.fn(), play: vi.fn(), stop: vi.fn(), seek: vi.fn(),
    }
    const wrapper = mount(GlobalAudioPlayer, { global: { stubs: { teleport: true } } })
    expect(wrapper.find('img').attributes('src')).toBe('https://node.test:7475/media/cover.jpg')
    expect(wrapper.find('[aria-label="Shuffle"]').attributes('aria-pressed')).toBe('true')
    for (const [label, action] of [['Previous track', 'previousExternal'], ['Next track', 'nextExternal'], ['Shuffle', 'shuffleExternal'], ['Open playing app', 'openExternal'], ['Pause audio', 'pause']]) {
      await wrapper.find(`[aria-label="${label}"]`).trigger('click'); expect(fixture.player[action!]).toHaveBeenCalledOnce()
    }
    expect(wrapper.find('audio').exists()).toBe(false)
    fixture.player.externalShuffle.value = false; await nextTick(); expect(wrapper.find('[aria-label="Shuffle"]').attributes('aria-pressed')).toBe('false')
    fixture.player.externalVisible.value = true; await nextTick(); expect(wrapper.find('.audio-player-bar').exists()).toBe(false)
    fixture.player.externalVisible.value = false; await nextTick(); expect(wrapper.find('.audio-player-bar').exists()).toBe(true)
    wrapper.unmount()
  })
})
