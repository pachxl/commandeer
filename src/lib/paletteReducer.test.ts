import { describe, expect, it } from 'vitest'
import type { AppConfig } from '../types'
import { clampSelectionIndex, initialState, reducer } from './paletteReducer'

const config: AppConfig = { scripts_dir: '' }

describe('palette selection', () => {
  it('sets pointer and keyboard selection absolutely', () => {
    const state = { ...initialState(config), selectedIndex: 12 }
    const next = reducer(state, { type: 'SET_SELECTION', index: 2 })
    expect(next.selectedIndex).toBe(2)
  })

  it('clamps stale selection after a list shrinks', () => {
    expect(clampSelectionIndex(12, 3)).toBe(2)
    expect(clampSelectionIndex(12, 0)).toBe(0)
    expect(clampSelectionIndex(-1, 3)).toBe(0)
  })
})
