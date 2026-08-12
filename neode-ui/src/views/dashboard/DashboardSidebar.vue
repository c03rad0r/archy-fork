<template>
  <aside
    v-show="!chatFullscreen"
    data-controller-zone="sidebar"
    class="hidden md:flex w-[256px] h-screen flex-shrink-0 sticky top-0 relative flex-col z-10"
    :class="{ 'sidebar-animate': showZoomIn }"
  >
    <div class="sidebar-shell">
      <div class="sidebar-inner flex flex-col h-full min-h-0">
        <div class="sidebar-logo flex items-center gap-3 mb-8 p-6 pb-0 shrink-0">
          <AnimatedLogo />
          <div class="min-w-0 flex-1">
            <h2 class="text-lg font-semibold text-white truncate">{{ serverName }}</h2>
            <p class="text-xs text-white/60">{{ $ver(version) }}</p>
          </div>
        </div>

        <nav class="sidebar-nav flex-1 min-h-0 overflow-y-auto overscroll-contain space-y-2 px-6 py-4" :aria-label="t('dashboard.mainNav')">
          <RouterLink
            v-for="(item, idx) in desktopNavItems"
            :key="item.path"
            :to="item.path"
            aria-current-value="page"
            class="sidebar-nav-item flex items-center gap-3 px-4 py-3 rounded-lg text-white/80 hover:bg-white/10 hover:text-white transition-colors"
            :class="{ 'nav-tab-active': item.isCombined && (route.path.includes('/apps') || route.path.includes('/marketplace') || route.path.includes('/discover') || route.path.includes('/app-session') || (item.path === '/dashboard/apps' && !!appLauncher.panelAppId)) }"
            :exact-active-class="item.isCombined ? undefined : 'nav-tab-active'"
            @click="appLauncher.closePanel()"
            :style="{ '--nav-stagger': idx }"
          >
            <svg v-if="item.icon === 'web5'" class="w-5 h-5" aria-hidden="true" fill="currentColor" viewBox="0 0 1631 1624">
              <path fill-rule="evenodd" clip-rule="evenodd" d="M914.932 359.228H916.229V715.252H1630.47V1088.98H1451.41V1267.98H1274.33V1445H1093.31V1624H715.534V1264.77H714.237V908.748H0V535.02H179.051V356.025H356.135V178.996H537.154V0H914.932V359.228ZM916.229 1425.33H1073.64V1248.31H1254.66V1071.28H1431.74V913.918H916.229V1425.33ZM556.83 375.695H375.811V552.723H198.727V710.082H714.237V198.666H556.83V375.695Z" />
            </svg>
            <svg v-else class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                v-for="(path, index) in getIconPath(item.icon)"
                :key="index"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                :d="path"
              />
            </svg>
            <span>{{ item.label }}</span>
            <span
              v-if="item.path === '/dashboard/web5' && web5Badge.pendingRequestCount > 0"
              class="ml-auto w-5 h-5 flex items-center justify-center rounded-full bg-orange-500 text-white text-[10px] font-bold"
            >{{ web5Badge.pendingRequestCount }}</span>
            <span
              v-if="item.path === '/dashboard/mesh' && meshStore.totalUnread > 0"
              class="ml-auto w-5 h-5 flex items-center justify-center rounded-full bg-orange-500 text-white text-[10px] font-bold"
            >{{ meshStore.totalUnread }}</span>
          </RouterLink>

          <!-- Chat launcher button -->
          <button
            @click="router.push('/dashboard/chat')"
            class="chat-launcher-btn w-full flex items-center gap-3 px-4 py-3 rounded-lg transition-all duration-300"
          >
            <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path v-for="(path, index) in getIconPath('chat')" :key="index" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="path" />
            </svg>
            <span>AIUI</span>
          </button>

          <!-- Logout - styled as nav item, below Settings -->
          <button
            @click="$emit('logout')"
            class="sidebar-logout-btn w-full flex items-center gap-3 px-4 py-3 rounded-lg text-white/80 hover:bg-white/10 hover:text-white transition-colors"
          >
            <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
            </svg>
            <span>Logout</span>
          </button>
        </nav>

        <div class="sidebar-bottom shrink-0">
          <div class="sidebar-controller px-6 pb-2">
            <ControllerIndicator />
            <CompanionIndicator />
          </div>

          <!-- Online status -->
          <div class="px-6 pb-2">
            <div class="rounded-lg bg-white/5 border border-white/10 px-4 py-2.5">
              <OnlineStatusPill />
            </div>
          </div>

          <!-- Mode switcher -->
          <div class="px-6 pb-6">
            <ModeSwitcher />
          </div>
        </div>
      </div>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink, useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { useUIModeStore } from '@/stores/uiMode'
import { useWeb5BadgeStore } from '@/stores/web5Badge'
import { useMeshStore } from '@/stores/mesh'
import AnimatedLogo from '@/components/AnimatedLogo.vue'
import OnlineStatusPill from '@/components/OnlineStatusPill.vue'
import ControllerIndicator from '@/components/ControllerIndicator.vue'
import CompanionIndicator from '@/components/CompanionIndicator.vue'
import ModeSwitcher from '@/components/ModeSwitcher.vue'

interface NavItem {
  path: string
  label: string
  icon: string
  isCombined?: boolean
}

defineProps<{
  showZoomIn: boolean
}>()

defineEmits<{
  logout: []
}>()

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const store = useAppStore()
const appLauncher = useAppLauncherStore()
const uiMode = useUIModeStore()
const web5Badge = useWeb5BadgeStore()
const meshStore = useMeshStore()

const chatFullscreen = computed(() => route.path === '/dashboard/chat')
const serverName = computed(() => store.serverName)
const version = computed(() => store.serverInfo?.version || '0.0.0')

const gamerDesktopNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'Apps', icon: 'apps', isCombined: true },
  { path: '/dashboard/cloud', label: 'Cloud', icon: 'cloud' },
  { path: '/dashboard/mesh', label: 'Mesh', icon: 'mesh' },
  { path: '/dashboard/server', label: 'Network', icon: 'server' },
  { path: '/dashboard/web5', label: 'Web5', icon: 'web5' },
  // { path: '/dashboard/fleet', label: 'Fleet', icon: 'fleet' }, // Hidden for beta
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const easyDesktopNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'My Apps', icon: 'apps' },
  { path: '/dashboard/cloud', label: 'Cloud', icon: 'cloud' },
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const chatDesktopNav: NavItem[] = [
  { path: '/dashboard', label: 'Home', icon: 'home' },
  { path: '/dashboard/apps', label: 'My Apps', icon: 'apps' },
  { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
]

const desktopNavItems = computed(() => {
  if (uiMode.isEasy) return easyDesktopNav
  if (uiMode.isChat) return chatDesktopNav
  return gamerDesktopNav
})

function getIconPath(iconName: string): string[] {
  const icons: Record<string, string[]> = {
    home: ['M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6'],
    apps: ['M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z'],
    marketplace: ['M16 11V7a4 4 0 00-8 0v4M5 9h14l1 12H4L5 9z'],
    cloud: ['M3 15a4 4 0 004 4h9a5 5 0 10-.1-9.999 5.002 5.002 0 10-9.78 2.096A4.001 4.001 0 003 15z'],
    server: ['M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01'],
    web5: ['M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9'],
    mesh: ['M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01M5.636 13.636a9 9 0 0112.728 0M1.5 10.5a14 14 0 0121 0'],
    fleet: ['M9 17V7m0 10a2 2 0 01-2 2H5a2 2 0 01-2-2V7a2 2 0 012-2h2a2 2 0 012 2m0 10a2 2 0 002 2h2a2 2 0 002-2M9 7a2 2 0 012-2h2a2 2 0 012 2m0 10V7m0 10a2 2 0 002 2h2a2 2 0 002-2V7a2 2 0 00-2-2h-2a2 2 0 00-2 2'],
    chat: ['M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z'],
    settings: [
      'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z',
      'M15 12a3 3 0 11-6 0 3 3 0 016 0z',
    ],
  }
  return icons[iconName] || []
}
</script>
