<template>
  <div class="onb-viewport relative overflow-hidden">
    <!-- Background layers with 3D perspective and zoom effect -->
    <div class="bg-perspective-container">
      <!-- Video background for intro/login routes (smooth transition from splash) -->
      <video
        v-if="useVideoBackground"
        ref="videoElement"
        class="bg-layer"
        autoplay
        loop
        muted
        playsinline
        preload="auto"
        poster="/assets/img/bg-intro.jpg"
        style="width: 100%; height: 100%; object-fit: cover; object-position: center; position: absolute; inset: 0; transform: scale(1); transition: none;"
        @pause.prevent="handleVideoPause"
        @ended="handleVideoEnded"
      >
        <source src="/assets/video/video-intro.mp4?v=8" type="video/mp4">
      </video>
      
      <!-- Login: static background + archipelago-style glitch (no zoom) -->
      <template v-else-if="isLoginRoute">
        <div
          class="bg-layer bg-login-static bg-fullwidth"
          :style="{ backgroundImage: `url('/assets/img/${loginBackground}')` }"
        />
        <!-- Archipelago-style glitch overlays - continuous every 5s -->
        <div class="login-glitch-layer login-glitch-1" :style="{ backgroundImage: `url('/assets/img/${loginBackground}')` }" />
        <div class="login-glitch-layer login-glitch-2" :style="{ backgroundImage: `url('/assets/img/${loginBackground}')` }" />
        <div class="login-glitch-scan" />
      </template>
      
      <!-- Static image background for other routes (with zoom on transition) -->
      <div 
        v-else
        class="bg-layer bg-zoom"
        :class="{ 'bg-zoom-in': isTransitioning }"
        :style="{ backgroundImage: `url('/assets/img/${currentBackground}')` }"
        :key="currentBackground"
      ></div>
      
      <!-- Glitch overlay layer - only for non-video, non-login background changes -->
      <div v-show="isGlitching && !useVideoBackground && !isLoginRoute" class="bg-glitch-layer"></div>
    </div>

    <!-- Content with 3D transitions -->
    <div class="perspective-container-wrapper">
      <div class="perspective-container">
        <!-- #145: fresh install with no cable — point at the WiFi picker. -->
        <!-- Onboarding routes only: logged-in sessions are offline-first and
             must never be nagged about connectivity. -->
        <OnboardingNetworkCallout v-if="isOnboardingRoute" />
        <RouterView v-slot="{ Component, route }">
          <Transition :name="transitionName">
            <div :key="route.path" class="view-wrapper">
              <component :is="Component" class="view-container" />
            </div>
          </Transition>
        </RouterView>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { useRoute } from 'vue-router'
import { resumeAudioContext, startSynthwave } from '@/composables/useLoginSounds'
import OnboardingNetworkCallout from '@/components/OnboardingNetworkCallout.vue'

const route = useRoute()
const currentBackground = ref('bg-intro.jpg')

// #145: the no-network callout follows the user across onboarding steps, but
// this wrapper also hosts /login (and could host more later) — scope the
// callout to the onboarding flow only.
const isOnboardingRoute = computed(() => route.path.startsWith('/onboarding/'))
const isGlitching = ref(false)
const isTransitioning = ref(false)
const videoElement = ref<HTMLVideoElement | null>(null)
const transitionName = ref('depth-forward')

// Ordered onboarding steps for direction detection
const onboardingOrder = [
  '/onboarding/intro', '/onboarding/path', '/onboarding/options',
  '/onboarding/seed', '/onboarding/seed-verify', '/onboarding/seed-restore',
  '/onboarding/did', '/onboarding/identity', '/onboarding/backup',
  '/onboarding/verify', '/onboarding/done', '/login'
]

// Routes that should use video background (smooth transition from splash, loops through login)
const videoBackgroundRoutes = ['/onboarding/intro', '/login']

// Login uses video when coming from splash, or static + glitch when direct
const isLoginRoute = computed(() => route.path === '/login')


// Check if current route should use video background.
// The FIRST login (nobody has logged in / set a password yet) keeps the VIDEO
// running so the splash → login handoff is one continuous shot; only after a
// successful login does /login switch to the rotating static backgrounds.
// Login.vue sets neode_first_login_done on every successful login.
const hasLoggedInBefore = () => {
  try {
    return localStorage.getItem('neode_first_login_done') === '1'
  } catch {
    return false
  }
}
const useVideoBackground = computed(() => {
  if (!videoBackgroundRoutes.includes(route.path)) return false
  if (route.path === '/login' && hasLoggedInBefore()) return false
  return true
})

// Map each route to a specific background image
// Note: bg-intro.jpg is used for splash and /onboarding/intro for seamless transition
const routeBackgrounds: Record<string, string> = {
  '/onboarding/intro': 'bg-intro.jpg', // Video will be used instead
  '/onboarding/options': 'bg-intro-4.webp',
  '/onboarding/path': 'bg-intro-3.jpg',
  '/onboarding/seed': 'bg-intro-1.webp',
  '/onboarding/seed-verify': 'bg-intro-1.webp',
  '/onboarding/seed-restore': 'bg-intro-1.webp',
  '/onboarding/did': 'bg-intro-4.webp',
  '/onboarding/identity': 'bg-intro-1.webp',
  '/onboarding/backup': 'bg-intro-6.webp',
  '/onboarding/verify': 'bg-intro-2.jpg',
  '/onboarding/done': 'bg-intro-1.webp',
  '/login': 'bg-intro.jpg' // Video loops from splash (same as intro)
}

// Until the user has logged in (or set their password) once, the login
// background stays PINNED to the same still the intro video ends on — the
// video → login handoff only reads as one continuous shot when both show the
// identical frame. From the second login onward the lock screen rotates
// through bg-intro-1..6 for variety (counter persisted in localStorage).
// Login.vue sets neode_first_login_done on every successful login.
const LOGIN_BACKGROUNDS = [
  'bg-intro-1.webp',
  'bg-intro-2.jpg',
  'bg-intro-3.jpg',
  'bg-intro-4.webp',
  'bg-intro-5.webp',
  'bg-intro-6.webp',
]
function pickNextLoginBackground(): string {
  try {
    if (localStorage.getItem('neode_first_login_done') !== '1') return 'bg-intro.jpg'
    const raw = localStorage.getItem('neode_login_bg_idx')
    const prev = raw !== null ? parseInt(raw, 10) : -1
    const next = (Number.isFinite(prev) ? prev + 1 : 0) % LOGIN_BACKGROUNDS.length
    localStorage.setItem('neode_login_bg_idx', String(next))
    return LOGIN_BACKGROUNDS[next]!
  } catch {
    return 'bg-intro.jpg'
  }
}
const loginBackground = ref(pickNextLoginBackground())
watch(() => route.path, (p) => {
  if (p === '/login') loginBackground.value = pickNextLoginBackground()
})

// Restore video time from splash screen for seamless transition
function restoreVideoTime() {
  if (videoElement.value && useVideoBackground.value) {
    const savedTime = sessionStorage.getItem('video_intro_currentTime')
    const wasPlaying = sessionStorage.getItem('video_intro_wasPlaying') === 'true'
    const savedPlaybackRate = sessionStorage.getItem('video_intro_playbackRate')
    
    if (savedTime) {
      const time = parseFloat(savedTime)
      const playbackRate = savedPlaybackRate ? parseFloat(savedPlaybackRate) : 1.0
      
      const setVideoTime = () => {
        if (videoElement.value) {
          // Set playback rate first for smooth playback
          videoElement.value.playbackRate = playbackRate
          // Set time with slight offset to ensure smooth transition (avoid frame boundary issues)
          videoElement.value.currentTime = Math.max(0, time - 0.05)
          
          // If video was playing, ensure it continues playing immediately
          if (wasPlaying) {
            requestAnimationFrame(() => ensureVideoPlaying())
          }
          
          // Clean up session storage after successful restore
          sessionStorage.removeItem('video_intro_currentTime')
          sessionStorage.removeItem('video_intro_wasPlaying')
          sessionStorage.removeItem('video_intro_playbackRate')
        }
      }
      
      if (videoElement.value.readyState >= 2) {
        // Video can play through current position, set time immediately
        setVideoTime()
      } else if (videoElement.value.readyState >= 1) {
        // Video metadata loaded, set time immediately
        setVideoTime()
      } else {
        // Wait for metadata to load
        const handleLoadedMetadata = () => {
          setVideoTime()
          if (videoElement.value) {
            videoElement.value.removeEventListener('loadedmetadata', handleLoadedMetadata)
          }
        }
        videoElement.value.addEventListener('loadedmetadata', handleLoadedMetadata, { once: true })
      }
    }
  }
}

// Video play with retry - hardened for reliability
function ensureVideoPlaying(retries = 3): void {
  const vid = videoElement.value
  if (!vid || !useVideoBackground.value) return
  if (!vid.paused) return
  vid.play()
    .then(() => {})
    .catch(() => {
      if (retries > 0) setTimeout(() => ensureVideoPlaying(retries - 1), 300)
    })
}

// Ensure video plays when route uses video background
watch([useVideoBackground, route], ([useVideo]) => {
  if (useVideo && videoElement.value) {
    // Use requestAnimationFrame for smoother transition
    requestAnimationFrame(() => {
      if (videoElement.value) {
        // Restore video time for seamless transition first
        restoreVideoTime()
        // Then ensure it's playing - use double RAF for smoother transition
        requestAnimationFrame(() => {
          if (videoElement.value && videoElement.value.paused) {
            ensureVideoPlaying()
          }
        })
      }
    })
  }
}, { immediate: true })

// Also ensure video plays on mount if route uses video
onMounted(() => {
  if (useVideoBackground.value && videoElement.value) {
    // Use requestAnimationFrame for smoother transition
    requestAnimationFrame(() => {
      if (videoElement.value) {
        // Restore video time for seamless transition
        restoreVideoTime()
        // Use double RAF for smoother playback start
        requestAnimationFrame(() => {
          if (videoElement.value) {
            ensureVideoPlaying()
          }
        })
      }
    })
  }
})

// Watch for video element to restore time when it becomes available
watch(videoElement, (element) => {
  if (element && useVideoBackground.value) {
    // Use requestAnimationFrame for smoother transition
    requestAnimationFrame(() => {
      if (element) {
        // Try to restore immediately if metadata already loaded
        if (element.readyState >= 2) {
          restoreVideoTime()
        } else if (element.readyState >= 1) {
          restoreVideoTime()
        } else {
          // Wait for metadata to load
          const handleLoadedMetadata = () => {
            restoreVideoTime()
            element.removeEventListener('loadedmetadata', handleLoadedMetadata)
          }
          element.addEventListener('loadedmetadata', handleLoadedMetadata, { once: true })
        }
      }
    })
  }
})

// Watch route changes for background swaps, zoom, glitch, and transition direction
watch(() => route.path, (newPath, oldPath) => {
  // Determine slide direction based on route order
  const oldIdx = onboardingOrder.indexOf(oldPath || '')
  const newIdx = onboardingOrder.indexOf(newPath)
  if (oldIdx >= 0 && newIdx >= 0) {
    transitionName.value = newIdx >= oldIdx ? 'slide-left' : 'slide-right'
  } else {
    transitionName.value = 'depth-forward'
  }
  const newBg = routeBackgrounds[newPath]
  const oldUsesVideo = videoBackgroundRoutes.includes(oldPath || '')
  const newUsesVideo = videoBackgroundRoutes.includes(newPath)
  
  // If both old and new routes use video, don't restart video - keep it playing seamlessly
  if (oldUsesVideo && newUsesVideo && videoElement.value) {
    // Video continues seamlessly, just ensure it's playing
    if (videoElement.value.paused) {
      ensureVideoPlaying()
    }
    // No glitch effect, no zoom, no transitions for video-to-video
    isGlitching.value = false
    isTransitioning.value = false
    return // Skip background change logic for video-to-video transitions
  }
  
  // If transitioning from video to non-video or vice versa, no glitch, no zoom (smooth transition)
  if (oldUsesVideo || newUsesVideo) {
    isGlitching.value = false
    isTransitioning.value = false
  }
  
  // Login route: set background immediately, no zoom, no transition (glitch is always-on)
  if (newPath === '/login') {
    currentBackground.value = 'bg-intro-1.webp'
    isTransitioning.value = false
    isGlitching.value = false
    return
  }
  
  // Only update if we have a defined background for this route and it's different
  if (newBg && newBg !== currentBackground.value) {
    // Trigger zoom animation ONLY for non-video routes (never for video)
    if (!newUsesVideo && !oldUsesVideo) {
      isTransitioning.value = true
      
      // Change background
      currentBackground.value = newBg
      
      // Only trigger glitch for non-video background changes
      setTimeout(() => {
        isGlitching.value = true
        setTimeout(() => {
          isGlitching.value = false
        }, 500) // Match glitch duration
        
        // Reset zoom after glitch
        isTransitioning.value = false
      }, 1500 + 50) // Wait for 3D transition (1500ms) + small delay - matches splash timing
    } else {
      // Smooth transition for video routes - no glitch, no zoom, no effects at all
      currentBackground.value = newBg
      isTransitioning.value = false
      isGlitching.value = false
    }
  }
})

// Prevent video from pausing during transitions
function handleVideoPause(event: Event) {
  if (useVideoBackground.value && videoElement.value) {
    event.preventDefault()
    ensureVideoPlaying()
  }
}

// Handle video ended - restart immediately for seamless loop
function handleVideoEnded() {
  if (useVideoBackground.value && videoElement.value) {
    videoElement.value.currentTime = 0
    ensureVideoPlaying()
  }
}

// Update body class to disable global glitch effects ONLY for video backgrounds
// This class is ONLY added on /onboarding/intro (login uses its own glitch)
// All other routes will have glitch effects enabled (normal behavior)
watch(useVideoBackground, (usesVideo) => {
  if (usesVideo) {
    // Add class ONLY on video background screens (/onboarding/intro, /login)
    // This disables glitch effects ONLY on these screens
    document.body.classList.add('video-background-active')
  } else {
    // Remove class on all other screens to re-enable glitch effects
    document.body.classList.remove('video-background-active')
  }
}, { immediate: true })

// Initialize background on mount based on current route
onMounted(() => {
  const bg = routeBackgrounds[route.path]
  if (bg) {
    currentBackground.value = bg
  }
  
  if (useVideoBackground.value) {
    isTransitioning.value = false
    isGlitching.value = false
    document.body.classList.add('video-background-active')
    const unlock = () => {
      resumeAudioContext()
      if (sessionStorage.getItem('archipelago_from_splash') !== '1') {
        startSynthwave()
      }
      document.removeEventListener('click', unlock)
      document.removeEventListener('touchstart', unlock)
      document.removeEventListener('keydown', unlock)
    }
    document.addEventListener('click', unlock, { once: true })
    document.addEventListener('touchstart', unlock, { once: true })
    document.addEventListener('keydown', unlock, { once: true })
  }
})
</script>

<style scoped>
/* Fixed viewport container — locks to screen on mobile, no bounce/overflow */
.onb-viewport {
  height: 100vh;
  height: 100dvh;
  width: 100%;
  position: fixed;
  inset: 0;
}

/* Wrapper to contain perspective without clipping */
.perspective-container-wrapper {
  position: relative;
  overflow: hidden;
  height: 100%;
}

/* Perspective container for 3D depth effect */
.perspective-container {
  perspective: 1200px;
  perspective-origin: 50% 50%;
  position: relative;
  height: 100%;
  overflow: hidden;
}

/* View wrapper - allows smooth transitions with absolute positioning */
.view-wrapper {
  position: absolute;
  inset: 0;
  transform-style: preserve-3d;
  backface-visibility: hidden;
  will-change: transform, opacity;
  overflow: hidden;
}

.view-container {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  -webkit-overflow-scrolling: touch;
}

/* 2advanced-style: fluid depth transitions */
.depth-forward-enter-active.view-wrapper,
.depth-forward-leave-active.view-wrapper {
  transition: all 0.9s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}

.depth-forward-enter-from.view-wrapper {
  opacity: 0;
  transform: translateZ(-1200px) scale(0.6);
  filter: blur(10px);
}

.depth-forward-enter-to.view-wrapper {
  opacity: 1;
  transform: translateZ(0) scale(1);
  filter: blur(0px);
}

.depth-forward-leave-from.view-wrapper {
  opacity: 1;
  transform: translateZ(0) scale(1);
  filter: blur(0px);
}

.depth-forward-leave-to.view-wrapper {
  opacity: 0;
  transform: translateZ(500px) scale(1.25);
  filter: blur(10px);
}

/* Horizontal slide transitions (direction-aware onboarding steps) */
.slide-left-enter-active.view-wrapper,
.slide-left-leave-active.view-wrapper,
.slide-right-enter-active.view-wrapper,
.slide-right-leave-active.view-wrapper {
  transition: transform 0.4s cubic-bezier(0.25, 0.46, 0.45, 0.94), opacity 0.4s ease;
}

.slide-left-enter-from.view-wrapper {
  opacity: 0;
  transform: translateX(60px);
}
.slide-left-leave-to.view-wrapper {
  opacity: 0;
  transform: translateX(-60px);
}
.slide-right-enter-from.view-wrapper {
  opacity: 0;
  transform: translateX(-60px);
}
.slide-right-leave-to.view-wrapper {
  opacity: 0;
  transform: translateX(60px);
}

/* Background zoom - 2advanced fluid */
.bg-zoom {
  transition: transform 1.8s cubic-bezier(0.25, 0.46, 0.45, 0.94);
  transform: scale(1);
}

.bg-zoom-in {
  transform: scale(1.15);
}

/* Subtle 3D tilt - 2advanced layered depth */
@media (min-width: 768px) {
  .depth-forward-enter-from.view-wrapper {
    transform: translateZ(-1200px) scale(0.6) rotateX(6deg);
  }
  
  .depth-forward-leave-to.view-wrapper {
    transform: translateZ(500px) scale(1.25) rotateX(-4deg);
  }
}

/* Background 3D container */
.bg-perspective-container {
  position: fixed;
  inset: 0;
  perspective: 1000px;
  perspective-origin: 50% 50%;
  z-index: -10;
  overflow: hidden;
  min-width: 100vw;
  width: 100vw;
}

/* Full width background on every screen */
.bg-fullwidth {
  min-width: 100vw;
  width: 100vw;
  background-size: cover;
  background-position: center center;
}

.bg-layer {
  position: absolute;
  inset: 0;
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
  transform-style: preserve-3d;
  backface-visibility: hidden;
}

/* Video background styling - video element itself has bg-layer class */
.bg-layer video,
video.bg-layer {
  width: 100%;
  height: 100%;
  object-fit: cover;
  object-position: center;
  position: absolute;
  inset: 0;
}

.bg-static {
  opacity: 1;
  transform: translateZ(0) scale(1);
}

/* Login: static background - just there, no zoom */
.bg-login-static {
  opacity: 1;
  transform: none;
}

/* Archipelago-style glitch overlays for login - continuous every 5s */
.login-glitch-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 5;
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
  opacity: 0;
}

.login-glitch-1 {
  mix-blend-mode: screen;
  filter: hue-rotate(22deg) saturate(1.35);
  animation: login-glitch-shift 5s steps(10, end) infinite;
}

.login-glitch-2 {
  mix-blend-mode: screen;
  filter: hue-rotate(-30deg) saturate(1.45);
  animation: login-glitch-shift-2 5s steps(9, end) infinite;
}

.login-glitch-scan {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 6;
  background:
    linear-gradient(180deg, rgba(255,255,255,0.16), rgba(0,0,0,0) 60%),
    repeating-linear-gradient(180deg, rgba(255,255,255,0.05) 0 2px, rgba(0,0,0,0) 2px 4px),
    radial-gradient(ellipse at center, rgba(0,0,0,0) 40%, rgba(0,0,0,0.35) 100%);
  opacity: 0;
  animation: login-glitch-scan 5s ease-out infinite;
}

@keyframes login-glitch-shift {
  0%, 82% { transform: translate(0,0); clip-path: inset(0% 0 0 0); opacity: 0; }
  82.1% { opacity: 0.22; }
  84% { transform: translate(6px,-2px); clip-path: inset(8% 0 70% 0); }
  86% { transform: translate(-5px,2px); clip-path: inset(42% 0 40% 0); }
  88% { transform: translate(3px,0); clip-path: inset(68% 0 10% 0); }
  91% { transform: translate(-4px,3px); clip-path: inset(18% 0 60% 0); }
  93% { transform: translate(5px,-3px); clip-path: inset(55% 0 20% 0); }
  95% { transform: translate(-3px,1px); clip-path: inset(10% 0 80% 0); }
  100% { transform: translate(0,0); clip-path: inset(0% 0 0 0); opacity: 0; }
}

@keyframes login-glitch-shift-2 {
  0%, 82% { transform: translate(0,0); clip-path: inset(0% 0 0 0); opacity: 0; }
  82.1% { opacity: 0.18; }
  84% { transform: translate(-6px,2px); clip-path: inset(12% 0 65% 0); }
  86% { transform: translate(5px,-1px) skewX(0.6deg); clip-path: inset(36% 0 42% 0); }
  89% { transform: translate(-3px,2px); clip-path: inset(72% 0 8% 0); }
  92% { transform: translate(4px,-3px); clip-path: inset(22% 0 58% 0); }
  95% { transform: translate(-4px,1px); clip-path: inset(50% 0 26% 0); }
  100% { transform: translate(0,0); clip-path: inset(0% 0 0 0); opacity: 0; }
}

@keyframes login-glitch-scan {
  0%, 82% { opacity: 0; transform: translateY(-20%); }
  84% { opacity: 0.4; }
  90% { opacity: 0.28; }
  100% { opacity: 0; transform: translateY(115%); }
}

/* Glitch overlay layer */
.bg-glitch-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 10;
  background: rgba(255, 255, 255, 0.1);
  mix-blend-mode: overlay;
  opacity: 0;
  animation: bg-glitch-flash 500ms ease-in-out;
}

@keyframes bg-glitch-flash {
  0%, 100% {
    opacity: 0;
    transform: translateX(0);
  }
  10% {
    opacity: 0.3;
    transform: translateX(-3px);
  }
  20% {
    opacity: 0;
    transform: translateX(3px);
  }
  30% {
    opacity: 0.4;
    transform: translateX(-2px);
  }
  40% {
    opacity: 0;
    transform: translateX(2px);
  }
  50% {
    opacity: 0.2;
    transform: translateX(-1px);
  }
  60% {
    opacity: 0;
    transform: translateX(1px);
  }
}

/* Kiosk (software compositor): flatten THIS screen's background stack.
   Same failure mode the dashboard kiosk fix addressed — perspective +
   preserve-3d layers and mix-blend-mode overlays fail to repaint under
   Chromium software compositing, so the black body fill shows through.
   The dashboard-only overrides never reached this scoped stack, which is
   why the kiosk login/onboarding background still went black. Keep 2D
   opacity crossfades; drop 3D transforms, blur filters, and blend-mode
   glitch overlays. */
/* The full selector must live inside :global() — with `:global(html.kiosk-mode)
   .bg-layer` the SFC compiler drops the descendant part, emitting bare
   `html.kiosk-mode { display: none !important }` rules that blank the whole
   document on kiosk (the v1.7.104 white-screen). */
:global(html.kiosk-mode .bg-perspective-container),
:global(html.kiosk-mode .perspective-container) {
  perspective: none !important;
}
/* Scoped to the onboarding viewport: a bare `.view-wrapper` match also killed
   the dashboard's route transitions on kiosk (transform:none !important beats
   the transition classes), freezing every tab change. The dashboard has its
   own kiosk-safe 2D transition overrides in dashboard-styles.css. */
:global(html.kiosk-mode .bg-layer),
:global(html.kiosk-mode .onb-viewport .view-wrapper) {
  transform: none !important;
  transform-style: flat !important;
  backface-visibility: visible !important;
  will-change: auto !important;
  filter: none !important;
}
:global(html.kiosk-mode .login-glitch-layer),
:global(html.kiosk-mode .login-glitch-scan) {
  display: none !important;
}
</style>

