// State reducer for the command palette's navigation stack.
//
// Pure, JSX-free logic extracted from Palette.tsx: the step stack, query, item
// cache, and selection state that drive palette navigation.

import type { AppConfig, PaletteAction, PaletteState } from '../types'

export function clampSelectionIndex(index: number, itemCount: number): number {
  return Math.min(Math.max(0, index), Math.max(0, itemCount - 1))
}

export function initialState(_config: AppConfig): PaletteState {
  return {
    query: '',
    stepStack: [],
    selectionStack: [],
    itemCache: { '__root__': [] },
    selectedIndex: 0,
    loading: false,
    error: null,
  }
}

export function reducer(state: PaletteState, action: PaletteAction): PaletteState {
  switch (action.type) {
    case 'SET_QUERY':
      return { ...state, query: action.query, selectedIndex: 0, error: null }

    case 'SET_ITEMS':
      return {
        ...state,
        itemCache: { ...state.itemCache, [action.stepId]: action.items },
        loading: false,
        selectedIndex: action.preserveSelection ? state.selectedIndex : 0,
      }

    case 'PUSH_STEP':
      return {
        ...state,
        stepStack: [...state.stepStack, action.step],
        // Remember where we were so popping back restores this view
        selectionStack: [
          ...state.selectionStack,
          { query: state.query, selectedIndex: state.selectedIndex },
        ],
        query: '',
        selectedIndex: 0,
        loading: false,
        error: null,
      }

    case 'POP_STEP': {
      const restored = state.selectionStack[state.selectionStack.length - 1]
      return {
        ...state,
        stepStack: state.stepStack.slice(0, -1),
        selectionStack: state.selectionStack.slice(0, -1),
        query: restored?.query ?? '',
        selectedIndex: restored?.selectedIndex ?? 0,
        loading: false,
        error: null,
      }
    }

    case 'REPLACE_STEP':
      // preserveSelection: same-id replaces (toggles, theme apply) keep the
      // query and highlighted row instead of jumping back to the top
      return {
        ...state,
        stepStack: [...state.stepStack.slice(0, -1), action.step],
        query: action.preserveSelection ? state.query : '',
        selectedIndex: action.preserveSelection ? state.selectedIndex : 0,
        loading: false,
        error: null,
      }

    case 'SET_SELECTION':
      return {
        ...state,
        selectedIndex: Math.max(0, action.index),
      }

    case 'SET_LOADING':
      return { ...state, loading: action.loading, error: null }

    case 'SET_ERROR':
      return { ...state, error: action.error, loading: false }

    case 'RESET':
      return {
        ...state,
        query: '',
        stepStack: [],
        selectionStack: [],
        selectedIndex: 0,
        loading: false,
        error: null,
      }

    default:
      return state
  }
}
