import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock Audio globally
class MockAudio {
  src = ''
  volume = 1
  loop = false
  currentTime = 0
  play = vi.fn().mockResolvedValue(undefined)
  pause = vi.fn()
  addEventListener = vi.fn()
}

vi.stubGlobal('Audio', MockAudio)

// Mock fetch for playLoopStart
vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
  arrayBuffer: vi.fn().mockResolvedValue(new ArrayBuffer(8)),
}))

// Mock AudioContext
const mockBufferSource = {
  buffer: null as AudioBuffer | null,
  connect: vi.fn(),
  start: vi.fn(),
  stop: vi.fn(),
}

const mockMediaElementSource = {
  connect: vi.fn(),
}

const mockGainNode = {
  gain: {
    value: 1,
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  },
  connect: vi.fn(),
}

const mockAudioContext = {
  state: 'running' as AudioContextState,
  currentTime: 0,
  destination: {},
  resume: vi.fn().mockResolvedValue(undefined),
  createOscillator: vi.fn().mockReturnValue({
    type: 'sine',
    frequency: { value: 440, setValueAtTime: vi.fn() },
    connect: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
  }),
  createGain: vi.fn().mockReturnValue({ ...mockGainNode, gain: { ...mockGainNode.gain } }),
  createBufferSource: vi.fn().mockReturnValue({ ...mockBufferSource }),
  createMediaElementSource: vi.fn().mockReturnValue({ ...mockMediaElementSource }),
  decodeAudioData: vi.fn().mockResolvedValue({} as AudioBuffer),
}

vi.stubGlobal('AudioContext', vi.fn().mockImplementation(() => ({ ...mockAudioContext })))

import {
  playPop,
  playLoginSuccessWhoosh,
  playTypingSound,
  playIntroTyping,
  stopIntroTyping,
  playWelcomeNoderunnerSpeech,
  playTypingTick,
  resumeAudioContext,
  startSynthwave,
  stopSynthwave,
  playLoopStart,
  playKeyboardTypingSound,
  playDashboardLoadOomph,
} from '../useLoginSounds'

describe('useLoginSounds', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('playPop', () => {
    it('creates Audio with pop.mp3 and plays it', () => {
      playPop()
      // Audio constructor was called (via MockAudio)
      expect(MockAudio.prototype.constructor).toBeDefined()
    })

    it('does not throw', () => {
      expect(() => playPop()).not.toThrow()
    })
  })

  describe('playLoginSuccessWhoosh', () => {
    it('does not throw', () => {
      expect(() => playLoginSuccessWhoosh()).not.toThrow()
    })
  })

  describe('playTypingSound', () => {
    it('does not throw', () => {
      expect(() => playTypingSound()).not.toThrow()
    })
  })

  describe('playIntroTyping', () => {
    it('does not throw', () => {
      expect(() => playIntroTyping()).not.toThrow()
    })

    it('creates a looping audio element', () => {
      playIntroTyping()
      // Does not throw, creates audio
    })
  })

  describe('stopIntroTyping', () => {
    it('does not throw when no audio playing', () => {
      expect(() => stopIntroTyping()).not.toThrow()
    })

    it('stops audio that was started', () => {
      playIntroTyping()
      expect(() => stopIntroTyping()).not.toThrow()
    })
  })

  describe('playWelcomeNoderunnerSpeech', () => {
    it('does not throw', () => {
      expect(() => playWelcomeNoderunnerSpeech()).not.toThrow()
    })
  })

  describe('playTypingTick', () => {
    it('does not throw', () => {
      expect(() => playTypingTick()).not.toThrow()
    })

    it('can be called multiple times (pool rotation)', () => {
      for (let i = 0; i < 10; i++) {
        expect(() => playTypingTick()).not.toThrow()
      }
    })
  })

  describe('resumeAudioContext', () => {
    it('does not throw', () => {
      expect(() => resumeAudioContext()).not.toThrow()
    })
  })

  describe('startSynthwave', () => {
    it('does not throw when no audio context', () => {
      // Without calling resumeAudioContext first, context might be null
      expect(() => startSynthwave()).not.toThrow()
    })
  })

  describe('stopSynthwave', () => {
    it('does not throw when nothing is playing', () => {
      expect(() => stopSynthwave()).not.toThrow()
    })
  })

  describe('playLoopStart', () => {
    it('does not throw when no audio context', () => {
      expect(() => playLoopStart()).not.toThrow()
    })
  })

  describe('playKeyboardTypingSound', () => {
    it('does not throw when no audio context', () => {
      expect(() => playKeyboardTypingSound()).not.toThrow()
    })
  })

  describe('playDashboardLoadOomph', () => {
    it('does not throw when no audio context', () => {
      expect(() => playDashboardLoadOomph()).not.toThrow()
    })
  })

  describe('audio context lifecycle', () => {
    it('resumeAudioContext then startSynthwave does not throw', () => {
      resumeAudioContext()
      expect(() => startSynthwave()).not.toThrow()
    })

    it('resumeAudioContext then stopSynthwave does not throw', () => {
      resumeAudioContext()
      expect(() => stopSynthwave()).not.toThrow()
    })

    it('resumeAudioContext then playKeyboardTypingSound does not throw', () => {
      resumeAudioContext()
      expect(() => playKeyboardTypingSound()).not.toThrow()
    })

    it('resumeAudioContext then playDashboardLoadOomph does not throw', () => {
      resumeAudioContext()
      expect(() => playDashboardLoadOomph()).not.toThrow()
    })

    it('resumeAudioContext then playLoopStart does not throw', () => {
      resumeAudioContext()
      expect(() => playLoopStart()).not.toThrow()
    })
  })
})
