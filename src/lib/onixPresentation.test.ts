import { describe, expect, it } from 'vitest'
import {
  initialOnixSessionState,
  onixSessionReducer,
  resolveOnixPresentation,
  type OnixPresentationContext,
} from './onixPresentation'

const compactContext: OnixPresentationContext = {
  isOnix: true,
  sessionExpanded: false,
  currentStep: null,
  query: '',
  error: null,
  confirmOpen: false,
  actionPanelOpen: false,
}

describe('Onix presentation state', () => {
  it('uses the compact lens only for an untouched Onix root session', () => {
    expect(resolveOnixPresentation(compactContext)).toBe('compact')
    expect(resolveOnixPresentation({ ...compactContext, isOnix: false })).toBe('default')
  })

  it.each([
    { query: 'files' },
    { query: ' ' },
    { currentStep: { id: 'settings' } },
    { error: 'failed' },
    { confirmOpen: true },
    { actionPanelOpen: true },
  ])('expands for a surface that cannot fit in the search lens: %o', patch => {
    expect(resolveOnixPresentation({ ...compactContext, ...patch })).toBe('expanded')
  })

  it('keeps expansion sticky until the visible session is explicitly reset', () => {
    const expanded = onixSessionReducer(initialOnixSessionState, { type: 'expand' })
    expect(expanded.expanded).toBe(true)

    const cleared = onixSessionReducer(expanded, {
      type: 'sync',
      context: {
        currentStep: null,
        query: '',
        error: null,
        confirmOpen: false,
        actionPanelOpen: false,
      },
    })
    expect(cleared).toBe(expanded)
    expect(onixSessionReducer(cleared, { type: 'reset' })).toEqual(initialOnixSessionState)
  })

  it('makes a forcing surface sticky when synchronized', () => {
    const expanded = onixSessionReducer(initialOnixSessionState, {
      type: 'sync',
      context: {
        currentStep: null,
        query: 'a',
        error: null,
        confirmOpen: false,
        actionPanelOpen: false,
      },
    })
    expect(expanded.expanded).toBe(true)
  })
})
