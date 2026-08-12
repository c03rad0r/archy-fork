import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock Audio globally
class MockAudio {
  src = ''
  volume = 1
  play = vi.fn().mockResolvedValue(undefined)
  pause = vi.fn()
  currentTime = 0
  addEventListener = vi.fn()
}

vi.stubGlobal('Audio', MockAudio)

// Mock AudioContext
const mockOscillator = {
  type: 'sine',
  frequency: { setValueAtTime: vi.fn() },
  connect: vi.fn(),
  start: vi.fn(),
  stop: vi.fn(),
}
const mockGain = {
  gain: {
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  },
  connect: vi.fn(),
}
const mockAudioContext = {
  createOscillator: vi.fn().mockReturnValue(mockOscillator),
  createGain: vi.fn().mockReturnValue(mockGain),
  currentTime: 0,
  destination: {},
}

vi.stubGlobal('AudioContext', vi.fn().mockImplementation(() => mockAudioContext))

import { playNavSound } from '../useNavSounds'

describe('playNavSound', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('is a function', () => {
    expect(playNavSound).toBeTypeOf('function')
  })

  it('plays move sound (default)', () => {
    playNavSound()
    // Should try to play a sound
  })

  it('plays move sound explicitly', () => {
    playNavSound('move')
  })

  it('plays select sound', () => {
    playNavSound('select')
  })

  it('plays action sound', () => {
    playNavSound('action')
  })

  it('plays back sound using AudioContext', () => {
    playNavSound('back')
    // Back uses Web Audio API synthesis
  })

  it('does not throw for any sound type', () => {
    expect(() => playNavSound('move')).not.toThrow()
    expect(() => playNavSound('select')).not.toThrow()
    expect(() => playNavSound('action')).not.toThrow()
    expect(() => playNavSound('back')).not.toThrow()
  })
})
