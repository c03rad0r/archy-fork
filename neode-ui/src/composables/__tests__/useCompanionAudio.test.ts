import { mount } from '@vue/test-utils'
import { defineComponent, nextTick, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useCompanionAudio } from '../useCompanionAudio'
import type { useAudioPlayer } from '../useAudioPlayer'

beforeEach(() => vi.useFakeTimers())
afterEach(() => { vi.useRealTimers(); delete (window as any).ArchipelagoAudio })
function fixture(install = true) {
  const messages: any[] = []
  const bridge: any = { onmessage: null, postMessage: (raw: string) => messages.push(JSON.parse(raw)) }
  if (install) (window as any).ArchipelagoAudio = bridge
  const player = {
    currentSrc: ref<string | null>('https://node.test/private?token=secret'), currentName: ref('A song'),
    playing: ref(true), currentTime: ref(12), duration: ref(120), externalArtwork: ref(''),
    externalApp: ref(true), externalShuffle: ref(false), error: ref<string | null>(null),
    pause: vi.fn(), play: vi.fn(), seek: vi.fn(), nextExternal: vi.fn(), previousExternal: vi.fn(), shuffleExternal: vi.fn(),
    stop: vi.fn(() => { player.currentSrc.value = null; player.playing.value = false }),
  }
  let audio!: ReturnType<typeof useCompanionAudio>
  const wrapper = mount(defineComponent({ setup() { audio = useCompanionAudio(player as unknown as ReturnType<typeof useAudioPlayer>); return () => null } }))
  return { messages, bridge, player, wrapper, audio,
    command: (command: string, extra = {}) => bridge.onmessage({ data: JSON.stringify({ version: 1, type: 'command', session: messages[0]?.session, command, ...extra }) }) }

}

describe('companion native audio control of the existing player', () => {
  it('sends bounded metadata and queue controls without stream URLs or credentials', async () => {
    const f = fixture(); await nextTick()
    expect(f.messages[0]).toMatchObject({ action: 'state', title: 'A song', playing: true, position: 12, duration: 120, next: true, previous: true, shuffle: true })
    expect(JSON.stringify(f.messages)).not.toContain('secret')
    expect(JSON.stringify(f.messages)).not.toContain('https:')
    f.player.currentTime.value = 13; await nextTick(); f.player.currentTime.value = 14; await nextTick()
    expect(f.messages).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1000)
    expect(f.messages).toHaveLength(2); expect(f.messages[1].position).toBe(14)
    f.wrapper.unmount(); expect(f.messages[f.messages.length - 1].action).toBe('release')
  })
  it('starts unique valid sessions on LAN HTTP without crypto.randomUUID', async () => {
    const descriptor = Object.getOwnPropertyDescriptor(crypto, 'randomUUID')
    Object.defineProperty(crypto, 'randomUUID', { configurable: true, value: undefined })
    const f = fixture()
    try {
      await nextTick()
      const first = f.messages[0].session
      expect(first).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
      f.player.currentSrc.value = 'app:second'; await nextTick()
      expect(f.messages[2].session).not.toBe(first)
    } finally {
      f.wrapper.unmount()
      if (descriptor) Object.defineProperty(crypto, 'randomUUID', descriptor)
      else Reflect.deleteProperty(crypto, 'randomUUID')
    }
  })
  it('operates the same queue and ignores controls from a stale session', async () => {
    const f = fixture(); await nextTick()
    f.command('pause', { session: 'foreign' }); expect(f.player.pause).not.toHaveBeenCalled()
    f.command('pause'); f.command('next'); f.command('previous'); f.command('shuffle'); f.command('seek', { position: 900 })
    expect(f.player.pause).toHaveBeenCalledOnce(); expect(f.player.nextExternal).toHaveBeenCalledOnce()
    expect(f.player.previousExternal).toHaveBeenCalledOnce(); expect(f.player.shuffleExternal).toHaveBeenCalledOnce()
    expect(f.player.seek).toHaveBeenCalledWith(120)
    f.command('seek', { position: '5' }); expect(f.player.seek).toHaveBeenCalledTimes(1)
    f.command('stop'); await nextTick(); expect(f.player.stop).toHaveBeenCalledOnce()
    expect(f.messages[f.messages.length - 1].action).toBe('release'); f.wrapper.unmount()
  })
  it('replaces native session when changing source and releases on logout/player stop', async () => {
    const f = fixture(); await nextTick(); const first = f.messages[0].session
    f.player.currentSrc.value = 'app:new'; await nextTick()
    expect(f.messages[1]).toMatchObject({ action: 'release', session: first })
    expect(f.messages[2].session).not.toBe(first)
    f.command('pause', { session: first }); expect(f.player.pause).not.toHaveBeenCalled()
    f.player.currentSrc.value = null; await nextTick()
    expect(f.messages[f.messages.length - 1].action).toBe('release'); f.wrapper.unmount()
  })
  it('connects a late native bridge and answers synchronization while backgrounded', async () => {
    const f = fixture(false); await nextTick(); expect(f.messages).toHaveLength(0)
    ;(window as any).ArchipelagoAudio = f.bridge
    document.dispatchEvent(new Event('visibilitychange'))
    expect(f.messages).toHaveLength(1)
    f.command('sync'); expect(f.messages).toHaveLength(2)
    f.wrapper.unmount(); const count = f.messages.length
    await vi.advanceTimersByTimeAsync(30000); expect(f.messages).toHaveLength(count)
  })
  it('does not revive paused sessions and reports native start failures without leaking details', async () => {
    const f = fixture(); await nextTick()
    f.bridge.onmessage({ data: JSON.stringify({ version: 1, session: f.messages[0].session, type: 'error', error: 'sensitive details' }) })
    expect(f.player.pause).toHaveBeenCalledOnce(); expect(f.player.error.value).toContain('Background playback')
    expect(f.player.error.value).not.toContain('sensitive')
    f.player.playing.value = false; await nextTick(); const count = f.messages.length
    await vi.advanceTimersByTimeAsync(10000); expect(f.messages).toHaveLength(count)
    f.wrapper.unmount()
  })
})
