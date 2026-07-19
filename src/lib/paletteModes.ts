// Root-level "@" modes for the command palette and the derived result list.
//
// Pure, JSX-free helpers extracted from Palette.tsx: parsing an @-prefixed root
// query into a mode, turning the current mode + items into the ranked result
// rows, and computing the calculator/time live preview.

import { fuzzyFilter } from './fuzzy'
import { commandToItem, buildFallbackItems } from './paletteItems'
import { applyOverride, buildQueryResults, type Overrides } from './paletteRanking'
import { evaluateCalcQuery } from '../providers/calculator'
import { tryTimeConversion } from './timezones'
import { IS_LINUX, IS_MAC } from './tauri'
import type { Command, LivePreview, PaletteItem, Step } from '../types'

// Root-level @ prefixes. Typing '@' (or a partial token) lists these as
// suggestions; a completed token followed by a space activates the mode.
//   @find   → global file search (FTS5 index → Everything → walkdir)
//   @search → file search in the focused Explorer/Finder folder
//   @web    → web search in the browser
export const AT_PREFIXES = [
  { token: '@find', icon: 'folder', description: 'Find files across your computer' },
  {
    token: '@search',
    icon: 'folder',
    description: IS_LINUX
      ? 'Search your home folder'
      : IS_MAC
        ? 'Search the focused Finder folder'
        : 'Search the focused Explorer folder',
  },
  { token: '@web', icon: 'search', description: 'Search the web' },
  { token: '@calc', icon: 'calculator', description: 'Calculate an expression (40+2, 100 usd to eur)' },
  { token: '@time', icon: 'clock', description: 'Convert time zones (4pm bst to est)' },
]

export interface AtModeState {
  // The raw @-query (null when not at root or not @-prefixed)
  atRaw: string | null
  // Lower-cased token before the first space ('@find', '@fi', …)
  atToken: string | null
  // Text after the completed token
  atRest: string
  // A recognised token followed by a space — the mode is active
  atComplete: boolean
  // '@' or a partial token — list the available @ commands as suggestions
  atSuggestMode: boolean
  folderMode: boolean
  folderQuery: string
  findMode: boolean
  findQuery: string
  webMode: boolean
  webQuery: string
  calcMode: boolean
  calcQuery: string
  timeMode: boolean
  timeQuery: string
}

// Parse an @-prefixed root query. '@'/'@fi' → suggestion mode; '@find rest'
// (completed token + space) → the corresponding mode with `rest` as query.
// @-modes are root-only, so a pushed step disables them (hasStep).
export function parseAtQuery(query: string, hasStep: boolean): AtModeState {
  const atRaw = !hasStep && query.startsWith('@') ? query : null
  const atSpaceIdx = atRaw?.indexOf(' ') ?? -1
  const atToken = atRaw ? (atSpaceIdx >= 0 ? atRaw.slice(0, atSpaceIdx) : atRaw).toLowerCase() : null
  const atRest = atRaw && atSpaceIdx >= 0 ? atRaw.slice(atSpaceIdx + 1) : ''
  const atComplete = atToken !== null && atSpaceIdx >= 0 && AT_PREFIXES.some(p => p.token === atToken)
  const atSuggestMode = atRaw !== null && !atComplete

  const folderMode = atComplete && atToken === '@search'
  const findMode = atComplete && atToken === '@find'
  const webMode = atComplete && atToken === '@web'
  const calcMode = atComplete && atToken === '@calc'
  const timeMode = atComplete && atToken === '@time'

  return {
    atRaw,
    atToken,
    atRest,
    atComplete,
    atSuggestMode,
    // "@search" → file search in the active Explorer/Finder folder
    folderMode,
    folderQuery: folderMode ? atRest.trimStart() : '',
    // "@find" → global file search
    findMode,
    findQuery: findMode ? atRest.trimStart() : '',
    // "@web" → a single row that opens the browser search
    webMode,
    webQuery: webMode ? atRest.trim() : '',
    // "@calc" / "@time" → evaluate the rest live; Enter copies the result
    calcMode,
    calcQuery: calcMode ? atRest.trim() : '',
    timeMode,
    timeQuery: timeMode ? atRest.trim() : '',
  }
}

export interface MatchedItemsParams {
  at: AtModeState
  currentStep: Step | null
  isInputStep: boolean
  isSliderStep: boolean
  isFormStep: boolean
  // The items backing the current view (step cache, folder/global results, or
  // the flat/hierarchical root lists) — already selected by the caller.
  rawItems: PaletteItem[]
  query: string
  providerCommands: Command[]
  overrides: Overrides
}

// Turn the current mode + backing items into the ranked/filtered rows shown in
// the list. Mirrors the old inline branch order exactly.
export function computeMatchedItems(params: MatchedItemsParams): PaletteItem[] {
  const { at, currentStep, isInputStep, isSliderStep, isFormStep, rawItems, query, providerCommands, overrides } =
    params

  if (isInputStep || isSliderStep || isFormStep || currentStep?.isVolumeMixerStep) {
    return []
  }
  if (at.atSuggestMode) {
    // '@' or a partial token: list the available @ commands; selecting one
    // inserts it into the query instead of executing
    return AT_PREFIXES.filter(p => p.token.startsWith(at.atToken ?? '@')).map(p => ({
      id: `at:${p.token}`,
      label: p.token,
      sublabel: p.description,
      icon: p.icon,
      data: p.token,
      actionLabel: 'Use',
    }))
  }
  if (at.webMode) {
    return at.webQuery
      ? [
          {
            id: `web:${at.webQuery}`,
            label: `Search the web for "${at.webQuery}"`,
            sublabel: 'Opens your browser',
            icon: 'search',
            data: at.webQuery,
            actionLabel: 'Search',
          },
        ]
      : []
  }
  if (at.calcMode || at.timeMode) {
    // Result is shown inline via previewResult; no list row needed
    return []
  }
  if (at.findMode) {
    // Global results are already ranked for this query (fzf + relevance
    // multipliers in globalFileSearch) — re-filtering would fight the ranker
    return rawItems
  }
  if (currentStep || at.folderMode) {
    return fuzzyFilter(
      rawItems,
      at.folderMode ? at.folderQuery : query,
      i => i.searchText ?? i.label + ' ' + (i.sublabel ?? ''),
    )
  }
  if (query) {
    // Root query: scripts and provider search results (which can share ids —
    // keep the first occurrence) ranked together by fuzzy score + frecency
    const merged = [...rawItems, ...providerCommands.map(c => applyOverride(commandToItem(c), overrides[c.id]))]
    const seen = new Set<string>()
    const deduped = merged.filter(i => (seen.has(i.id) ? false : (seen.add(i.id), true)))
    const results = buildQueryResults(deduped, query, overrides)
    // Nothing matched: surface actionable fallback rows so the palette is
    // never a dead end (web / files / GitHub).
    return results.length === 0 ? buildFallbackItems(query) : results
  }
  // Root browse: folders first, then scripts with last-used floating up —
  // exactly as assembled in the __root__ cache
  return rawItems
}

// Live preview shown on the right side of the search input for calculator /
// time-zone modes (root prefixes and Tools input steps).
export function computePreviewResult(params: {
  currentStep: Step | null
  query: string
  calcMode: boolean
  calcQuery: string
  timeMode: boolean
  timeQuery: string
}): LivePreview | null {
  const { currentStep, query, calcMode, calcQuery, timeMode, timeQuery } = params
  if (currentStep?.livePreview) return currentStep.livePreview(query)
  if (calcMode && calcQuery) {
    const r = evaluateCalcQuery(calcQuery)
    return r ? { label: r.display, sublabel: r.sublabel, copy: r.copy } : null
  }
  if (timeMode && timeQuery) {
    const r = tryTimeConversion(timeQuery)
    return r ? { label: r.label, sublabel: r.sublabel, copy: r.copy } : null
  }
  return null
}
