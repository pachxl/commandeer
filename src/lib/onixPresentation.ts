export interface OnixPresentationContext {
  isOnix: boolean
  sessionExpanded: boolean
  currentStep?: unknown | null
  query: string
  error?: unknown | null
  confirmOpen: boolean
  actionPanelOpen: boolean
}

export type OnixPresentationState = 'default' | 'compact' | 'expanded'

export interface OnixSessionState {
  expanded: boolean
}

export type OnixSessionEvent =
  | { type: 'expand' }
  | { type: 'sync'; context: Omit<OnixPresentationContext, 'isOnix' | 'sessionExpanded'> }
  | { type: 'reset' }

export const initialOnixSessionState: OnixSessionState = { expanded: false }

/**
 * Surfaces that cannot fit in the compact search lens force an expanded shell.
 * Query length is intentional rather than trim(): entering a space is still an
 * interaction and must not make mounted results flicker back to compact.
 */
export function contextRequiresExpandedOnix(
  context: Omit<OnixPresentationContext, 'isOnix' | 'sessionExpanded'>,
): boolean {
  return (
    context.query.length > 0 ||
    context.currentStep != null ||
    context.error != null ||
    context.confirmOpen ||
    context.actionPanelOpen
  )
}

/**
 * Expansion is sticky for one visible palette session. Clearing a query or
 * popping back to root therefore leaves the useful result surface open. Only
 * the existing whole-session dismissal/reset path should dispatch `reset`.
 */
export function onixSessionReducer(state: OnixSessionState, event: OnixSessionEvent): OnixSessionState {
  if (event.type === 'reset') return initialOnixSessionState
  if (state.expanded || event.type === 'expand' || contextRequiresExpandedOnix(event.context)) {
    return state.expanded ? state : { expanded: true }
  }
  return state
}

export function resolveOnixPresentation(context: OnixPresentationContext): OnixPresentationState {
  if (!context.isOnix) return 'default'
  if (
    context.sessionExpanded ||
    contextRequiresExpandedOnix({
      currentStep: context.currentStep,
      query: context.query,
      error: context.error,
      confirmOpen: context.confirmOpen,
      actionPanelOpen: context.actionPanelOpen,
    })
  ) {
    return 'expanded'
  }
  return 'compact'
}
