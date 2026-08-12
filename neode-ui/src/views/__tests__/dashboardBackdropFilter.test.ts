import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

/**
 * Chromium/Brave mis-rasterise `backdrop-filter` inside the dashboard's
 * animated perspective/scroll containers. style.css already neutralises it
 * for the shared glass classes, but that list is hand-maintained: a component
 * that declares its own `backdrop-filter` in a local <style> block is simply
 * not covered, and nothing fails.
 *
 * That is exactly how the 2026-08-03 seam shipped. `.home-card-shell` carried
 * `backdrop-filter: blur(18px)` in Home.vue and was missing from the list, so
 * a hover repaint left a vertical line where the refreshed backdrop met the
 * stale one — visible in both dashboard cards at the same screen x, and
 * absent in the gap between them.
 *
 * This test makes the omission fail loudly instead of shipping as a glitch
 * nobody can reproduce on demand.
 */

const root = resolve(__dirname, '../../..')
const styleCss = readFileSync(resolve(root, 'src/style.css'), 'utf8')

/** The selector list that disables backdrop-filter on the dashboard. */
function dashboardMitigationBlock(): string {
  const start = styleCss.indexOf('body.dashboard-active .dashboard-scroll-panel .glass-card')
  expect(start, 'dashboard backdrop-filter mitigation block not found').toBeGreaterThan(-1)
  const end = styleCss.indexOf('}', start)
  return styleCss.slice(start, end)
}

/** Class selectors that declare a non-none backdrop-filter in a .vue file. */
function blurredClassesIn(relPath: string): string[] {
  const src = readFileSync(resolve(root, relPath), 'utf8')
  const found = new Set<string>()
  // Match `.some-class { ... backdrop-filter: <not none> ... }` on one line,
  // which is how these single-line rules are written in this codebase.
  const ruleRe = /(\.[a-zA-Z0-9_-]+)\s*\{([^}]*)\}/g
  let m: RegExpExecArray | null
  while ((m = ruleRe.exec(src)) !== null) {
    const selector = m[1]
    const body = m[2]
    if (!selector || !body) continue
    const decl = /(?:^|[;{\s])backdrop-filter\s*:\s*([^;]+)/.exec(body)
    if (decl?.[1] && decl[1].trim() !== 'none') found.add(selector)
  }
  return [...found]
}

describe('dashboard backdrop-filter mitigation', () => {
  it('covers every backdrop-filter surface Home.vue defines itself', () => {
    const block = dashboardMitigationBlock()
    const uncovered = blurredClassesIn('src/views/Home.vue').filter(
      (sel) => !block.includes(`.dashboard-scroll-panel ${sel},`),
    )
    expect(
      uncovered,
      `these Home.vue classes declare backdrop-filter but are not in the ` +
        `body.dashboard-active .dashboard-scroll-panel mitigation list in style.css, ` +
        `so Chromium will leave repaint seams across the dashboard cards`,
    ).toEqual([])
  })

  it('still lists the shared glass classes', () => {
    // Guards against someone "cleaning up" the list and silently reopening
    // the original black-rectangle corruption this block was written for.
    const block = dashboardMitigationBlock()
    for (const sel of ['.glass-card', '.glass-button', '.home-card-shell']) {
      expect(block).toContain(`.dashboard-scroll-panel ${sel},`)
    }
  })

  it('the mitigation actually disables the filter', () => {
    const start = styleCss.indexOf('body.dashboard-active .dashboard-scroll-panel .glass-card')
    const body = styleCss.slice(styleCss.indexOf('{', start), styleCss.indexOf('}', start))
    expect(body).toContain('backdrop-filter: none')
    expect(body).toContain('-webkit-backdrop-filter: none')
  })
})
