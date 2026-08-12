import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    isOnboardingComplete: vi.fn(),
    completeOnboarding: vi.fn(),
  },
}))

import { isOnboardingComplete, completeOnboarding } from '../useOnboarding'
import { rpcClient } from '@/api/rpc-client'

const mockedRpc = vi.mocked(rpcClient)

describe('useOnboarding', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  describe('isOnboardingComplete', () => {
    it('returns true when RPC says complete', async () => {
      mockedRpc.isOnboardingComplete.mockResolvedValue(true)
      const result = await isOnboardingComplete()
      expect(result).toBe(true)
    })

    it('returns false when RPC says not complete', async () => {
      mockedRpc.isOnboardingComplete.mockResolvedValue(false)
      const result = await isOnboardingComplete()
      expect(result).toBe(false)
    })

    it('falls back to localStorage when RPC fails with non-retryable error', async () => {
      mockedRpc.isOnboardingComplete.mockRejectedValue(new Error('Unknown error'))
      localStorage.setItem('neode_onboarding_complete', '1')
      const result = await isOnboardingComplete()
      expect(result).toBe(true)
    })

    it('returns false from localStorage fallback when not set', async () => {
      mockedRpc.isOnboardingComplete.mockRejectedValue(new Error('Unknown error'))
      const result = await isOnboardingComplete()
      expect(result).toBe(false)
    })

    it('retries on 502 errors before falling back', async () => {
      mockedRpc.isOnboardingComplete
        .mockRejectedValueOnce(new Error('502 Bad Gateway'))
        .mockResolvedValueOnce(true)

      const promise = isOnboardingComplete()
      await vi.advanceTimersByTimeAsync(900)
      const result = await promise
      expect(result).toBe(true)
      expect(mockedRpc.isOnboardingComplete).toHaveBeenCalledTimes(2)
    })

    it('retries on 503 errors', async () => {
      mockedRpc.isOnboardingComplete
        .mockRejectedValueOnce(new Error('503 Service Unavailable'))
        .mockResolvedValueOnce(false)

      const promise = isOnboardingComplete()
      await vi.advanceTimersByTimeAsync(900)
      const result = await promise
      expect(result).toBe(false)
    })

    it('falls back to localStorage after exhausting retries', async () => {
      mockedRpc.isOnboardingComplete.mockRejectedValue(new Error('502 Bad Gateway'))
      localStorage.setItem('neode_onboarding_complete', '1')

      const promise = isOnboardingComplete()
      await vi.advanceTimersByTimeAsync(8000)
      const result = await promise
      expect(result).toBe(true)
    }, 10000)
  })

  describe('completeOnboarding', () => {
    it('calls RPC and sets localStorage', async () => {
      mockedRpc.completeOnboarding.mockResolvedValue(true)
      await completeOnboarding()
      expect(mockedRpc.completeOnboarding).toHaveBeenCalled()
      expect(localStorage.getItem('neode_onboarding_complete')).toBe('1')
    })

    it('sets localStorage even when RPC fails', async () => {
      mockedRpc.completeOnboarding.mockRejectedValue(new Error('Network error'))
      const promise = completeOnboarding()
      await vi.advanceTimersByTimeAsync(10000)
      await promise
      expect(localStorage.getItem('neode_onboarding_complete')).toBe('1')
    })
  })
})
