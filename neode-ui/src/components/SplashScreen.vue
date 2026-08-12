<template>
  <Transition name="splash-fade">
    <div v-if="showSplash" class="fixed inset-0 z-[2000] flex items-center justify-center bg-black" style="will-change: opacity, transform;">
      <!-- Video background - shown during Welcome Noderunner and Logo (seamless, no zoom) -->
      <video
        v-if="showWelcome || showLogo"
        ref="videoElement"
        class="absolute inset-0 w-full h-full object-cover"
        :style="{ opacity: backgroundOpacity, transform: 'scale(1)', transition: 'opacity 1.2s ease-out' }"
        autoplay
        loop
        muted
        playsinline
        preload="auto"
        poster="/assets/img/bg-intro.jpg"
      >
        <source src="/assets/video/video-intro.mp4?v=8" type="video/mp4">
        <!-- Fallback to image if video fails -->
        <div 
          class="absolute inset-0"
          :style="{ 
            backgroundImage: 'url(/assets/img/bg-intro.jpg)', 
            backgroundSize: 'auto 100vh', 
            backgroundPosition: 'center top', 
            backgroundRepeat: 'no-repeat',
          }"
        />
      </video>
      
      <!-- Static image background - shown during alien intro -->
      <div 
        v-else
        class="absolute inset-0"
        :style="{ 
          backgroundImage: 'url(/assets/img/bg-intro.jpg)', 
          backgroundSize: 'auto 100vh', 
          backgroundPosition: 'center top', 
          backgroundRepeat: 'no-repeat',
          opacity: backgroundOpacity,
          transform: 'scale(1)',
          transition: 'opacity 1.2s ease-out',
        }"
      />

      <!-- Alien Intro -->
      <Transition name="fade">
        <div 
          v-if="!alienIntroComplete" 
          class="absolute inset-0 z-10 flex items-center justify-center transition-opacity duration-800"
          :class="{ 'opacity-0': fadeAlienIntro }"
        >
          <div class="font-mono text-white px-4 sm:px-5 max-w-[95vw] sm:max-w-[90vw] md:max-w-[1200px] text-base sm:text-lg md:text-[24px] leading-relaxed break-words">
            <div v-if="showLine1" class="flex items-start mb-4 sm:mb-6 opacity-0" :class="{ 'opacity-100': showLine1 }">
              <span class="text-[#fbbf24] mr-3 sm:mr-6 flex-shrink-0">></span>
              <span class="text-white break-words">{{ displayLine1 }}</span><span v-if="isTypingLine1" class="intro-typing-caret" aria-hidden="true"></span>
            </div>
            <div v-if="showLine2" class="flex items-start mb-4 sm:mb-6 opacity-0" :class="{ 'opacity-100': showLine2 }">
              <span class="text-[#fbbf24] mr-3 sm:mr-6 flex-shrink-0">></span>
              <span class="text-white break-words">{{ displayLine2 }}</span><span v-if="isTypingLine2" class="intro-typing-caret" aria-hidden="true"></span>
            </div>
            <div v-if="showLine3" class="flex items-start mb-4 sm:mb-6 opacity-0" :class="{ 'opacity-100': showLine3 }">
              <span class="text-[#fbbf24] mr-3 sm:mr-6 flex-shrink-0">></span>
              <span class="text-white break-words">{{ displayLine3 }}</span><span v-if="isTypingLine3" class="intro-typing-caret" aria-hidden="true"></span>
            </div>
            <div v-if="showLine4" class="flex items-start mb-8 sm:mb-12 opacity-0" :class="{ 'opacity-100': showLine4 }">
              <span class="text-[#fbbf24] mr-3 sm:mr-6 flex-shrink-0">></span>
              <span class="text-white break-words">{{ displayLine4 }}</span><span v-if="isTypingLine4" class="intro-typing-caret" aria-hidden="true"></span>
            </div>
          </div>
        </div>
      </Transition>

      <!-- Welcome Message -->
      <Transition name="welcome-fade">
        <div 
          v-if="showWelcome" 
          class="absolute inset-0 z-[15] flex items-center justify-center font-mono text-3xl sm:text-4xl md:text-5xl px-4"
          :class="{ 'welcome-fade-out': fadeWelcome }"
        >
          <div class="typing-container">
            <span class="text-white" :class="{ 'typing-text': typingWelcome }">
            Welcome Noderunner
          </span>
          </div>
        </div>
      </Transition>

      <!-- Logo - Archipelago logo for splash -->
      <Transition name="logo-zoom">
        <div v-if="showLogo" class="relative z-20 logo-container">
          <img 
            src="/assets/img/logo-archipelago.svg" 
            alt="Archipelago" 
            class="w-[min(80vw,900px)] max-w-[90vw] h-auto filter drop-shadow-[0_6px_24px_rgba(0,0,0,0.35)] m-5 object-contain logo-zoom-bounce"
          />
        </div>
      </Transition>

      <!-- Tap to start - logo + "Enter the Exit" behind (like screensaver) -->
      <div
        v-if="showTapToStart"
        class="absolute inset-0 z-[100] flex items-center justify-center cursor-pointer overflow-hidden"
        :class="tapStartTransitioning ? 'tap-overlay-zoom-out' : 'bg-black/40'"
        @click="handleTapToStart"
      >
        <div class="tap-to-start-content relative flex items-center justify-center perspective-1000">
          <span
            class="tap-to-start-text font-archipelago font-extrabold text-6xl sm:text-7xl md:text-8xl lg:text-9xl tracking-widest uppercase whitespace-nowrap select-none transition-opacity duration-300"
            :class="{ 'opacity-0': tapStartTransitioning }"
          >
            Enter to Exit
          </span>
          <div
            class="tap-to-start-logo absolute transition-transform duration-300 ease-out"
            :class="[
              { 'tap-logo-launch': tapStartTransitioning },
              { 'scale-110': introLogoHover && !tapStartTransitioning }
            ]"
            @mouseenter="onIntroLogoHover"
            @mouseleave="introLogoHover = false"
          >
            <!-- Audio viz ring - visible on hover -->
            <div
              class="intro-logo-viz-ring"
              :class="{ 'intro-logo-viz-visible': introLogoHover && !tapStartTransitioning }"
            >
              <div
                v-for="i in 48"
                :key="i - 1"
                class="intro-logo-viz-segment"
                :style="{ '--segment-deg': `${((i - 1) / 48) * 360}deg`, '--segment-index': i - 1 }"
              ></div>
            </div>
            <ScreensaverLogo />
          </div>
        </div>
      </div>

      <!-- Skip Button -->
      <button
        v-if="!alienIntroComplete && !showTapToStart"
        @click="handleSkipClick"
        class="absolute bottom-8 right-8 z-20 bg-black/60 border border-white/30 text-white/70 font-mono text-xs px-4 py-2 rounded backdrop-blur-[10px] hover:bg-black/80 hover:text-white/90 hover:border-white/50 hover:-translate-y-0.5 active:translate-y-0 transition-all duration-300"
      >
        Skip Intro
      </button>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import ScreensaverLogo from '@/components/ScreensaverLogo.vue'
import { playIntroTyping, playKeyboardTypingSound, playLoopStart, playPop, playWelcomeNoderunnerSpeech, resumeAudioContext, startSynthwave, stopIntroTyping } from '@/composables/useLoginSounds'

const emit = defineEmits<{
  complete: []
}>()

const INTRO_LINES = [
  'In the future there will be 3 types of humans',
  'Government Employees',
  'Corporate Employees',
  'And Noderunners...',
] as const
const MS_PER_CHAR = 55
const BLINK_AFTER_TYPING = 1500

const showSplash = ref(true)
const showTapToStart = ref(true)
const tapStartTransitioning = ref(false)
const introLogoHover = ref(false)
const backgroundOpacity = ref(0)
const alienIntroComplete = ref(false)
const fadeAlienIntro = ref(false)
const showWelcome = ref(false)
const fadeWelcome = ref(false)
const typingWelcome = ref(false)
const showLogo = ref(false)
const showLine1 = ref(false)
const showLine2 = ref(false)
const showLine3 = ref(false)
const showLine4 = ref(false)
const displayLine1 = ref('')
const displayLine2 = ref('')
const displayLine3 = ref('')
const displayLine4 = ref('')
const isTypingLine1 = ref(false)
const isTypingLine2 = ref(false)
const isTypingLine3 = ref(false)
const isTypingLine4 = ref(false)
const videoElement = ref<HTMLVideoElement | null>(null)
let introTypingTimeout: ReturnType<typeof setTimeout> | null = null
const pendingTimers: ReturnType<typeof setTimeout>[] = []

function scheduleTimer(fn: () => void, delay: number) {
  const id = setTimeout(fn, delay)
  pendingTimers.push(id)
  return id
}

// Ensure video plays continuously from Welcome Noderunner through logo
let videoPauseHandler: ((e: Event) => void) | null = null
watch([showWelcome, showLogo], ([welcome, logo]) => {
  if ((welcome || logo) && videoElement.value) {
    if (videoElement.value.paused) {
      videoElement.value.play().catch(err => {
        if (import.meta.env.DEV) console.warn('Video autoplay failed:', err)
      })
    }
    // Add pause prevention handler once, remove when no longer needed
    if (!videoPauseHandler) {
      videoPauseHandler = () => {
        if ((showWelcome.value || showLogo.value) && videoElement.value) {
          videoElement.value.play().catch(() => {})
        }
      }
      videoElement.value.addEventListener('pause', videoPauseHandler)
    }
  } else if (videoPauseHandler && videoElement.value) {
    videoElement.value.removeEventListener('pause', videoPauseHandler)
    videoPauseHandler = null
  }
})

// Start video as soon as welcome appears
watch(showWelcome, (isShowing) => {
  if (isShowing && videoElement.value) {
    // Start video immediately when welcome appears
    videoElement.value.play().catch(err => {
      if (import.meta.env.DEV) console.warn('Video autoplay failed on welcome:', err)
    })
  }
})

// Store video currentTime continuously and before unmounting for seamless transition
watch(showSplash, (isShowing) => {
  if (!isShowing && videoElement.value) {
    // Store current video time for seamless transition
    const currentTime = videoElement.value.currentTime
    const wasPlaying = !videoElement.value.paused
    sessionStorage.setItem('video_intro_currentTime', currentTime.toString())
    sessionStorage.setItem('video_intro_wasPlaying', wasPlaying.toString())
    // Store video playback rate to maintain smooth playback
    sessionStorage.setItem('video_intro_playbackRate', videoElement.value.playbackRate.toString())
  }
})

// Continuously update video time while playing (for more accurate restoration)
let videoTimeUpdateInterval: number | null = null
watch([showWelcome, showLogo], ([welcome, logo]) => {
  if ((welcome || logo) && videoElement.value) {
    // Update stored time every 50ms for better accuracy and smoother transition
    videoTimeUpdateInterval = window.setInterval(() => {
      if (videoElement.value && !videoElement.value.paused) {
        sessionStorage.setItem('video_intro_currentTime', videoElement.value.currentTime.toString())
        sessionStorage.setItem('video_intro_wasPlaying', 'true')
        sessionStorage.setItem('video_intro_playbackRate', videoElement.value.playbackRate.toString())
      }
    }, 50) // More frequent updates for smoother transition
  } else {
    if (videoTimeUpdateInterval) {
      clearInterval(videoTimeUpdateInterval)
      videoTimeUpdateInterval = null
    }
  }
})

// NOTE: whether the intro should play is decided ONCE by App.vue
// (shouldShowIntroSplash) — this component is only mounted when the answer was
// yes, so it must not re-check neode_intro_seen itself. The old internal
// re-check silently skipped the whole sequence for any browser that had seen
// the intro before, even when App.vue explicitly asked for a replay (demo
// fresh visits, the Replay Intro link).

function handleEnterKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && showTapToStart.value && !tapStartTransitioning.value) {
    e.preventDefault()
    handleTapToStart()
  }
}

function onIntroLogoHover() {
  introLogoHover.value = true
  if (!tapStartTransitioning.value) playKeyboardTypingSound()
}

function handleTapToStart() {
  if (!showTapToStart.value || tapStartTransitioning.value) return
  resumeAudioContext()
  playPop()
  tapStartTransitioning.value = true
  // Logo: grow (150ms) then zoom out to background (850ms). Total 1s.
  setTimeout(() => {
    showTapToStart.value = false
    tapStartTransitioning.value = false
    startAlienIntro()
  }, 1000)
}

function handleSkipClick() {
  resumeAudioContext()
  skipIntro()
}

function skipIntro() {
  // Jump to "Welcome Noderunner" part
  if (introTypingTimeout) {
    clearTimeout(introTypingTimeout)
    introTypingTimeout = null
  }
  alienIntroComplete.value = true
  fadeAlienIntro.value = true
  showWelcome.value = true
  typingWelcome.value = true
  stopIntroTyping()
  playLoopStart()
  startSynthwave()
  playWelcomeNoderunnerSpeech()

  // Stop alien intro typing and any playing typing sound
  stopIntroTyping()
  isTypingLine1.value = false
  isTypingLine2.value = false
  isTypingLine3.value = false
  isTypingLine4.value = false
  
  // Start background fade in at 0.3 opacity when welcome appears
  scheduleTimer(() => {
    backgroundOpacity.value = 0.3
  }, 0)

  // Continue with welcome fade out after typing (2s) + cursor continues (1.5s) + 3 blinks (1.35s)
  scheduleTimer(() => {
    fadeWelcome.value = true
    typingWelcome.value = false
  }, 4850)

  // Show logo - no zoom, just fade
  scheduleTimer(() => {
    showLogo.value = true
  }, 5500)

  // Hide welcome after logo starts appearing
  scheduleTimer(() => {
    showWelcome.value = false
  }, 6000)

  // Fade background to full opacity just before completing (for smooth transition to modal)
  scheduleTimer(() => {
    backgroundOpacity.value = 1
  }, 9000)

  // Complete splash with smooth transition
  scheduleTimer(() => {
    scheduleTimer(() => {
      showSplash.value = false
      document.body.classList.add('splash-complete')
      localStorage.setItem('neode_intro_seen', '1')
      emit('complete')
    }, 500)
  }, 9500)
}

function startAlienIntro() {
  function typeLine(
    lineIndex: number,
    displayRef: { value: string },
    isTypingRef: { value: boolean },
    onDone: () => void
  ) {
    const text = INTRO_LINES[lineIndex]!
    let i = 0
    displayRef.value = ''
    isTypingRef.value = true

    function tick() {
      if (i === 0) {
        playIntroTyping()
      }
      if (i < text.length) {
        displayRef.value = text.slice(0, i + 1)
        i++
        introTypingTimeout = setTimeout(tick, MS_PER_CHAR)
      } else {
        stopIntroTyping()
        isTypingRef.value = false
        introTypingTimeout = setTimeout(onDone, BLINK_AFTER_TYPING)
      }
    }
    tick()
  }

  function scheduleLine1() {
    showLine1.value = true
    typeLine(0, displayLine1, isTypingLine1, scheduleLine2)
  }

  function scheduleLine2() {
    showLine2.value = true
    typeLine(1, displayLine2, isTypingLine2, scheduleLine3)
  }

  function scheduleLine3() {
    showLine3.value = true
    typeLine(2, displayLine3, isTypingLine3, scheduleLine4)
  }

  function scheduleLine4() {
    showLine4.value = true
    typeLine(3, displayLine4, isTypingLine4, () => {
      isTypingLine4.value = false
      fadeAlienIntro.value = true
      introTypingTimeout = setTimeout(showWelcomePhase, 800)
    })
  }

  function showWelcomePhase() {
    alienIntroComplete.value = true
    showWelcome.value = true
    typingWelcome.value = true
    stopIntroTyping()
    playLoopStart()
    startSynthwave()
    playWelcomeNoderunnerSpeech()
    if (videoElement.value) {
      videoElement.value.play().catch(err => {
        if (import.meta.env.DEV) console.warn('Video autoplay failed on welcome:', err)
      })
    }
    backgroundOpacity.value = 0.3

    scheduleTimer(() => {
      fadeWelcome.value = true
      typingWelcome.value = false
    }, 4850)

    scheduleTimer(() => {
      showLogo.value = true
    }, 5500)

    scheduleTimer(() => {
      showWelcome.value = false
    }, 6000)

    scheduleTimer(() => {
      backgroundOpacity.value = 1
    }, 9000)

    scheduleTimer(() => {
      if (videoElement.value && !videoElement.value.paused) {
        sessionStorage.setItem('video_intro_currentTime', videoElement.value.currentTime.toString())
        sessionStorage.setItem('video_intro_wasPlaying', 'true')
      }
      showSplash.value = false
      document.body.classList.add('splash-complete')
      localStorage.setItem('neode_intro_seen', '1')
      emit('complete')
    }, 9500)
  }

  introTypingTimeout = setTimeout(scheduleLine1, 500)

}

onMounted(() => {
  window.addEventListener('keydown', handleEnterKey)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleEnterKey)
  if (introTypingTimeout) {
    clearTimeout(introTypingTimeout)
    introTypingTimeout = null
  }
  // Clear all scheduled timers to prevent firing on unmounted component
  for (const id of pendingTimers) clearTimeout(id)
  pendingTimers.length = 0
  // Clear video time update interval
  if (videoTimeUpdateInterval) {
    clearInterval(videoTimeUpdateInterval)
    videoTimeUpdateInterval = null
  }
})
</script>

<style scoped>
.splash-fade-enter-active {
  transition: opacity 0.5s ease;
}

.splash-fade-leave-active {
  transition: opacity 1s ease-out, transform 1s ease-out;
}

.splash-fade-enter-from,
.splash-fade-leave-to {
  opacity: 0;
}

.splash-fade-leave-to {
  transform: scale(1.1);
}

/* Wide logo zooms out towards user when login modal comes in */
.splash-fade-leave-active .logo-container {
  transition: transform 1s cubic-bezier(0.25, 0.46, 0.45, 0.94);
  transform-origin: center center;
}

.splash-fade-leave-to .logo-container {
  transform: scale(1.4);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.5s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* Welcome message fade out */
.welcome-fade-enter-active {
  transition: opacity 0.8s ease-out;
}

.welcome-fade-leave-active {
  transition: opacity 0.6s ease-in;
}

.welcome-fade-enter-from,
.welcome-fade-leave-to {
  opacity: 0;
}

.welcome-fade-out {
  opacity: 0;
  transition: opacity 0.6s ease-in;
}

/* Logo zoom bounce animation - smooth and buttery */
.logo-zoom-enter-active {
  transition: all 1s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}

.logo-zoom-leave-active {
  transition: opacity 0.5s ease-out, transform 0.5s ease-out;
}

.logo-zoom-enter-from {
  opacity: 0;
  transform: scale(0.7);
}

.logo-zoom-enter-to {
  opacity: 1;
  transform: scale(1);
}

.logo-zoom-leave-from {
  opacity: 1;
  transform: scale(1);
}

.logo-zoom-leave-to {
  opacity: 0;
  transform: scale(1.05);
}

.logo-zoom-bounce {
  animation: logoZoomBounce 1.2s cubic-bezier(0.25, 0.46, 0.45, 0.94) forwards;
}

@keyframes logoZoomBounce {
  0% {
    transform: scale(0.85);
    opacity: 0;
  }
  50% {
    transform: scale(1.02);
    opacity: 0.9;
  }
  75% {
    transform: scale(0.98);
    opacity: 0.95;
  }
  100% {
    transform: scale(1);
    opacity: 1;
  }
}

/* Container to keep the typing text centered */
.typing-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-width: 0;
}

/* Intro typing cursor - block style, yellow blink (Archipelago style) */
.intro-typing-caret {
  display: inline-block;
  width: 4px;
  min-width: 4px;
  height: 1.2em;
  background: #fbbf24;
  margin-left: 2px;
  vertical-align: text-bottom;
  animation: intro-caret-blink 0.5s step-end infinite;
}

@keyframes intro-caret-blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

/* Ensure text wraps smoothly on mobile */
.font-mono {
  word-wrap: break-word;
  overflow-wrap: break-word;
  hyphens: auto;
}

/* Smooth line transitions for mobile */
@media (max-width: 640px) {
  .font-mono span {
    display: inline-block;
    max-width: 100%;
  }
}

/* Background zoom transition - matches OnboardingWrapper style */
.bg-zoom-transition {
  transition: transform 1.5s cubic-bezier(0.4, 0, 0.2, 1), opacity 1.2s ease-out;
  transform: scale(1);
  transform-origin: center center;
  will-change: transform, opacity;
}

.bg-zoom-transition.bg-zoom-in {
  transform: scale(1.15);
}

/* Tap to start - logo grow then zoom out to background */
.tap-overlay-zoom-out {
  background-color: rgba(0, 0, 0, 0.4);
  transition: background-color 0.6s cubic-bezier(0.4, 0, 0.2, 1);
  animation: tap-overlay-fade 1s ease-out forwards;
}
@keyframes tap-overlay-fade {
  0% { background-color: rgba(0, 0, 0, 0.4); }
  30% { background-color: rgba(0, 0, 0, 0.35); }
  100% { background-color: rgba(0, 0, 0, 0); }
}
.perspective-1000 {
  perspective: 1000px;
}
.tap-logo-launch {
  animation: tap-logo-launch 1s cubic-bezier(0.22, 1, 0.36, 1) forwards;
  transform-origin: center center;
  will-change: transform, opacity;
}
@keyframes tap-logo-launch {
  0% { transform: scale(1); opacity: 1; }
  15% { transform: scale(1.2); opacity: 1; }
  25% { transform: scale(1.15); opacity: 1; }
  100% { transform: scale(0); opacity: 0; }
}

/* Tap to start - "Enter the Exit" big behind logo */
.tap-to-start-content {
  min-height: 12rem;
}
.tap-to-start-text {
  position: absolute;
  z-index: 0;
  pointer-events: none;
  background: linear-gradient(
    90deg,
    rgba(0, 0, 0, 0.35) 0%,
    rgba(0, 0, 0, 0.35) 38%,
    rgba(0, 0, 0, 0.35) 40%,
    rgba(255, 255, 255, 0.5) 48%,
    rgba(255, 255, 255, 0.7) 50%,
    rgba(255, 255, 255, 0.5) 52%,
    rgba(0, 0, 0, 0.35) 60%,
    rgba(0, 0, 0, 0.35) 100%
  );
  background-size: 250% 100%;
  background-position: 0% 0;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: tap-to-start-flare-wipe 14s ease-in-out infinite;
}
@keyframes tap-to-start-flare-wipe {
  0%, 82%, 100% {
    background-position: 0% 0;
  }
  88% {
    background-position: 100% 0;
  }
}
.tap-to-start-logo {
  position: relative;
  z-index: 1;
  filter: drop-shadow(0 0 40px rgba(255, 255, 255, 0.15));
  overflow: visible;
}
.intro-logo-viz-ring {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 1px;
  height: 1px;
  transform: translate(-50%, -50%);
  z-index: 0;
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.35s ease;
  --viz-radius: 7rem;
}
.intro-logo-viz-ring.intro-logo-viz-visible {
  opacity: 1;
}
.intro-logo-viz-segment {
  position: absolute;
  left: 50%;
  top: 50%;
  width: 4px;
  height: 24px;
  margin-left: -2px;
  margin-top: -12px;
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.6), rgba(255, 255, 255, 0.15));
  border-radius: 2px;
  transform-origin: center center;
  transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius)));
  animation: intro-viz-pulse 2.5s ease-in-out infinite;
  animation-delay: calc(var(--segment-index, 0) * 0.02s);
}
@keyframes intro-viz-pulse {
  0%, 100% { opacity: 0.4; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(0.5); }
  50% { opacity: 1; transform: rotate(var(--segment-deg)) translateY(calc(-1 * var(--viz-radius))) scaleY(1); }
}
@media (min-width: 640px) {
  .intro-logo-viz-ring { --viz-radius: 8rem; }
  .intro-logo-viz-segment { height: 26px; margin-top: -13px; }
}
@media (min-width: 768px) {
  .intro-logo-viz-ring { --viz-radius: 9rem; }
  .intro-logo-viz-segment { height: 28px; margin-top: -14px; }
}

.tap-to-start-logo :deep(.logo-gradient-border) {
  width: 12rem;
  height: 12rem;
  position: relative;
  z-index: 1;
}
@media (min-width: 640px) {
  .tap-to-start-content {
    min-height: 14rem;
  }
  .tap-to-start-logo :deep(.logo-gradient-border) {
    width: 14rem;
    height: 14rem;
  }
}
@media (min-width: 768px) {
  .tap-to-start-content {
    min-height: 16rem;
  }
  .tap-to-start-logo :deep(.logo-gradient-border) {
    width: 16rem;
    height: 16rem;
  }
}
</style>

