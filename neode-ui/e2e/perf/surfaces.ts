// SURFACES table for the Phase 02 UI-performance profiling harness (02-01-PLAN.md,
// Task 1). One row per D-09 surface. Every `path` here must exist in
// `neode-ui/src/router/index.ts` (verified by acceptance criteria).
//
// `navSteps` are ordered, real UI-click selectors — clicking an <a>/<RouterLink>
// or a button that calls `router.push()` triggers genuine client-side SPA
// navigation (no full page reload), which is what "remount storm" measurement
// requires. `page.goto()` is deliberately NOT used to move between dashboard
// surfaces — it would force a full reload every time and always show
// remounted=true regardless of whether KeepAlive would have helped.
//
// `contentSelector` is chosen to be present only once real content has
// painted (never a loading skeleton/spinner). `rootSelector` is the stable
// outermost element of the mounted view, used by the remount probe in
// measure.ts. `.view-container` is a class Dashboard.vue's nested
// `<router-view>` merges onto every non-chat/non-mesh view's root element via
// Vue's automatic fallthrough-attribute merging (verified: `grep -rn
// "view-container" src` shows it added only by Dashboard.vue's default
// branch) — so it works as a uniform root selector across most surfaces.
// Mesh and Chat take Dashboard.vue's other template branch (no class
// fallthrough) so they use their own static root class instead.

export type SurfaceKind = 'main-tab' | 'secondary'

export interface Surface {
  id: string
  label: string
  /** Route path, must exist in router/index.ts. */
  path: string
  kind: SurfaceKind
  /** A selector present only once real content has painted (not a skeleton/spinner). */
  contentSelector: string
  /** The stable outermost element of the view, used for the remount probe. */
  rootSelector: string
  /**
   * Ordered selectors clicked (via real UI interaction) to reach/open this
   * surface. Every step is resolved with `.first()` and clicked; the chain
   * is re-run unchanged for the revisit measurement.
   */
  navSteps: string[]
  /**
   * When set, this surface is an in-page trigger (e.g. a modal) rather than
   * a distinct navigable route (D-09's "Wallet / send flows" — no Wallet.vue
   * exists; the real surface is the Send button on the Home dashboard wallet
   * card, opening SendBitcoinModal). Revisit is measured by clicking this to
   * close, then re-running the last `navSteps` entry to reopen — "open to
   * content" rather than "navigate to content" per the plan's fallback rule.
   */
  closeSelector?: string
}

const SIDEBAR = '[data-controller-zone="sidebar"]'

export const SURFACES: Surface[] = [
  {
    id: 'home',
    label: 'Home (wallet figures)',
    path: '/dashboard',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.home-card',
    navSteps: [`${SIDEBAR} a[href="/dashboard"]`],
  },
  {
    id: 'apps',
    label: 'Apps (My Apps)',
    path: '/dashboard/apps',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.apps-card-grid-desktop [data-controller-container]',
    navSteps: [`${SIDEBAR} a[href="/dashboard/apps"]`],
  },
  {
    id: 'marketplace',
    label: 'Marketplace (App Store)',
    path: '/dashboard/marketplace',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.marketplace-container [data-controller-container]',
    // Marketplace has no sidebar entry — reached via Home's "Browse Store" link.
    navSteps: [`${SIDEBAR} a[href="/dashboard"]`, 'a:has-text("Browse Store")'],
  },
  {
    id: 'discover',
    label: 'Discover (App Store tab)',
    path: '/dashboard/discover',
    kind: 'secondary',
    rootSelector: '.view-container',
    contentSelector: '.discover-container [data-controller-container]',
    // Discover has no sidebar entry — reached via the "App Store" tab inside Apps.
    navSteps: [`${SIDEBAR} a[href="/dashboard/apps"]`, '.apps-view a:has-text("App Store")'],
  },
  {
    id: 'cloud',
    label: 'Cloud / Files',
    path: '/dashboard/cloud',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.apps-view [data-controller-container]',
    navSteps: [`${SIDEBAR} a[href="/dashboard/cloud"]`],
  },
  {
    id: 'mesh',
    label: 'Mesh',
    path: '/dashboard/mesh',
    kind: 'main-tab',
    rootSelector: '.mesh-view',
    contentSelector: '.mesh-status-grid',
    navSteps: [`${SIDEBAR} a[href="/dashboard/mesh"]`],
  },
  {
    id: 'server',
    label: 'Server (Network)',
    path: '/dashboard/server',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.view-container [data-controller-container]',
    navSteps: [`${SIDEBAR} a[href="/dashboard/server"]`],
  },
  {
    id: 'web5',
    label: 'Web5',
    path: '/dashboard/web5',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.view-container [data-controller-container]',
    navSteps: [`${SIDEBAR} a[href="/dashboard/web5"]`],
  },
  {
    id: 'fleet',
    label: 'Fleet',
    path: '/dashboard/fleet',
    kind: 'main-tab',
    rootSelector: '.view-container',
    contentSelector: '.view-container [data-controller-container]',
    // Fleet's sidebar entry is hidden for beta — reached via Web5's Federation
    // card link. Web5Federation.vue renders TWO "Fleet" links: a
    // `.web5-card-actions-top` one that CSS permanently hides
    // (`display: none` — the "compact header variants are permanently
    // retired" rule in style.css) and the real, visible one inside
    // `.web5-card-actions-bottom-grid`. Scope to the latter so `.first()`
    // doesn't land on the hidden copy.
    navSteps: [`${SIDEBAR} a[href="/dashboard/web5"]`, '.web5-card-actions-bottom-grid a:has-text("Fleet")'],
  },
  {
    id: 'chat',
    label: 'Chat (AIUI)',
    path: '/dashboard/chat',
    kind: 'main-tab',
    rootSelector: '.chat-fullscreen',
    contentSelector: '.chat-iframe, .chat-placeholder',
    navSteps: [`${SIDEBAR} button:has-text("AIUI")`],
    // DashboardSidebar.vue is `v-show="!chatFullscreen"` — the sidebar (and
    // therefore the neutral Settings link) is not visible while on Chat, so
    // the normal "away via sidebar" step is impossible here. Chat's own close
    // button calls closeChat() (router.back(), landing on wherever we came
    // from — Home, per navSteps below), which is the surface's real "away".
    closeSelector: '.chat-close-btn',
  },
  {
    id: 'app-details',
    label: 'AppDetails (secondary)',
    path: '/dashboard/apps/:id',
    kind: 'secondary',
    rootSelector: '.view-container',
    contentSelector: '.app-details-container h1',
    navSteps: [`${SIDEBAR} a[href="/dashboard/apps"]`, '.apps-card-grid-desktop [data-controller-container]'],
  },
  {
    id: 'marketplace-app-details',
    label: 'MarketplaceAppDetails (secondary)',
    path: '/dashboard/marketplace/:id',
    kind: 'secondary',
    rootSelector: '.view-container',
    contentSelector: '.app-details-container h1',
    navSteps: [
      `${SIDEBAR} a[href="/dashboard"]`,
      'a:has-text("Browse Store")',
      '.marketplace-container [data-controller-container]',
    ],
  },
  {
    id: 'cloud-folder',
    label: 'CloudFolder (secondary)',
    path: '/dashboard/cloud/:folderId',
    kind: 'secondary',
    rootSelector: '.view-container',
    contentSelector: '.cloud-folder-container h1',
    navSteps: [`${SIDEBAR} a[href="/dashboard/cloud"]`, '.apps-view [data-controller-container]'],
  },
  {
    id: 'openwrt-gateway',
    label: 'OpenWrtGateway (secondary)',
    path: '/dashboard/server/openwrt',
    kind: 'secondary',
    rootSelector: '.view-container',
    contentSelector: 'h1:has-text("OpenWrt Gateway")',
    navSteps: [`${SIDEBAR} a[href="/dashboard/server"]`, 'a:has-text("OpenWrt Gateway")'],
  },
  {
    id: 'wallet-send',
    label: 'Wallet / send flow (Home wallet card, SendBitcoinModal)',
    // Not a route — RESEARCH.md found no Wallet.vue; the real surface is a
    // modal opened from the Home wallet card. Path recorded for reference
    // only (the page the trigger lives on).
    path: '/dashboard',
    kind: 'secondary',
    rootSelector: '[role="dialog"]',
    contentSelector: 'h3:has-text("Send Bitcoin")',
    navSteps: [`${SIDEBAR} a[href="/dashboard"]`, 'button:has-text("Send")'],
    closeSelector: '[role="dialog"] button[aria-label="Close"]',
  },
]
