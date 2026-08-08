import { describe, expect, it } from 'vitest'
import { getOpticalRenderMetrics, ONIX_FRAGMENT_SHADER } from './useOnixOptics'

describe('Onix optical render contract', () => {
  it('supersamples a 1x desktop while retaining logical shader coordinates', () => {
    expect(getOpticalRenderMetrics(770, 66, 1)).toEqual({
      logicalWidth: 770,
      logicalHeight: 66,
      renderScale: 2,
      pixelWidth: 1540,
      pixelHeight: 132,
    })
  })

  it('keeps Retina at 2x and caps unexpectedly larger backing scales', () => {
    expect(getOpticalRenderMetrics(770, 340, 2).renderScale).toBe(2)
    expect(getOpticalRenderMetrics(770, 340, 3).renderScale).toBe(2)
    expect(getOpticalRenderMetrics(770, 340, Number.NaN).renderScale).toBe(2)
  })

  it('casts the pointer ray outward and keeps the response local to the nearest rim', () => {
    expect(ONIX_FRAGMENT_SHADER).toContain('vec2 from_pointer = surface_point - pointer_position;')
    expect(ONIX_FRAGMENT_SHADER).not.toContain('pointer_position - surface_point')
    expect(ONIX_FRAGMENT_SHADER).toContain('float excess_gap = max(normal_gap - pointer_depth, 0.0);')
    expect(ONIX_FRAGMENT_SHADER).toContain('float pointer_focus =')
  })
})
