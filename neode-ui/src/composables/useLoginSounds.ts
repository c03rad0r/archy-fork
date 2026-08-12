/**
 * Login screen audio: intro loop (MP3) + transition sounds.
 *
 * First-install vs returning-user gate: the synthwave loop, welcome
 * voice, pop/whoosh/oomph transitions exist for the first-boot cinematic
 * moment. After the user has completed onboarding we silence all of
 * them — every subsequent login should be quiet. Typing sounds are
 * exempt and continue to play regardless.
 */

/** One-way, per-page-load override: when App.vue decides to run the intro
 *  splash (demo fresh boot, prod first install, Replay Intro), the WHOLE
 *  cinematic — speech, synthwave, pops — must be audible even though
 *  localStorage already says onboarding is complete. Without this the
 *  replayed intro runs silent (lost "Welcome Noderunner" + song). */
let cinematicMode = false
export function enableCinematicSounds() {
  cinematicMode = true
}

/** True when the node has not yet completed onboarding — i.e. we're
 *  still in the first-install cinematic — or when the intro is being
 *  deliberately replayed this page load (see enableCinematicSounds).
 *  Reads the localStorage cache set by useOnboarding (which is
 *  re-seeded from the backend on each successful check), so this stays
 *  correct after a browser clear once the onboarding-complete probe
 *  runs. Sound calls that fire before that probe completes will fall
 *  through silent on an already-onboarded node — which is exactly what
 *  we want: ordinary re-logins stay quiet. */
function isFirstInstallPhase(): boolean {
  if (cinematicMode) return true
  try {
    return localStorage.getItem('neode_onboarding_complete') !== '1'
  } catch {
    return true
  }
}

let audioContext: AudioContext | null = null
let introAudio: HTMLAudioElement | null = null
let introGain: GainNode | null = null

/** Get AudioContext - only returns existing. Create via resumeAudioContext() after user gesture. */
function getContext(): AudioContext | null {
  return audioContext
}

/** Create AudioContext if needed (call only from user gesture - click/tap/key) */
function ensureContext(): AudioContext | null {
  if (audioContext) return audioContext
  try {
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext
    if (!Ctx) return null
    audioContext = new Ctx()
    return audioContext
  } catch {
    return null
  }
}

const INTRO_AUDIO_URL = '/assets/audio/cosmic-updrift.mp3'
const LOOP_START_URL = '/assets/audio/loop-start.mp3'

/** Play loop-start when transitioning from typing intro to Welcome Noderunner, as the intro music comes in.
 * Uses Web Audio API so it plays after context is resumed (user gesture). */
export function playLoopStart() {
  if (!isFirstInstallPhase()) return
  const ctx = getContext()
  if (!ctx) return
  try {
    if (ctx.state === 'suspended') ctx.resume()
  } catch {
    return
  }
  fetch(LOOP_START_URL)
    .then((res) => res.arrayBuffer())
    .then((buf) => ctx.decodeAudioData(buf))
    .then((decoded) => {
      const src = ctx.createBufferSource()
      src.buffer = decoded
      const gain = ctx.createGain()
      gain.gain.value = 0.5
      src.connect(gain)
      gain.connect(ctx.destination)
      src.start(0)
    })
    .catch(() => {})
}

/** Resume audio context - MUST be called from user gesture (click/tap/key). Creates context if needed. */
export function resumeAudioContext() {
  const ctx = ensureContext()
  if (ctx?.state === 'suspended') {
    ctx.resume().catch(() => {})
  }
}

/** Start intro loop - Cosmic Updrift. Only works after resumeAudioContext() (user gesture). */
export function startSynthwave() {
  if (!isFirstInstallPhase()) return
  const ctxOrNull = getContext()
  if (!ctxOrNull) return

  try {
    if (ctxOrNull.state === 'suspended') ctxOrNull.resume().catch(() => {})
  } catch {
    return
  }

  stopSynthwave()

  const audio = new Audio(INTRO_AUDIO_URL)
  audio.loop = true

  const ctx = ctxOrNull
  const source = ctx.createMediaElementSource(audio)
  const gainNode = ctx.createGain()
  gainNode.gain.value = 0.25
  source.connect(gainNode)
  gainNode.connect(ctx.destination)
  introGain = gainNode
  introAudio = audio

  audio.play().catch(() => {})
}

/** Stop intro loop (call on login success) */
export function stopSynthwave() {
  if (introAudio) {
    if (introGain && audioContext) {
      const t = audioContext.currentTime
      introGain.gain.setValueAtTime(introGain.gain.value, t)
      introGain.gain.linearRampToValueAtTime(0.001, t + 0.2)
    }
    setTimeout(() => {
      introAudio?.pause()
      introAudio = null
      introGain = null
    }, 220)
  }
}

/** Stop ALL login audio and close AudioContext. Call on route change to dashboard. */
export function stopAllAudio() {
  // Stop synthwave loop
  if (introAudio) {
    introAudio.pause()
    introAudio = null
    introGain = null
  }
  // Stop intro typing
  stopIntroTyping()
  // Close AudioContext to kill any lingering BufferSource nodes (playLoopStart)
  if (audioContext) {
    audioContext.close().catch(() => {})
    audioContext = null
  }
}

/** Pop sound - plays when intro initiator (tap to start) is pressed */
export function playPop() {
  if (!isFirstInstallPhase()) return
  const audio = new Audio('/assets/audio/pop.mp3')
  audio.volume = 0.6
  audio.play().catch(() => {})
}

/** Whoosh transition on successful login */
export function playLoginSuccessWhoosh() {
  if (!isFirstInstallPhase()) return
  const woosh = new Audio('/assets/audio/woosh.mp3')
  woosh.volume = 0.5
  woosh.play().catch(() => {})
}

/** Typing sound - plays once when welcome typing starts (typing.mp3) */
export function playTypingSound() {
  const audio = new Audio('/assets/audio/typing.mp3')
  audio.volume = 0.6
  audio.play().catch(() => {})
}

/** Intro typing - ONE sound per sentence: play when sentence starts, stop when it ends (intro-typing.mp3 from archy assets) */
let introTypingAudio: HTMLAudioElement | null = null
const INTRO_TYPING_URL = '/assets/audio/intro-typing.mp3'

export function playIntroTyping() {
  stopIntroTyping()
  introTypingAudio = new Audio(INTRO_TYPING_URL)
  introTypingAudio.volume = 0.5
  introTypingAudio.loop = true
  introTypingAudio.play().catch(() => {})
}

export function stopIntroTyping() {
  if (introTypingAudio) {
    introTypingAudio.pause()
    introTypingAudio.currentTime = 0
    introTypingAudio = null
  }
}

const WELCOME_SPEECH_URL = '/assets/audio/welcome-noderunner.mp3'

/** Sci-fi female voice: "Welcome Noderunner" - plays when welcome text types in.
 * Requires pre-recorded audio from ElevenLabs. Run:
 *   ELEVENLABS_API_KEY=your_key node neode-ui/scripts/generate-welcome-speech.js
 * Browse sci-fi voices at elevenlabs.io/voice-library and set ELEVENLABS_VOICE_ID for custom voice. */
export function playWelcomeNoderunnerSpeech() {
  if (!isFirstInstallPhase()) return
  const audio = new Audio(WELCOME_SPEECH_URL)
  audio.volume = 0.9
  audio.play().catch(() => {})
}

/** Typing tick - for dashboard welcome typing (typing.mp3) */
let typingTickPool: HTMLAudioElement[] = []
const TYPING_TICK_POOL_SIZE = 5

function getTypingTick(): HTMLAudioElement {
  if (typingTickPool.length === 0) {
    for (let i = 0; i < TYPING_TICK_POOL_SIZE; i++) {
      const a = new Audio('/assets/audio/typing.mp3')
      a.volume = 0.4
      typingTickPool.push(a)
    }
  }
  const a = typingTickPool.shift()!
  typingTickPool.push(a)
  return a
}

export function playTypingTick() {
  const a = getTypingTick()
  a.currentTime = 0
  a.play().catch(() => {})
}

/** Keyboard input sound - short synthesized click per key. Does NOT use typing.mp3 or intro-typing.mp3. */
export function playKeyboardTypingSound() {
  const ctx = getContext()
  if (!ctx) return

  try {
    if (ctx.state === 'suspended') ctx.resume()
  } catch {
    return
  }

  const t = ctx.currentTime
  const osc = ctx.createOscillator()
  const gain = ctx.createGain()

  osc.type = 'sine'
  osc.frequency.setValueAtTime(1200, t)
  gain.gain.setValueAtTime(0, t)
  gain.gain.linearRampToValueAtTime(0.06, t + 0.002)
  gain.gain.exponentialRampToValueAtTime(0.001, t + 0.04)

  osc.connect(gain)
  gain.connect(ctx.destination)
  osc.start(t)
  osc.stop(t + 0.04)
}

/** Gaming-style boot thud - soft impact when dashboard loads */
/**
 * @param force Play regardless of install phase. The first dashboard entry
 * right after the onboarding wizard needs this: by then onboarding is already
 * flagged complete in localStorage, so the isFirstInstallPhase() gate (which
 * exists to keep ordinary re-logins quiet) would silence the one entrance
 * that SHOULD be loud.
 */
export function playDashboardLoadOomph(force = false) {
  if (!force && !isFirstInstallPhase()) return
  // The login→dashboard route change runs stopAllAudio(), which CLOSES the
  // AudioContext — so the context must be recreated here, not just fetched.
  // The login click's sticky user activation lets the fresh context run.
  const ctx = ensureContext()
  if (!ctx) return

  try {
    if (ctx.state === 'suspended') ctx.resume()
  } catch {
    return
  }

  const t = ctx.currentTime

  // Soft layered thud - sine only, smooth attack/decay
  const layers = [
    { freq: 55, dur: 0.35, gain: 0.4, attack: 0.02 },
    { freq: 82, dur: 0.28, gain: 0.25, attack: 0.03 },
    { freq: 110, dur: 0.22, gain: 0.15, attack: 0.04 },
    { freq: 165, dur: 0.18, gain: 0.1, attack: 0.05 },
  ]

  for (let i = 0; i < layers.length; i++) {
    const L = layers[i]!
    const osc = ctx.createOscillator()
    const g = ctx.createGain()
    osc.type = 'sine'
    osc.frequency.value = L.freq
    g.gain.setValueAtTime(0, t)
    g.gain.linearRampToValueAtTime(L.gain, t + L.attack)
    g.gain.exponentialRampToValueAtTime(0.001, t + L.dur)
    osc.connect(g)
    g.connect(ctx.destination)
    osc.start(t + i * 0.01)
    osc.stop(t + L.dur)
  }
}
