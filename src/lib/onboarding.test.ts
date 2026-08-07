import { describe, expect, it } from 'vitest'
import {
  CURRENT_ONBOARDING_VERSION,
  ONBOARDING_VERSION_KEY,
  markOnboardingSeen,
  shouldShowOnboarding,
} from './onboarding'

describe('onboarding eligibility', () => {
  it('welcomes a genuinely new installation', () => {
    expect(shouldShowOnboarding(() => null)).toBe(true)
  })

  it('does not interrupt an established installation', () => {
    expect(shouldShowOnboarding(key => (key === 'commandeer:scripts' ? '[]' : null))).toBe(false)
  })

  it('records the current onboarding version', () => {
    const values = new Map<string, string>()
    markOnboardingSeen((key, value) => values.set(key, value))
    expect(values.get(ONBOARDING_VERSION_KEY)).toBe(CURRENT_ONBOARDING_VERSION)
  })
})
