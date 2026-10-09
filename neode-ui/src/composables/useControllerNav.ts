/**
 * Controller / gamepad navigation for Archipelago.
 *
 * Navigation model (from the design spec):
 *
 * SIDEBAR (vertical list):
 *   Up/Down = move between items, wraps top↔bottom, auto-navigates
 *   Right   = jump to first container in main content
 *   Left    = does nothing
 *
 * MAIN CONTENT (container tile grid):
 *   Arrows  = move between containers spatially (the red tile grid)
 *   Enter   = trigger container's primary action (navigate link / launch)
 *   Escape  = back to sidebar
 *   Left from leftmost container = back to sidebar
 *
 * INSIDE CONTAINER (yellow inner controls — entered via second Enter):
 *   Arrows  = move between inner controls spatially
 *   Escape  = exit back to the container tile
 *   Cannot move to other containers without exiting first
 *
 * TEXT INPUTS:
 *   Up/Down = exit field, navigate to nearest element
 *   Enter   = submit (click next button)
 *   Left/Right = cursor movement (stay in field)
 */

import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useControllerStore } from '@/stores/controller'
import { useSpotlightStore } from '@/stores/spotlight'
import { useCLIStore } from '@/stores/cli'
import { useAppLauncherStore } from '@/stores/appLauncher'
import { playNavSound } from '@/composables/useNavSounds'

// ─── Element Queries ────────────────────────────────────────────

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
  '[data-controller-focus]',
  '[data-controller-container]',
].join(', ')

function getFocusableElements(root: Document | HTMLElement = document): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    el =>
      !el.hasAttribute('disabled') &&
      el.offsetParent !== null &&
      !el.hasAttribute('data-controller-ignore') &&
      !el.closest('[data-controller-ignore]')
  )
}

/** Sidebar items */
function getSidebarElements(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="sidebar"]') as HTMLElement | null
  return zone ? getFocusableElements(zone) : []
}

/** Main zone containers only — the [C] tile grid */
function getContainers(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
  if (!zone) return []
  return Array.from(zone.querySelectorAll<HTMLElement>('[data-controller-container]')).filter(
    el => el.offsetParent !== null
  )
}

/** Nav bar items [N] — focusable elements in main zone that are NOT inside any container
 *  (mode-switcher buttons, tab buttons, search inputs above the grid) */
function getNavBarItems(): HTMLElement[] {
  const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
  if (!zone) return []
  return getFocusableElements(zone).filter(el =>
    !el.hasAttribute('data-controller-container') &&
    !el.closest('[data-controller-container]')
  )
}

function isNavBarItem(el: HTMLElement | null): boolean {
  if (!el) return false
  if (!isInZone(el, 'main')) return false
  if (el.hasAttribute('data-controller-container') || el.closest('[data-controller-container]')) return false
  // On container-free pages (e.g. Settings), don't classify elements as nav bar items —
  // let them fall through to the main zone handler which supports linear up/down/right nav.
  const zone = document.querySelector('[data-controller-zone="main"]')
  if (zone && !zone.querySelector('[data-controller-container]')) return false
  return true
}

/** Inner focusables within a container (buttons, links — not the container itself) */
function getInnerFocusables(container: HTMLElement): HTMLElement[] {
  return getFocusableElements(container).filter(
    el => el !== container && !el.hasAttribute('data-controller-container')
  )
}

function isInZone(el: HTMLElement | null, zone: 'sidebar' | 'main'): boolean {
  if (!el) return false
  return !!el.closest(`[data-controller-zone="${zone}"]`)
}

/** Topmost open modal dialog, if any — it owns navigation while visible. */
function getOpenModal(): HTMLElement | null {
  const dialogs = Array.from(
    document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]'),
  ).filter(el => el.offsetParent !== null)
  return dialogs[dialogs.length - 1] ?? null
}

function isInsideContainer(el: HTMLElement | null): boolean {
  if (!el) return false
  const container = el.closest('[data-controller-container]')
  return !!container && container !== el
}

function isContainer(el: HTMLElement | null): boolean {
  return !!el?.hasAttribute('data-controller-container')
}

// ─── Spatial Navigation ─────────────────────────────────────────

function findNearestInDirection(
  from: HTMLElement,
  candidates: HTMLElement[],
  direction: 'up' | 'down' | 'left' | 'right'
): HTMLElement | null {
  const fromRect = from.getBoundingClientRect()
  const fromCX = fromRect.left + fromRect.width / 2
  const fromCY = fromRect.top + fromRect.height / 2
  const threshold = 50

  const filtered = candidates.filter(el => {
    if (el === from) return false
    const r = el.getBoundingClientRect()
    switch (direction) {
      case 'left':  return r.right <= fromRect.left + threshold
      case 'right': return r.left >= fromRect.right - threshold
      case 'up':    return r.bottom <= fromRect.top + threshold
      case 'down':  return r.top >= fromRect.bottom - threshold
    }
  })

  if (!filtered.length) return null

  const scored = filtered.map(el => {
    const r = el.getBoundingClientRect()
    const cx = r.left + r.width / 2
    const cy = r.top + r.height / 2
    const isVertical = direction === 'up' || direction === 'down'
    const overlap = isVertical
      ? Math.max(0, Math.min(fromRect.right, r.right) - Math.max(fromRect.left, r.left))
      : Math.max(0, Math.min(fromRect.bottom, r.bottom) - Math.max(fromRect.top, r.top))
    const dist = isVertical ? Math.abs(cy - fromCY) : Math.abs(cx - fromCX)
    return { el, overlap, dist }
  })

  scored.sort((a, b) => {
    const isVertical = direction === 'up' || direction === 'down'
    // For vertical nav: prefer closest element first, use overlap as tiebreaker.
    // This prevents a distant full-width element from winning over a closer narrow one.
    if (isVertical) {
      // Both have overlap — prefer closer distance
      if (a.overlap > 0 && b.overlap > 0) {
        if (a.dist !== b.dist) return a.dist - b.dist
        if (b.overlap !== a.overlap) return b.overlap - a.overlap
        return a.el.getBoundingClientRect().left - b.el.getBoundingClientRect().left
      }
      // One has overlap, the other doesn't — prefer the one with overlap
      if (a.overlap !== b.overlap) return b.overlap - a.overlap
      return a.dist - b.dist
    }
    // Horizontal: overlap first (same row), then distance
    if (b.overlap !== a.overlap) return b.overlap - a.overlap
    return a.dist - b.dist
  })

  return scored[0]?.el ?? null
}

// ─── Focus Memory ───────────────────────────────────────────────

const zoneFocusMemory = new Map<string, HTMLElement>()

function rememberFocus(zone: string, el: HTMLElement) {
  zoneFocusMemory.set(zone, el)
}

function recallFocus(zone: string): HTMLElement | null {
  const el = zoneFocusMemory.get(zone)
  if (!el) return null
  if (document.contains(el) && el.offsetParent !== null) return el
  zoneFocusMemory.delete(zone)
  return null
}

// ─── Focus Helper ───────────────────────────────────────────────

function focusEl(el: HTMLElement, sound: 'move' | 'action' | 'back' = 'move') {
  playNavSound(sound)
  el.focus({ preventScroll: true })
  el.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
}

// ─── Nav-Ring Modality ──────────────────────────────────────────
// The orange focus ring must only render while the user is actually
// navigating directionally (arrows / gamepad). Ring visibility is gated in
// CSS behind html.controller-nav; directional input turns it on, any
// pointer input (tap, click) turns it off. Without this gate the ring
// painted on plain :focus — every tap on a tabindex card, and every
// programmatic route-change autofocus, lit it up with no controller in
// sight (worst on mobile).
function setNavRing(on: boolean) {
  document.documentElement.classList.toggle('controller-nav', on)
}

function navRingActive(): boolean {
  return document.documentElement.classList.contains('controller-nav')
}

// ─── Main Composable ────────────────────────────────────────────

export function useControllerNav(containerRef?: { value: HTMLElement | null }) {
  const route = useRoute()
  const router = useRouter()
  const store = useControllerStore()
  const isControllerActive = ref(false)
  const gamepadCount = ref(0)

  watch([isControllerActive, gamepadCount], () => {
    store.setActive(isControllerActive.value)
    store.setGamepadCount(gamepadCount.value)
  }, { immediate: true })

  let keyNavTimeout: ReturnType<typeof setTimeout> | null = null
  let pollIntervalId: ReturnType<typeof setInterval> | null = null

  function checkGamepads() {
    const gamepads = navigator.getGamepads?.()
    const count = gamepads ? Array.from(gamepads).filter(g => g?.connected).length : 0
    if (count !== gamepadCount.value) {
      gamepadCount.value = count
      isControllerActive.value = count > 0
    }
  }

  // ─── Keyboard Handler ───────────────────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.target as HTMLElement | null)?.closest?.('[data-archipelago-terminal]')) return
    const navKeys = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Enter', 'Escape']
    if (!navKeys.includes(e.key)) return

    const target = e.target as HTMLElement
    const activeEl = document.activeElement as HTMLElement

    // ── MODAL SCOPE ──────────────────────────────────────────
    // An open dialog owns navigation: focus is pulled inside, arrows move
    // spatially between its controls, Enter activates. Escape stays with the
    // modal's own close handling. Standard mapping, no per-modal code.
    const modal = getOpenModal()
    if (modal) {
      if (e.key === 'Escape') return
      const focusables = getFocusableElements(modal)
      if (!focusables.length) return
      if (!activeEl || !modal.contains(activeEl)) {
        e.preventDefault()
        const first = focusables[0]
        if (first) focusEl(first)
        return
      }
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
        // Typing keys stay with the field; Enter keeps the form-submit
        // behavior below; only Up/Down leave the field spatially.
        if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Enter') return
      } else if (e.key === 'Enter') {
        e.preventDefault()
        playNavSound('action')
        activeEl.click()
        return
      }
      e.preventDefault()
      const dir =
        e.key === 'ArrowDown' ? ('down' as const)
        : e.key === 'ArrowUp' ? ('up' as const)
        : e.key === 'ArrowLeft' ? ('left' as const)
        : ('right' as const)
      const nearest = findNearestInDirection(activeEl, focusables.filter(el => el !== activeEl), dir)
      if (nearest) focusEl(nearest)
      return
    }

    // ── TEXT INPUT HANDLING ──────────────────────────────────
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      if (
        e.key === 'Enter' &&
        target.tagName === 'INPUT' &&
        (target as HTMLInputElement).type !== 'submit' &&
        !target.hasAttribute('data-controller-no-submit')
      ) {
        // Enter in input: click next button (submit pattern). Live-filter
        // search boxes opt out via data-controller-no-submit — the "next
        // focusable element" after a search field is often an unrelated
        // button (a clear icon, a sideload/upload action) that just happens
        // to sit next to it in the DOM, not a submit action for the query.
        e.preventDefault()
        const all = getFocusableElements(containerRef?.value ?? document)
        const idx = all.indexOf(target)
        const next = idx >= 0 ? all[idx + 1] : undefined
        if (next && (next.tagName === 'BUTTON' || next.getAttribute('role') === 'button')) {
          next.focus()
          next.click()
        } else if (next) {
          next.focus()
        }
        return
      }
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        // Up/Down: exit field, navigate spatially
        e.preventDefault()
        const dir = e.key === 'ArrowDown' ? 'down' as const : 'up' as const
        const all = getFocusableElements(containerRef?.value ?? document)
        const candidates = all.filter(el => el !== target)
        const nearest = findNearestInDirection(target, candidates, dir)
        if (nearest) {
          focusEl(nearest)
        } else {
          // Spatial nav failed — try containers directly (e.g. search bar → first container)
          const containers = getContainers()
          const containerNearest = containers.length
            ? findNearestInDirection(target, containers, dir)
            : null
          if (containerNearest) {
            focusEl(containerNearest)
          } else {
            // Last fallback: tab order
            const idx = all.indexOf(target)
            const fallback = dir === 'down' ? all[idx + 1] : all[idx - 1]
            if (fallback) focusEl(fallback)
          }
        }
        return
      }
      // Left/Right: cursor movement in field, but exit at edges
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        const input = target as HTMLInputElement
        const atStart = input.selectionStart === 0 && input.selectionEnd === 0
        const atEnd = input.selectionStart === (input.value?.length ?? 0)
        if ((e.key === 'ArrowLeft' && atStart) || (e.key === 'ArrowRight' && atEnd)) {
          e.preventDefault()
          const dir = e.key === 'ArrowLeft' ? 'left' as const : 'right' as const
          const all = getFocusableElements(containerRef?.value ?? document)
          const candidates = all.filter(el => el !== target)
          const nearest = findNearestInDirection(target, candidates, dir)
          if (nearest) focusEl(nearest)
        }
        return
      }
      // Other keys (Escape): handled below.
      if (e.key !== 'Escape') return
    }

    // ── CLOSE OVERLAYS (Escape) ─────────────────────────────
    if (e.key === 'Escape') {
      if (useAppLauncherStore().isOpen) { useAppLauncherStore().close(); e.preventDefault(); return }
      if (useSpotlightStore().isOpen) { useSpotlightStore().close(); e.preventDefault(); return }
      if (useCLIStore().isOpen) { useCLIStore().close(); e.preventDefault(); return }

      // Inside container inner controls → exit to container
      if (isInsideContainer(activeEl)) {
        const container = activeEl.closest('[data-controller-container]') as HTMLElement | null
        if (container && container.tabIndex >= 0) {
          focusEl(container, 'back')
          e.preventDefault()
          return
        }
      }

      // On a container or anywhere in main → go to sidebar
      if (isInZone(activeEl, 'main')) {
        const sidebar = getSidebarElements()
        const sidebarZone = document.querySelector('[data-controller-zone="sidebar"]')
        const activeTab = sidebarZone?.querySelector<HTMLElement>('.nav-tab-active')
        const target = activeTab ?? sidebar[0]
        if (target) {
          rememberFocus('main', activeEl)
          focusEl(target, 'back')
          e.preventDefault()
        }
        return
      }

      // Detail pages: go back
      if (/\/apps\/[^/]+$|\/marketplace\/[^/]+$|\/cloud\/[^/]+$/.test(route.path)) {
        playNavSound('back')
        window.history.back()
        e.preventDefault()
      }
      return
    }

    // ── ENTER ───────────────────────────────────────────────
    if (e.key === 'Enter') {
      e.preventDefault()

      if (isContainer(activeEl)) {
        // Container declares its own click as THE action (e.g. a media file
        // card whose click plays it) — without this the a[href] fallback
        // below hits the card's download link and gamepad-select downloads
        // a song instead of playing it.
        if (activeEl.hasAttribute('data-controller-primary')) {
          playNavSound('action')
          activeEl.click()
          return
        }
        // Prioritised action: install button
        if (activeEl.hasAttribute('data-controller-install')) {
          const btn = activeEl.querySelector<HTMLButtonElement>('[data-controller-install-btn]:not([disabled])')
          if (btn) { playNavSound('action'); btn.click(); return }
        }
        // Prioritised action: launch button
        if (activeEl.hasAttribute('data-controller-launch')) {
          const btn = activeEl.querySelector<HTMLButtonElement>('[data-controller-launch-btn]:not([disabled])')
          if (btn) { playNavSound('action'); btn.click(); return }
        }
        // Primary link (e.g. dashboard cards with a[href])
        const primaryLink = activeEl.querySelector<HTMLElement>('a[href]')
        if (primaryLink) {
          playNavSound('action')
          primaryLink.click()
          return
        }
        // Fallback: first non-disabled action button (skip uninstall/delete buttons)
        const inner = getInnerFocusables(activeEl)
        const actionBtn = inner.find(el =>
          (el.tagName === 'BUTTON' || el.getAttribute('role') === 'button') &&
          !el.getAttribute('aria-label')?.toLowerCase().includes('uninstall') &&
          !el.closest('[class*="absolute top"]')
        ) ?? inner[0]
        if (actionBtn) {
          focusEl(actionBtn, 'action')
          return
        }
        // Last resort: click the container itself (triggers goToApp on AppCard)
        playNavSound('action')
        activeEl.click()
        return
      }

      // Regular element: click it
      if (activeEl) {
        playNavSound('action')
        activeEl.click()
      }
      return
    }

    // ── ARROW KEYS ──────────────────────────────────────────
    if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) return
    e.preventDefault()

    // Mark controller as active
    isControllerActive.value = true
    setNavRing(true)
    if (keyNavTimeout) clearTimeout(keyNavTimeout)
    keyNavTimeout = setTimeout(() => { isControllerActive.value = gamepadCount.value > 0 }, 3000)

    // Nothing focused yet (fresh page, or focus was lost) — enter nav mode
    // on the first arrow press by focusing the first container, instead of
    // spatially navigating from <body>.
    if (!activeEl || activeEl === document.body || activeEl === document.documentElement) {
      autoFocusMain()
      return
    }

    const dir = e.key === 'ArrowLeft' ? 'left' as const
      : e.key === 'ArrowRight' ? 'right' as const
      : e.key === 'ArrowUp' ? 'up' as const
      : 'down' as const

    // ── SIDEBAR ─────────────────────────────────────────────
    if (isInZone(activeEl, 'sidebar')) {
      const items = getSidebarElements()
      const idx = items.indexOf(activeEl)

      if (dir === 'up' || dir === 'down') {
        // Linear wrap
        if (idx < 0) return
        const nextIdx = dir === 'down'
          ? (idx >= items.length - 1 ? 0 : idx + 1)
          : (idx <= 0 ? items.length - 1 : idx - 1)
        const next = items[nextIdx]
        if (next && next !== activeEl) {
          focusEl(next)
          // Auto-navigate sidebar links (not buttons — Logout etc. require Enter)
          if (next.tagName === 'A') {
            const href = (next as HTMLAnchorElement).getAttribute('href')
            if (href?.startsWith('/')) router.push(href).catch(() => {})
          }
        }
        return
      }

      if (dir === 'right') {
        // Jump to first container in main
        rememberFocus('sidebar', activeEl)
        const remembered = recallFocus('main')
        // Only use remembered if it's a container (not a nav bar button)
        const target = (remembered && isContainer(remembered)) ? remembered : null
        const containers = getContainers()
        const dest = target ?? containers[0]
        if (dest) {
          focusEl(dest)
        } else {
          // Check if this is a container-free page (e.g. Settings) — focus first button immediately
          const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
          const hasAnyContainers = zone?.querySelector('[data-controller-container]')
          if (!hasAnyContainers && zone) {
            const focusable = getFocusableElements(zone)
            if (focusable[0]) { focusEl(focusable[0]); return }
          }
          // Containers not rendered yet (route transition / animation in progress)
          // Poll until they appear, up to 1s
          let attempts = 0
          const poll = setInterval(() => {
            attempts++
            const retryContainers = getContainers()
            if (retryContainers[0]) {
              clearInterval(poll)
              focusEl(retryContainers[0])
            } else if (attempts >= 10) {
              clearInterval(poll)
              // Last resort: focus first focusable element
              if (zone) { const f = getFocusableElements(zone); if (f[0]) focusEl(f[0]) }
            }
          }, 100)
        }
        return
      }

      // Left from sidebar: does nothing
      return
    }

    // ── INSIDE CONTAINER (inner controls) ───────────────────
    if (isInsideContainer(activeEl)) {
      const container = activeEl.closest('[data-controller-container]') as HTMLElement
      const inner = getInnerFocusables(container)
      const next = findNearestInDirection(activeEl, inner, dir)
      if (next) focusEl(next)
      // Can't leave container via arrows — must use Escape
      return
    }

    // ── NAV BAR [N] — secondary controls above the grid ────
    if (isNavBarItem(activeEl)) {
      const navItems = getNavBarItems()

      if (dir === 'left' || dir === 'right') {
        // Spatial nav between nav bar items
        const next = findNearestInDirection(activeEl, navItems, dir)
        if (next) { focusEl(next); return }
        // Left from leftmost nav item → sidebar
        if (dir === 'left') {
          const sidebarZone = document.querySelector('[data-controller-zone="sidebar"]')
          const activeTab = sidebarZone?.querySelector<HTMLElement>('.nav-tab-active')
          const target = activeTab ?? getSidebarElements()[0]
          if (target) focusEl(target)
        }
        return
      }

      if (dir === 'down' || dir === 'up') {
        // Up/Down from standalone element → find nearest focusable (container or button) in direction.
        // Searches containers + standalone elements together so mixed pages (Settings) don't jump.
        if (dir === 'down') rememberFocus('navBar', activeEl)
        const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
        if (zone) {
          const allFocusable = getFocusableElements(zone).filter(el =>
            el.hasAttribute('data-controller-container') ||
            !el.closest('[data-controller-container]')
          )
          const target = findNearestInDirection(activeEl, allFocusable, dir)
          if (target) { focusEl(target); return }
        }
      }
      return
    }

    // ── MAIN ZONE: CONTAINER TILE GRID [C] ──────────────────
    if (isInZone(activeEl, 'main')) {
      const containers = getContainers()

      // Try spatial nav to containers + standalone focusables (not inner buttons).
      // This handles mixed pages (e.g. Settings) where containers and buttons coexist.
      const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
      const navTargets = zone ? getFocusableElements(zone).filter(el =>
        el.hasAttribute('data-controller-container') ||
        !el.closest('[data-controller-container]')
      ) : containers
      const next = findNearestInDirection(activeEl, navTargets, dir)
      if (next) {
        rememberFocus('main', next)
        focusEl(next)
        return
      }

      // Up from top-row container → nav bar, or previous focusable (linear pages like Settings)
      if (dir === 'up') {
        const remembered = recallFocus('navBar')
        if (remembered) { focusEl(remembered); return }
        const navItems = getNavBarItems()
        if (navItems.length) {
          const nearest = findNearestInDirection(activeEl, navItems, 'up')
          if (nearest) { focusEl(nearest); return }
          const first = navItems[0]
          if (first) { focusEl(first); return }
        }
        // No nav bar items — try any focusable element above (linear page nav)
        const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
        if (zone) {
          const allFocusable = getFocusableElements(zone).filter(el =>
            el.hasAttribute('data-controller-container') ||
            !el.closest('[data-controller-container]')
          )
          const above = findNearestInDirection(activeEl, allFocusable, 'up')
          if (above) { rememberFocus('main', above); focusEl(above) }
        }
        return
      }

      // Left from leftmost container → sidebar
      if (dir === 'left') {
        rememberFocus('main', activeEl)
        const remembered = recallFocus('sidebar')
        const sidebarZone = document.querySelector('[data-controller-zone="sidebar"]')
        const activeTab = sidebarZone?.querySelector<HTMLElement>('.nav-tab-active')
        const target = remembered ?? activeTab ?? getSidebarElements()[0]
        if (target) focusEl(target)
        return
      }

      // At grid edges: try containers + nav bar items as fallback
      // (prevents dead ends, but never jumps into container inner controls)
      if (dir === 'down' || dir === 'right') {
        const zone = document.querySelector('[data-controller-zone="main"]') as HTMLElement | null
        if (zone) {
          const allFocusable = getFocusableElements(zone).filter(el =>
            el.hasAttribute('data-controller-container') ||
            !el.closest('[data-controller-container]')
          )
          const fallback = findNearestInDirection(activeEl, allFocusable, dir)
          if (fallback) {
            rememberFocus('main', fallback)
            focusEl(fallback)
          }
        }
      }
      return
    }

    // ── FALLBACK: unhandled focusable element ───────────────
    // Covers standalone buttons/links in empty/error states, modals, etc.
    // that aren't inside a recognized zone or container.
    if (dir === 'left') {
      const sidebar = getSidebarElements()
      const sidebarZone = document.querySelector('[data-controller-zone="sidebar"]')
      const activeTab = sidebarZone?.querySelector<HTMLElement>('.nav-tab-active')
      const target = activeTab ?? sidebar[0]
      if (target) { rememberFocus('main', activeEl); focusEl(target) }
    } else {
      // Exclude container inner buttons to prevent focus getting lost
      const all = getFocusableElements().filter(el =>
        el.hasAttribute('data-controller-container') ||
        !el.closest('[data-controller-container]')
      )
      const next = findNearestInDirection(activeEl, all, dir)
      if (next) focusEl(next)
    }
  }

  // ─── Gamepad Detection ──────────────────────────────────────

  function handleGamepadConnected() {
    const gamepads = navigator.getGamepads?.()
    gamepadCount.value = gamepads ? Array.from(gamepads).filter(g => g?.connected).length : 1
    isControllerActive.value = true
    setNavRing(true)
  }

  function handleGamepadDisconnected() {
    const gamepads = navigator.getGamepads?.()
    gamepadCount.value = gamepads ? Array.from(gamepads).filter(g => g?.connected).length : 0
    isControllerActive.value = gamepadCount.value > 0
  }

  // ─── Scroll Support ────────────────────────────────────────

  function handleWheel(e: WheelEvent) {
    // Scroll the container UNDER THE POINTER, not the focused element. Real
    // wheel events always target the element beneath the cursor, so walking up
    // from e.target matches native behaviour. Using document.activeElement here
    // caused the wheel to scroll a previously-clicked container (e.g. the mesh
    // peer list, still focused after a click) instead of the panel actually
    // being hovered — producing a double-scroll where both moved at once.
    const start = (e.target as HTMLElement | null) ?? (document.activeElement as HTMLElement | null)
    if (!start) return
    let p: HTMLElement | null = start
    while (p) {
      const style = getComputedStyle(p)
      if ((style.overflowY === 'auto' || style.overflowY === 'scroll') && p.scrollHeight > p.clientHeight) {
        if (e.deltaY !== 0) { p.scrollTop += e.deltaY; e.preventDefault() }
        return
      }
      p = p.parentElement
    }
  }

  // ─── Auto-Focus on Route Change ────────────────────────────

  function autoFocusMain() {
    // Only when the user is actually in directional-nav mode. For pointer
    // and touch users this programmatic focus painted the first card's
    // focus ring on every route change with zero controller input.
    if (!navRingActive()) return
    const active = document.activeElement as HTMLElement | null
    // Don't steal focus from inputs, modals, or sidebar
    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) return
    if (document.querySelector('[role="dialog"]')) return
    if (isInZone(active, 'sidebar')) return

    requestAnimationFrame(() => {
      // Re-check sidebar after RAF — user may still be navigating
      if (isInZone(document.activeElement as HTMLElement, 'sidebar')) return
      const remembered = recallFocus('main')
      if (remembered) { remembered.focus({ preventScroll: true }); return }
      const containers = getContainers()
      if (containers[0]) containers[0].focus({ preventScroll: true })
    })
  }

  watch(() => route.path, () => {
    zoneFocusMemory.delete('main')
    zoneFocusMemory.delete('navBar')
    setTimeout(autoFocusMain, 150)
  })

  // ─── Lifecycle ─────────────────────────────────────────────

  // Any pointer interaction ends directional-nav mode — the ring should
  // never linger once the user reaches for mouse or touch.
  function handlePointerDown() {
    setNavRing(false)
  }

  onMounted(() => {
    checkGamepads()
    window.addEventListener('keydown', handleKeyDown, true)
    window.addEventListener('pointerdown', handlePointerDown, { capture: true, passive: true })
    window.addEventListener('wheel', handleWheel, { passive: false })
    window.addEventListener('gamepadconnected', handleGamepadConnected)
    window.addEventListener('gamepaddisconnected', handleGamepadDisconnected)
    pollIntervalId = setInterval(() => checkGamepads(), 500)
    setTimeout(autoFocusMain, 300)
  })

  onBeforeUnmount(() => {
    window.removeEventListener('keydown', handleKeyDown, true)
    window.removeEventListener('pointerdown', handlePointerDown, { capture: true } as EventListenerOptions)
    window.removeEventListener('wheel', handleWheel)
    window.removeEventListener('gamepadconnected', handleGamepadConnected)
    window.removeEventListener('gamepaddisconnected', handleGamepadDisconnected)
    if (pollIntervalId) clearInterval(pollIntervalId)
    if (keyNavTimeout) clearTimeout(keyNavTimeout)
    setNavRing(false)
  })

  return { isControllerActive, gamepadCount }
}
