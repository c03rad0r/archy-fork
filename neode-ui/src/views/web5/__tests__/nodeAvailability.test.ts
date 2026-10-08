import { describe, expect, it } from 'vitest'
import { nodeAvailability, lastContactLabel, validContact, REACHABILITY_MAX_AGE, availabilityRank } from '../nodeAvailability'

describe('node availability observations', () => {
  const now = Date.parse('2026-10-06T00:00:00Z')
  it('ages cached observations and rejects malformed or future samples', () => {
    expect(nodeAvailability({ reachable: true, checkedAt: now }, now)).toBe('online')
    expect(nodeAvailability({ reachable: false, checkedAt: now }, now)).toBe('unreachable')
    expect(nodeAvailability({ reachable: null, checkedAt: now }, now)).toBe('unknown')
    for (const checkedAt of [NaN, now + 120000, now - REACHABILITY_MAX_AGE - 1]) {
      expect(nodeAvailability({ reachable: true, checkedAt }, now)).toBe('unknown')
    }
    expect(nodeAvailability(undefined, now)).toBe('unknown')
  })
  it('reports last contact without inventing continuous offline duration', () => {
    expect(lastContactLabel(now - 2 * 86400000, now)).toBe('Last seen 2d ago')
    for (const value of [null, undefined, '', 'broken', 0, now + 120000]) {
      expect(validContact(value, now)).toBeUndefined()
      expect(lastContactLabel(value, now)).toBe('No confirmed contact')
    }
    expect(lastContactLabel(now + 30000, now)).toBe('Last seen just now')
    expect(['unreachable', 'online', 'unknown'].sort((a, b) => availabilityRank(a as any) - availabilityRank(b as any))).toEqual(['online', 'unknown', 'unreachable'])
  })
})
