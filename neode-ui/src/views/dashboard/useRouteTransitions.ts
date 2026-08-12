import { type RouteLocationNormalizedLoaded } from 'vue-router'

/** Tab order for vertical transitions between main navigation items.
 * Exported so keepAliveRoutes.ts (and plan 02-04's widening of
 * KEEP_ALIVE_PATHS) can derive the main-tab path set from a single source
 * of truth instead of re-deriving it. */
export const TAB_ORDER = [
  '/dashboard',
  '/dashboard/apps',
  '/dashboard/marketplace',
  '/dashboard/cloud',
  '/dashboard/mesh',
  '/dashboard/server',
  '/dashboard/web5',
  '/dashboard/fleet',
  '/dashboard/chat',
  '/dashboard/settings'
]

/** Web5 group sub-tab order for mobile horizontal swipe transitions */
const WEB5_TAB_ORDER = ['/dashboard/web5', '/dashboard/cloud', '/dashboard/server', '/dashboard/mesh']

/** Route-to-background image mapping */
export const ROUTE_BACKGROUNDS: Record<string, string> = {
  '/dashboard': 'bg-home.webp',
  '/dashboard/': 'bg-home.webp',
  '/dashboard/apps': 'bg-myapps.webp',
  '/dashboard/discover': 'bg-appstore.webp',
  '/dashboard/marketplace': 'bg-appstore.webp',
  '/dashboard/cloud': 'bg-cloud.webp',
  '/dashboard/mesh': 'bg-mesh.webp',
  '/dashboard/server': 'bg-network.jpg',
  '/dashboard/web5': 'bg-web5.jpg',
  '/dashboard/server/federation': 'bg-web5.jpg',
  '/dashboard/monitoring': 'bg-web5.jpg',
  '/dashboard/fleet': 'bg-web5.jpg',
  '/dashboard/settings': 'bg-settings.webp',
  '/dashboard/chat': 'bg-aiui.jpg',
}

export function isDetailRoute(path: string): boolean {
  return (path.includes('/apps/') && !path.endsWith('/apps')) ||
    (path.includes('/marketplace/') && !path.endsWith('/marketplace'))
}

/**
 * Creates a route transition tracker that determines the appropriate
 * CSS transition name based on navigation direction and route depth.
 */
export function useRouteTransitions() {
  let previousPath = ''
  let previousTab = ''

  function getTransitionName(currentRoute: RouteLocationNormalizedLoaded): string {
    const currentPath = currentRoute.path

    if (!previousPath) {
      previousPath = currentPath
      return 'fade'
    }

    // Chat transitions: directional slide
    const isChat = currentPath === '/dashboard/chat'
    const wasChat = previousPath === '/dashboard/chat'
    if (isChat) {
      previousPath = currentPath
      return 'chat-open'
    }
    if (wasChat) {
      previousPath = currentPath
      return 'chat-close'
    }

    const isAppDetails = currentPath.includes('/apps/') && !currentPath.endsWith('/apps')
    const isAppsList = currentPath === '/dashboard/apps'
    const wasAppDetails = previousPath.includes('/apps/') && !previousPath.endsWith('/apps')
    const wasAppsList = previousPath === '/dashboard/apps'

    const isMarketplaceDetails = currentPath.includes('/marketplace/') && !currentPath.endsWith('/marketplace')
    const isMarketplaceList = currentPath === '/dashboard/marketplace'
    const wasMarketplaceDetails = previousPath.includes('/marketplace/') && !previousPath.endsWith('/marketplace')
    const wasMarketplaceList = previousPath === '/dashboard/marketplace'

    const isCloudFolder = currentPath.includes('/cloud/') && !currentPath.endsWith('/cloud')
    const isCloudList = currentPath === '/dashboard/cloud'
    const wasCloudFolder = previousPath.includes('/cloud/') && !previousPath.endsWith('/cloud')
    const wasCloudList = previousPath === '/dashboard/cloud'

    const isFederation = currentPath === '/dashboard/server/federation'
    const wasFederation = previousPath === '/dashboard/server/federation'
    const isMonitoring = currentPath === '/dashboard/monitoring'
    const wasMonitoring = previousPath === '/dashboard/monitoring'
    const isFleet = currentPath === '/dashboard/fleet'
    const wasFleet = previousPath === '/dashboard/fleet'
    const isWeb5 = currentPath === '/dashboard/web5'
    const wasWeb5 = previousPath === '/dashboard/web5'
    // Any Web5 sub-detail (networking-profits, credentials, …) animates as a
    // depth push from/back-to the Web5 tab — same feel as Find Nodes.
    const isWeb5Detail = currentPath.startsWith('/dashboard/web5/')
    const wasWeb5Detail = previousPath.startsWith('/dashboard/web5/')

    let transitionName = 'fade'

    // Mobile: Horizontal slide transitions between sub-tabs
    if (typeof window !== 'undefined' && window.innerWidth < 768) {
      const isServices = currentPath === '/dashboard/apps' && (currentRoute.query.tab === 'services' || currentRoute.query.tab === 'websites')
      const wasServices = previousTab === 'services' || previousTab === 'websites'
      const currentAppsIdx = isServices ? 2
        : currentPath === '/dashboard/marketplace' ? 1
        : currentPath === '/dashboard/apps' ? 0 : -1
      const prevAppsIdx = wasServices ? 2
        : previousPath === '/dashboard/marketplace' ? 1
        : previousPath === '/dashboard/apps' ? 0 : -1

      const currentWeb5Idx = WEB5_TAB_ORDER.indexOf(currentPath)
      const prevWeb5Idx = WEB5_TAB_ORDER.indexOf(previousPath)

      if (currentAppsIdx !== -1 && prevAppsIdx !== -1 && currentAppsIdx !== prevAppsIdx) {
        transitionName = currentAppsIdx > prevAppsIdx ? 'slide-left' : 'slide-right'
      } else if (currentWeb5Idx !== -1 && prevWeb5Idx !== -1 && currentWeb5Idx !== prevWeb5Idx) {
        transitionName = currentWeb5Idx > prevWeb5Idx ? 'slide-left' : 'slide-right'
      } else {
        const currentIndex = TAB_ORDER.indexOf(currentPath)
        const previousIndex = TAB_ORDER.indexOf(previousPath)

        if (currentIndex !== -1 && previousIndex !== -1 && currentIndex !== previousIndex) {
          transitionName = currentIndex > previousIndex ? 'slide-down' : 'slide-up'
        }
      }
    }
    // Desktop depth transitions: list <-> detail
    else if (wasAppsList && isAppDetails) {
      transitionName = 'depth-forward'
    } else if (wasAppDetails && isAppsList) {
      transitionName = 'depth-back'
    } else if (wasMarketplaceList && isMarketplaceDetails) {
      transitionName = 'depth-forward'
    } else if (wasMarketplaceDetails && isMarketplaceList) {
      transitionName = 'depth-back'
    } else if (wasCloudList && isCloudFolder) {
      transitionName = 'depth-forward'
    } else if (wasCloudFolder && isCloudList) {
      transitionName = 'depth-back'
    } else if (wasWeb5 && isFederation) {
      transitionName = 'depth-forward'
    } else if (wasFederation && isWeb5) {
      transitionName = 'depth-back'
    } else if (wasWeb5 && isMonitoring) {
      transitionName = 'depth-forward'
    } else if (wasMonitoring && isWeb5) {
      transitionName = 'depth-back'
    } else if (wasWeb5 && isFleet) {
      transitionName = 'depth-forward'
    } else if (wasFleet && isWeb5) {
      transitionName = 'depth-back'
    } else if (wasWeb5 && isWeb5Detail) {
      transitionName = 'depth-forward'
    } else if (wasWeb5Detail && isWeb5) {
      transitionName = 'depth-back'
    } else if (wasMarketplaceList && isAppDetails) {
      transitionName = 'depth-forward'
    } else if (wasAppDetails && isMarketplaceList) {
      transitionName = 'depth-back'
    }
    // Desktop: no transition between Apps <-> Marketplace (same-page tab feel)
    else if ((wasAppsList && isMarketplaceList) || (wasMarketplaceList && isAppsList)) {
      transitionName = 'fade'
    }
    // Vertical transition: between main tabs (desktop)
    else {
      const currentIndex = TAB_ORDER.indexOf(currentPath)
      const previousIndex = TAB_ORDER.indexOf(previousPath)

      if (currentIndex !== -1 && previousIndex !== -1 && currentIndex !== previousIndex) {
        transitionName = currentIndex > previousIndex ? 'slide-down' : 'slide-up'
      }
    }

    previousPath = currentPath
    previousTab = (currentRoute.query.tab as string) || ''

    return transitionName
  }

  return { getTransitionName }
}
