import { describe, expect, it } from 'vitest'
import { clampMixerVolume, nextMixerIndex } from './VolumeMixer'

describe('volume mixer keyboard helpers', () => {
  it('clamps session volume to the Core Audio scalar range', () => {
    expect(clampMixerVolume(-0.2)).toBe(0)
    expect(clampMixerVolume(0.42)).toBe(0.42)
    expect(clampMixerVolume(1.2)).toBe(1)
  })

  it('moves selection without escaping the visible session list', () => {
    expect(nextMixerIndex(1, 3, 1)).toBe(2)
    expect(nextMixerIndex(2, 3, 1)).toBe(2)
    expect(nextMixerIndex(0, 3, -1)).toBe(0)
    expect(nextMixerIndex(1, 3, -1)).toBe(0)
    expect(nextMixerIndex(0, 0, 1)).toBe(0)
  })
})
