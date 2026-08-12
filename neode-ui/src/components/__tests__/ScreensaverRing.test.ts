import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import ScreensaverRing from '../ScreensaverRing.vue'

// The ring is shared by the screensaver (default), SystemDangerZone (compact)
// and — as of FED-06 — the payment-success tick (badge). These assertions pin
// the variant mapping so adding a size can never silently re-point an existing
// call site at different geometry.
describe('ScreensaverRing', () => {
  it('maps each size variant to its own ring class', () => {
    expect(mount(ScreensaverRing).classes()).toContain('viz-ring-default')
    expect(mount(ScreensaverRing, { props: { size: 'compact' } }).classes()).toContain(
      'viz-ring-compact',
    )
    expect(mount(ScreensaverRing, { props: { size: 'badge' } }).classes()).toContain(
      'viz-ring-badge',
    )
  })

  it('keeps the existing variants off the badge class', () => {
    expect(mount(ScreensaverRing).classes()).not.toContain('viz-ring-badge')
    expect(mount(ScreensaverRing, { props: { size: 'compact' } }).classes()).not.toContain(
      'viz-ring-badge',
    )
  })

  it('renders one segment per segmentCount, defaulting to 48', () => {
    expect(mount(ScreensaverRing).findAll('.viz-segment')).toHaveLength(48)
    expect(
      mount(ScreensaverRing, { props: { segmentCount: 12 } }).findAll('.viz-segment'),
    ).toHaveLength(12)
    expect(
      mount(ScreensaverRing, { props: { size: 'badge', segmentCount: 24 } }).findAll('.viz-segment'),
    ).toHaveLength(24)
  })
})
