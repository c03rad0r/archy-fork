/**
 * Epic interface sounds for controller/keyboard navigation.
 * Layered synthesis - cool, impactful, celebratory for actions.
 */

let audioContext: AudioContext | null = null

function getContext(): AudioContext | null {
  if (audioContext) return audioContext
  try {
    audioContext = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)()
    return audioContext
  } catch {
    return null
  }
}

function playTone(
  ctx: AudioContext,
  freq: number,
  duration: number,
  gain: number,
  type: OscillatorType = 'sine',
  startOffset = 0
) {
  const osc = ctx.createOscillator()
  const g = ctx.createGain()
  osc.connect(g)
  g.connect(ctx.destination)
  g.gain.setValueAtTime(0, ctx.currentTime)
  g.gain.linearRampToValueAtTime(gain, ctx.currentTime + 0.01)
  g.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + duration)
  osc.frequency.value = freq
  osc.type = type
  osc.start(ctx.currentTime + startOffset)
  osc.stop(ctx.currentTime + startOffset + duration)
}

export function playNavSound(type: 'move' | 'select' | 'action' | 'back' = 'move') {
  if (type === 'move') {
    const audio = new Audio('/assets/audio/arrows.mp3')
    audio.volume = 0.5
    audio.play().catch(() => {})
    return
  }
  if (type === 'select' || type === 'action') {
    const audio = new Audio('/assets/audio/enter.mp3')
    audio.volume = 0.5
    audio.play().catch(() => {})
    return
  }

  const ctx = getContext()
  if (!ctx) return

  try {
    if (ctx.state === 'suspended') ctx.resume()
  } catch {
    return
  }

  switch (type) {
    case 'back': {
      playTone(ctx, 440, 0.06, 0.08, 'sine')
      playTone(ctx, 330, 0.08, 0.05, 'sine', 0.03)
      playTone(ctx, 220, 0.1, 0.04, 'triangle', 0.05)
      break
    }
  }
}
