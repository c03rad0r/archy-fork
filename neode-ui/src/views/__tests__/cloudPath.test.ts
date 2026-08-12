import { describe, expect, it } from 'vitest'
import { normalizeCloudPath, parentCloudPath } from '../cloudPath'

describe('cloudPath helpers', () => {
  it('normalizes query paths', () => {
    expect(normalizeCloudPath('Photos/Trips')).toBe('/Photos/Trips')
    expect(normalizeCloudPath('/Photos//Trips')).toBe('/Photos/Trips')
    expect(normalizeCloudPath('', '/Photos')).toBe('/Photos')
  })

  it('walks to the parent folder without leaving root', () => {
    expect(parentCloudPath('/Photos/Trips/Day 1')).toBe('/Photos/Trips')
    expect(parentCloudPath('/Photos')).toBe('/')
    expect(parentCloudPath('/')).toBe('/')
  })
})
