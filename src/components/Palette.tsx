import { useReducer, useEffect, useRef, useState, useCallback, MutableRefObject } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { fuzzyFilter, fuzzyScoreFieldsBatch } from '../lib/fuzzy'
import { frecencyBonus, recordUse } from '../lib/frecency'
import { appEvents } from '../lib/appEvents'
import { getOverrides, invalidateOverridesCache, setOverride } from '../lib/overrides'
import { SETTINGS_COMMAND_ID } from '../commands/settings'
import { loadActiveFolderItems, openFileItem } from '../commands/fileSearch'
import { loadGlobalFileResults } from '../commands/globalFileSearch'
import { searchAllProviders } from '../providers'
import { evaluateCalcQuery } from '../providers/calculator'
import { tryTimeConversion } from '../lib/timezones'
import { IS_LINUX, IS_MAC, envInfo, openPath, openUrl, pasteToPrevious, readQuicklinks, readNotes, revealPath, runScriptCapture, setCommandHotkey, writeClipboardText, writeQuicklinks, writeNotes, type Bookmark, type ClipboardItem, type CommandOverride, type Note, type Quicklink } from '../lib/tauri'
import type { ActionItem, AppConfig, Command, PaletteAction, PaletteItem, PaletteState } from '../types'
import SearchInput, { SliderInput } from './SearchInput'
import ResultsList from './ResultsList'
import ResultsGrid from './ResultsGrid'
import FormView from './FormView'
import ActionPanel from './ActionPanel'
import DetailPane from './DetailPane'
import { ToastContainer, type ToastKind, type ToastMessage } from './Toast'
import ClaudeUsage from './ClaudeUsage'
import SystemStatsPanel from './SystemStats'
import Footer from './Footer'
import StepBreadcrumb from './StepBreadcrumb'
// ── Root items (the command list) ────────────────────────────────────────────

// Extra search terms (folder name, keywords, aliases) folded into the
// fuzzy-match text
function searchTextFor(cmd: Command, prefix?: string): string | undefined {
  if (!prefix && !cmd.keywords?.length && !cmd.aliases?.length) return undefined
  return [prefix, cmd.label, cmd.description, ...(cmd.keywords ?? []), ...(cmd.aliases ?? [])].filter(Boolean).join(' ')
}

function commandToItem(cmd: Command): PaletteItem {
  return {
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.isFolder ? undefined : cmd.description,
    icon: cmd.icon,
    iconPath: cmd.iconPath,
    isFolder: cmd.isFolder,
    source: cmd.source,
    actionLabel: cmd.actionLabel,
    searchText: searchTextFor(cmd),
    keywords: cmd.keywords,
    data: cmd.data ?? cmd.id,
    color: cmd.color,
    accessories: cmd.accessories,
    metadata: cmd.metadata,
    liveOutputKey: cmd.liveOutputKey,
    running: cmd.running,
  }
}

// Hierarchical root view: folders first, then root scripts
function commandsToItems(commands: Command[]): PaletteItem[] {
  return commands.map(commandToItem)
}

// Flat view for cross-folder search: all scripts with folder as sublabel + searchText
function commandsToFlatItems(commands: Command[]): PaletteItem[] {
  return commands.map(cmd => ({
    ...commandToItem(cmd),
    sublabel: cmd.folderName,
    searchText: searchTextFor(cmd, cmd.folderName),
  }))
}

// Fallback commands shown when a root query matches nothing — so the palette
// is never a dead end. Injected into the results list (keyboard-navigable,
// unlike a static empty-state message). Web/GitHub open a browser; "files"
// hands off to the @find mode so the query keeps refining in-place.
function buildFallbackItems(query: string): PaletteItem[] {
  const q = query.trim()
  if (!q) return []
  const data = (kind: string) => ({ kind, q }) as unknown
  return [
    { id: 'fallback:web', label: `Search the web for “${q}”`, icon: 'search', source: 'builtin', data: data('web'), actionLabel: 'Open' },
    { id: 'fallback:files', label: `Search files for “${q}”`, icon: 'folder', source: 'builtin', data: data('files'), actionLabel: 'Search' },
    { id: 'fallback:github', label: `Search GitHub for “${q}”`, icon: 'search', source: 'builtin', data: data('github'), actionLabel: 'Open' },
  ]
}

// ── Overrides (aliases & pins) ────────────────────────────────────────────────

type Overrides = Record<string, CommandOverride>

// Fold a user alias into the item's search text and display metadata
function applyOverride(item: PaletteItem, ov?: CommandOverride): PaletteItem {
  if (!ov?.alias) return item
  return {
    ...item,
    searchText: `${item.searchText ?? item.label} ${ov.alias}`,
  }
}

// Alias-prefix matches sort above everything else; shorter aliases win ties.
function aliasPrefixRank(query: string, ov?: CommandOverride): { tier: number; len: number } | null {
  if (!ov?.alias) return null
  const alias = ov.alias.toLowerCase()
  const q = query.trim().toLowerCase()
  if (alias === q) return { tier: 0, len: alias.length }
  if (alias.startsWith(q)) return { tier: 1, len: alias.length }
  return null
}

// ── Query ranking ─────────────────────────────────────────────────────────────

// Weighted fields for multi-field fuzzy scoring: label is the strongest signal,
// sublabel weaker, and the full search text (description, folder, keywords)
// weakest — enough to surface a match without outranking label hits.
const RANK_FIELDS = [
  { text: (item: PaletteItem) => item.label, weight: 1.0 },
  { text: (item: PaletteItem) => item.sublabel, weight: 0.5 },
  { text: (item: PaletteItem) => item.searchText, weight: 0.35 },
]

// Root query results: scripts and provider results ranked together by weighted
// fuzzy score, hard bonuses for exact/prefix label matches, alias matches,
// pins, and frecency. Alias-prefix matches are hoisted above everything.
// Array.sort is stable, so ties preserve input order (no row flicker).
function buildQueryResults(items: PaletteItem[], query: string, overrides: Overrides): PaletteItem[] {
  const q = query.trim().toLowerCase()
  const baseScores = fuzzyScoreFieldsBatch(items, query, RANK_FIELDS)
  const ranked = items
    .map(item => {
      const baseScore = baseScores.get(item)
      if (baseScore === undefined) return null
      let score = baseScore
      const label = item.label.toLowerCase()
      if (label === q) score += 300
      else if (label.startsWith(q)) score += 120
      else if (label.includes(q)) score += 40

      const ov = overrides[item.id]
      const alias = ov?.alias?.toLowerCase()
      if (alias) {
        if (alias === q) score += 200
        else if (alias.startsWith(q)) score += 80
        else if (alias.includes(q)) score += 25
      }

      score += frecencyBonus(item.id)
      if (ov?.pinned) score += 10

      return {
        item,
        score,
        aliasRank: aliasPrefixRank(query, ov),
        // Scripts/shortcuts from the commands folder always sort above
        // provider results (calculator, kill, …)
        scriptTier: item.source === 'script' ? 0 : 1,
      }
    })
    .filter((r): r is NonNullable<typeof r> => r !== null)
  ranked.sort((a, b) => {
    if (a.aliasRank && b.aliasRank) {
      if (a.aliasRank.tier !== b.aliasRank.tier) return a.aliasRank.tier - b.aliasRank.tier
      return a.aliasRank.len - b.aliasRank.len
    }
    if (a.aliasRank) return -1
    if (b.aliasRank) return 1
    if (a.scriptTier !== b.scriptTier) return a.scriptTier - b.scriptTier
    return b.score - a.score
  })
  return ranked.map(r => r.item)
}

// ── Reducer ───────────────────────────────────────────────────────────────────

function initialState(_config: AppConfig): PaletteState {
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

function reducer(state: PaletteState, action: PaletteAction): PaletteState {
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

    case 'MOVE_SELECTION':
      return {
        ...state,
        selectedIndex: Math.max(0, state.selectedIndex + action.delta),
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

// ── Component ─────────────────────────────────────────────────────────────────

const LAST_CMD_KEY = 'commandeer:last'

// Root-level @ prefixes. Typing '@' (or a partial token) lists these as
// suggestions; a completed token followed by a space activates the mode.
//   @find   → global file search (FTS5 index → Everything → walkdir)
//   @search → file search in the focused Explorer/Finder folder
//   @web    → web search in the browser
const AT_PREFIXES = [
  { token: '@find', icon: 'folder', description: 'Find files across your computer' },
  { token: '@search', icon: 'folder', description: IS_LINUX ? 'Search your home folder' : IS_MAC ? 'Search the focused Finder folder' : 'Search the focused Explorer folder' },
  { token: '@web', icon: 'search', description: 'Search the web' },
  { token: '@calc', icon: 'calculator', description: 'Calculate an expression (40+2, 100 usd to eur)' },
  { token: '@time', icon: 'clock', description: 'Convert time zones (4pm bst to est)' },
]

// Base (unscaled) logical width of the palette window. The scale factor
// multiplies this for the window size and is applied as a CSS zoom on the
// content, so the whole palette grows/shrinks uniformly.
const PALETTE_WIDTH = 669

// Debounce between keystrokes and the global-search IPC round trip
const FIND_DEBOUNCE_MS = 120

// Debounce between keystrokes and the provider search fan-out (kill <name>,
// calculator, apps, …)
const PROVIDER_DEBOUNCE_MS = 150


// An inline script the palette polls on a timer: its captured stdout replaces
// the row's sublabel live (at render time, outside the ranked search text so
// refreshes never re-rank the list).
export interface InlineScript {
  path: string
  refreshSeconds: number
}

interface PaletteProps {
  config: AppConfig
  commands: Command[]
  // Palette scale factor applied as a CSS zoom (1.0 = default). Drives the
  // window width/height so the whole palette grows/shrinks uniformly.
  scale: number
  inlineScripts: InlineScript[]
  onConfigChange: (config: AppConfig) => void
  resetRef: MutableRefObject<(() => void) | null>
  commandHotkeyRef?: MutableRefObject<((commandId: string) => void) | null>
  onToggleGameMode: () => void
  gameModeEnabled: boolean
  claudeUsageVisible: boolean
  systemStatsVisible: boolean
}

export default function Palette({
  config,
  commands,
  scale,
  inlineScripts,
  onConfigChange: _onConfigChange,
  resetRef,
  commandHotkeyRef,
  onToggleGameMode,
  gameModeEnabled,
  claudeUsageVisible,
  systemStatsVisible,
}: PaletteProps) {
  const [state, dispatch] = useReducer(reducer, config, initialState)
  const [sliderValue, setSliderValue] = useState(0)
  const [providerCommands, setProviderCommands] = useState<Command[]>([])
  const [actionPanelOpen, setActionPanelOpen] = useState(false)
  const [actionPanelIndex, setActionPanelIndex] = useState(0)
  const [formValues, setFormValues] = useState<Record<string, unknown>>({})
  const [toasts, setToasts] = useState<ToastMessage[]>([])
  // Live-captured stdout for inline scripts, keyed by script path. Replaces
  // the row's sublabel at render time (see displayItems). Stored outside the
  // reducer so refreshes never re-rank the list.
  const [inlineOutputs, setInlineOutputs] = useState<Record<string, string>>({})
  // Whether the palette window is focused — polling pauses while hidden so we
  // don't run user scripts in the background.
  const [windowFocused, setWindowFocused] = useState(true)
  const inlineTimersRef = useRef<number[]>([])
  const toastIdRef = useRef(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const configRef = useRef(config)
  const commandsRef = useRef(commands)
  const providerCommandsRef = useRef(providerCommands)
  const providerRequestRef = useRef(0)
  const providerTimeoutRef = useRef<number | null>(null)
  const scaleRef = useRef(scale)
  configRef.current = config
  commandsRef.current = commands
  providerCommandsRef.current = providerCommands
  scaleRef.current = scale

  const toast = useCallback((message: string, kind: ToastKind = 'info') => {
    const id = ++toastIdRef.current
    setToasts(prev => [...prev, { id, message, kind }])
    window.setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id))
    }, 2000)
  }, [])

  // Expose reset function to App and the toast helper to the rest of the app
  useEffect(() => {
    resetRef.current = () => dispatch({ type: 'RESET' })
    appEvents.toast = toast
    return () => { appEvents.toast = undefined }
  }, [resetRef, toast])

  // Re-run an inline script and update its live sublabel. Used by the polling
  // timers and by Enter on an inline row (force-refresh). On error the
  // previous output is kept (first failure shows an ellipsis).
  const refreshInline = useCallback(async (path: string) => {
    try {
      const out = await runScriptCapture(path)
      setInlineOutputs(prev => (prev[path] === out ? prev : { ...prev, [path]: out }))
    } catch {
      setInlineOutputs(prev => (path in prev ? prev : { ...prev, [path]: '…' }))
    }
  }, [])

  // Pause polling while the palette is hidden (focus loss auto-hides it) so we
  // don't keep running user scripts in the background.
  useEffect(() => {
    let unlisten: (() => void) | undefined
    ;(async () => {
      unlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        setWindowFocused(focused)
      })
    })()
    return () => { unlisten?.() }
  }, [])

  // Seed + poll each inline script on its @vicinae.refreshTime interval. Only
  // runs while focused; re-seeds on re-focus.
  useEffect(() => {
    inlineTimersRef.current.forEach(clearInterval)
    inlineTimersRef.current = []
    if (!windowFocused) return
    for (const s of inlineScripts) {
      void refreshInline(s.path)
      const id = window.setInterval(() => { void refreshInline(s.path) }, Math.max(1, s.refreshSeconds) * 1000)
      inlineTimersRef.current.push(id)
    }
    return () => { inlineTimersRef.current.forEach(clearInterval); inlineTimersRef.current = [] }
  }, [inlineScripts, windowFocused, refreshInline])

  // Commands can come from the static list (scripts, settings) or
  // from a provider's per-query search results
  const resolveCommand = useCallback((id: string): Command | undefined => {
    return commandsRef.current.find(c => c.id === id)
      ?? providerCommandsRef.current.find(c => c.id === id)
  }, [])

  // Per-command user overrides (alias, pin, hotkey) from overrides.json
  const [overrides, setOverrides] = useState<Overrides>({})
  const overridesRef = useRef(overrides)
  overridesRef.current = overrides

  useEffect(() => {
    getOverrides().then(setOverrides).catch(console.error)
  }, [])

  const refreshOverrides = useCallback(async () => {
    setOverrides(await getOverrides())
  }, [])

  // Global per-command shortcut (or deep link) fired: show the palette and run
  // the command's action or push its root step
  const handleCommandHotkey = useCallback(async (commandId: string) => {
    const win = getCurrentWindow()
    await win.show()
    await win.setFocus()

    const cmd = resolveCommand(commandId)
    if (!cmd) return

    if (cmd.action) {
      try {
        await cmd.action(configRef.current)
        recordUse(cmd.id)
        if (!cmd.noClose) {
          dispatch({ type: 'RESET' })
          await win.hide()
        }
      } catch (err) {
        dispatch({ type: 'SET_ERROR', error: String(err) })
      }
      return
    }

    if (cmd.createRootStep) {
      recordUse(cmd.id)
      dispatch({ type: 'RESET' })
      dispatch({ type: 'PUSH_STEP', step: cmd.createRootStep(configRef.current) })
    }
  }, [resolveCommand])

  useEffect(() => {
    if (!commandHotkeyRef) return
    commandHotkeyRef.current = handleCommandHotkey
    return () => { commandHotkeyRef.current = null }
  }, [commandHotkeyRef, handleCommandHotkey])

  // Reinitialise root items when the commands list or overrides change. The
  // Settings command is kept out of both lists — it's appended as the
  // always-last row below.
  useEffect(() => {
    const lastId = localStorage.getItem(LAST_CMD_KEY)
    const withOverrides = (items: PaletteItem[]) =>
      items.map(i => applyOverride(i, overrides[i.id]))

    // Hierarchical view: everything from the user's commands folder first
    // (script folders, then loose scripts with last-used floating up), then
    // built-in virtual folders (Apps, System, Tools) and any other builtins.
    // searchOnly commands are excluded here but stay in the flat search list.
    const isScript = (c: Command) => c.source === 'script'
    const rootLoose = commands.filter(c =>
      !c.isFolder &&
      !c.searchOnly &&
      c.id !== SETTINGS_COMMAND_ID &&
      (!c.folderName || overrides[c.id]?.showAtRoot)
    )
    const scriptFolders = commands.filter(c => c.isFolder && isScript(c))
    const builtinFolders = commands.filter(c => c.isFolder && !isScript(c))
    const scriptLoose = rootLoose.filter(isScript)
    const builtinLoose = rootLoose.filter(c => !isScript(c))
    const sortedScripts = lastId
      ? [...scriptLoose].sort((a, b) => (a.id === lastId ? -1 : b.id === lastId ? 1 : 0))
      : scriptLoose
    dispatch({
      type: 'SET_ITEMS',
      stepId: '__root__',
      items: withOverrides(commandsToItems([...scriptFolders, ...sortedScripts, ...builtinFolders, ...builtinLoose])),
      preserveSelection: true,
    })

    // Flat view: all scripts (no folder nav items) for cross-folder search
    const allScripts = commands.filter(c => !c.isFolder && c.id !== SETTINGS_COMMAND_ID)
    dispatch({ type: 'SET_ITEMS', stepId: '__root_flat__', items: withOverrides(commandsToFlatItems(allScripts)), preserveSelection: true })
  }, [commands, overrides])

  // Current step key
  const currentStep = state.stepStack[state.stepStack.length - 1] ?? null
  const cacheKey = currentStep?.id ?? '__root__'

  // Parse an @-prefixed root query: '@'/'@fi' → suggestion mode; '@find rest'
  // (completed token + space) → the corresponding mode with `rest` as query.
  const atRaw = !currentStep && state.query.startsWith('@') ? state.query : null
  const atSpaceIdx = atRaw?.indexOf(' ') ?? -1
  const atToken = atRaw ? (atSpaceIdx >= 0 ? atRaw.slice(0, atSpaceIdx) : atRaw).toLowerCase() : null
  const atRest = atRaw && atSpaceIdx >= 0 ? atRaw.slice(atSpaceIdx + 1) : ''
  const atComplete = atToken !== null && atSpaceIdx >= 0 && AT_PREFIXES.some(p => p.token === atToken)
  const atSuggestMode = atRaw !== null && !atComplete

  // "@search" → file search in the active Explorer folder. The file list is
  // fetched once per palette show (walked in parallel on the Rust side) and
  // cached; keystrokes then filter it client-side.
  const folderMode = atComplete && atToken === '@search'
  const folderQuery = folderMode ? atRest.trimStart() : ''
  const folderLoad = useRef({ token: 0, loaded: false, loading: false })

  useEffect(() => {
    if (!folderMode) return
    const fl = folderLoad.current
    if (fl.loaded || fl.loading) return
    fl.loading = true
    const token = fl.token
    dispatch({ type: 'SET_LOADING', loading: true })
    loadActiveFolderItems()
      .then(items => {
        if (folderLoad.current.token !== token) return
        folderLoad.current.loaded = true
        dispatch({ type: 'SET_ITEMS', stepId: '__folder__', items })
      })
      .catch(err => {
        if (folderLoad.current.token !== token) return
        dispatch({ type: 'SET_ERROR', error: String(err) })
      })
      .finally(() => { folderLoad.current.loading = false })
  }, [folderMode])

  // "@find" → global file search. Unlike the folder search this hits the
  // backend per (debounced) keystroke — the index does the narrowing — and
  // results arrive pre-ranked by the fzf scorer + relevance multipliers.
  const findMode = atComplete && atToken === '@find'
  const findQuery = findMode ? atRest.trimStart() : ''
  const findToken = useRef(0)

  // "@web" → a single row that opens the browser search
  const webMode = atComplete && atToken === '@web'
  const webQuery = webMode ? atRest.trim() : ''

  // "@calc" / "@time" → evaluate the rest of the query live; Enter copies the
  // result without closing the palette
  const calcMode = atComplete && atToken === '@calc'
  const calcQuery = calcMode ? atRest.trim() : ''
  const timeMode = atComplete && atToken === '@time'
  const timeQuery = timeMode ? atRest.trim() : ''

  useEffect(() => {
    if (!findMode) return
    const token = ++findToken.current
    if (!findQuery.trim()) {
      dispatch({ type: 'SET_ITEMS', stepId: '__global__', items: [] })
      return
    }
    dispatch({ type: 'SET_LOADING', loading: true })
    const timer = setTimeout(() => {
      loadGlobalFileResults(findQuery, configRef.current)
        .then(items => {
          if (findToken.current !== token) return
          dispatch({ type: 'SET_ITEMS', stepId: '__global__', items })
        })
        .catch(err => {
          if (findToken.current !== token) return
          dispatch({ type: 'SET_ERROR', error: String(err) })
        })
    }, FIND_DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [findMode, findQuery])

  // Debounced provider search: dynamic per-query results (kill <name>,
  // calculator, …) surfaced inline at root. Skipped inside steps and for any
  // @-prefixed query, whose mode owns the whole result list.
  useEffect(() => {
    const requestId = ++providerRequestRef.current
    if (providerTimeoutRef.current) window.clearTimeout(providerTimeoutRef.current)
    if (currentStep || atRaw !== null || !state.query.trim()) {
      setProviderCommands([])
      return
    }
    providerTimeoutRef.current = window.setTimeout(async () => {
      try {
        const cmds = await searchAllProviders(state.query, configRef.current)
        if (requestId === providerRequestRef.current) setProviderCommands(cmds)
      } catch (err) {
        console.error(err)
      }
    }, PROVIDER_DEBOUNCE_MS)
    return () => {
      if (providerTimeoutRef.current) window.clearTimeout(providerTimeoutRef.current)
    }
  }, [state.query, currentStep, atRaw])

  // Close the action panel when the selection, query, or step changes
  useEffect(() => {
    setActionPanelOpen(false)
    setActionPanelIndex(0)
  }, [state.selectedIndex, state.query, currentStep])

  // Initialize form field defaults when a form step is pushed
  useEffect(() => {
    if (!currentStep?.isFormStep) return
    const defaults: Record<string, unknown> = {}
    for (const field of currentStep.fields ?? []) {
      if (field.defaultValue !== undefined) defaults[field.id] = field.defaultValue
      else if (field.type === 'checkbox') defaults[field.id] = false
      else if (field.type === 'dropdown' && field.options?.length) defaults[field.id] = field.options[0].value
      else defaults[field.id] = ''
    }
    setFormValues(defaults)
  }, [currentStep])

  // Load items when a new step is pushed or replaced. Keyed on the step object
  // (not its id) so a REPLACE_STEP with the same id still reloads.
  useEffect(() => {
    if (!currentStep?.load) return
    dispatch({ type: 'SET_LOADING', loading: true })
    currentStep.load(configRef.current)
      // preserveSelection: PUSH/POP already reset the index to 0; same-id
      // REPLACEs deliberately keep the highlighted row across the reload
      .then(items => dispatch({ type: 'SET_ITEMS', stepId: currentStep.id, items, preserveSelection: true }))
      .catch(err => dispatch({ type: 'SET_ERROR', error: String(err) }))
  }, [currentStep]) // eslint-disable-line react-hooks/exhaustive-deps

  // Notify the step when it leaves the top of the stack (pop, replace,
  // reset/hide) so uncommitted previews can be undone
  useEffect(() => {
    return () => { currentStep?.onExit?.() }
  }, [currentStep])

  // Initialize slider position when a slider step is pushed: seed from the
  // step's loadSliderValue (current volume, stored transparency, …), showing
  // min until it resolves.
  useEffect(() => {
    if (!currentStep?.isSliderStep) return
    setSliderValue(currentStep.minValue ?? 0)
    if (!currentStep.loadSliderValue) return
    let cancelled = false
    currentStep.loadSliderValue()
      .then(value => { if (!cancelled) setSliderValue(value) })
      .catch(err => console.error('loadSliderValue failed:', err))
    return () => { cancelled = true }
  }, [currentStep])

  // Derived filtered items
  // At root with a query: search the flat list (all scripts across all folders)
  // At root without a query, or inside a step: use the current step's items
  const rawItems = currentStep
    ? (state.itemCache[cacheKey] ?? [])
    : folderMode
      ? (state.itemCache['__folder__'] ?? [])
      : findMode
        ? (state.itemCache['__global__'] ?? [])
        : state.query
          ? (state.itemCache['__root_flat__'] ?? [])
          : (state.itemCache['__root__'] ?? [])
  const isInputStep = currentStep?.isInputStep ?? false
  const isSliderStep = currentStep?.isSliderStep ?? false
  const isFormStep = currentStep?.isFormStep ?? false
  const isGridStep = currentStep?.isGridStep ?? false

  // Live preview shown on the right side of the search input for calculator /
  // time-zone modes (root prefixes and Tools input steps).
  const previewResult = (() => {
    if (currentStep?.livePreview) return currentStep.livePreview(state.query)
    if (calcMode && calcQuery) {
      const r = evaluateCalcQuery(calcQuery)
      return r ? { label: r.display, sublabel: r.sublabel, copy: r.copy } : null
    }
    if (timeMode && timeQuery) {
      const r = tryTimeConversion(timeQuery)
      return r ? { label: r.label, sublabel: r.sublabel, copy: r.copy } : null
    }
    return null
  })()

  let matchedItems: PaletteItem[]
  if (isInputStep || isSliderStep || isFormStep) {
    matchedItems = []
  } else if (atSuggestMode) {
    // '@' or a partial token: list the available @ commands; selecting one
    // inserts it into the query instead of executing
    matchedItems = AT_PREFIXES
      .filter(p => p.token.startsWith(atToken ?? '@'))
      .map(p => ({
        id: `at:${p.token}`,
        label: p.token,
        sublabel: p.description,
        icon: p.icon,
        data: p.token,
        actionLabel: 'Use',
      }))
  } else if (webMode) {
    matchedItems = webQuery
      ? [{
          id: `web:${webQuery}`,
          label: `Search the web for "${webQuery}"`,
          sublabel: 'Opens your browser',
          icon: 'search',
          data: webQuery,
          actionLabel: 'Search',
        }]
      : []
  } else if (calcMode || timeMode) {
    // Result is shown inline via previewResult; no list row needed
    matchedItems = []
  } else if (findMode) {
    // Global results are already ranked for this query (fzf + relevance
    // multipliers in globalFileSearch) — re-filtering would fight the ranker
    matchedItems = rawItems
  } else if (currentStep || folderMode) {
    matchedItems = fuzzyFilter(rawItems, folderMode ? folderQuery : state.query, i =>
      i.searchText ?? (i.label + ' ' + (i.sublabel ?? ''))
    )
  } else if (state.query) {
    // Root query: scripts and provider search results (which can share ids —
    // keep the first occurrence) ranked together by fuzzy score + frecency
    const merged = [
      ...rawItems,
      ...providerCommands.map(c => applyOverride(commandToItem(c), overrides[c.id])),
    ]
    const seen = new Set<string>()
    const deduped = merged.filter(i => (seen.has(i.id) ? false : (seen.add(i.id), true)))
    matchedItems = buildQueryResults(deduped, state.query, overrides)
    // Nothing matched: surface actionable fallback rows so the palette is
    // never a dead end (web / files / GitHub).
    if (matchedItems.length === 0) {
      matchedItems = buildFallbackItems(state.query)
    }
  } else {
    // Root browse: folders first, then scripts with last-used floating up —
    // exactly as assembled in the __root__ cache
    matchedItems = rawItems
  }
  const noMatches = matchedItems.length === 0

  const visibleItems = matchedItems.slice(0, 50)
  // Overlay live inline-script outputs onto the displayed rows: an inline
  // item's sublabel becomes the script's captured stdout (or "…" until the
  // first refresh resolves). Done at render time, outside the ranked search
  // text, so a changing output never re-ranks the list mid-tick.
  const displayItems = Object.keys(inlineOutputs).length === 0
    ? visibleItems
    : visibleItems.map(i => {
        const key = i.liveOutputKey
        if (!key) return i
        const out = inlineOutputs[key]
        if (out === undefined) return i
        return { ...i, sublabel: out }
      })
  const clampedIndex = Math.min(state.selectedIndex, Math.max(0, visibleItems.length - 1))
  const selectedItem = displayItems[clampedIndex] ?? null

  // Settings is reachable from a fixed footer button instead of the results list
  const settingsCmd = !currentStep && !isInputStep && atRaw === null
    ? commands.find(c => c.id === SETTINGS_COMMAND_ID)
    : undefined
  const handleOpenSettings = useCallback(() => {
    if (!settingsCmd?.createRootStep) return
    dispatch({ type: 'PUSH_STEP', step: settingsCmd.createRootStep(configRef.current) })
  }, [settingsCmd])
  const primaryAction = previewResult
    ? 'Copy'
    : selectedItem
      ? (selectedItem.actionLabel
        ?? (selectedItem.isFolder
          ? 'Open Folder'
          : selectedItem.id.startsWith('script:') ? 'Run Script' : 'Select'))
      : null

  // Preview pane: shown when the selected item has something to preview
  // (image, text file, color swatch, font, or metadata).
  const showPreview = selectedItem != null

  // Forward ref so buildActions (defined before handleSelect) can trigger the
  // normal selection path for step rows
  const handleSelectRef = useRef<((item: PaletteItem) => Promise<void>) | null>(null)

  // Re-run the current step's load in place, for action-panel handlers that
  // mutate the data behind the visible list (e.g. deleting a note while
  // inside the Notes folder). Ref so buildActions' stable closure always
  // sees the live step.
  const reloadStepRef = useRef<() => void>(() => {})
  reloadStepRef.current = () => {
    const step = currentStep
    if (!step?.load) return
    step.load(configRef.current)
      .then(items => dispatch({ type: 'SET_ITEMS', stepId: step.id, items, preserveSelection: true }))
      .catch(err => dispatch({ type: 'SET_ERROR', error: String(err) }))
  }

  // Ctrl+K action panel: secondary actions for the highlighted item, keyed off
  // its provider source
  const buildActions = useCallback((item: PaletteItem): ActionItem[] => {
    const actions: ActionItem[] = []
    const cmd = resolveCommand(item.id)

    const pushCopy = (label: string, value: string, shortcut?: string) => {
      actions.push({
        id: 'copy',
        label,
        shortcut,
        icon: 'copy',
        handler: async () => {
          await navigator.clipboard.writeText(value)
          toast('Copied to clipboard', 'success')
          await getCurrentWindow().hide()
        },
      })
    }

    const runPrimary = (id: string, label: string) => {
      actions.push({
        id,
        label,
        shortcut: '↵',
        handler: async () => {
          if (cmd?.action) {
            await cmd.action(configRef.current)
            if (!cmd.noClose) await getCurrentWindow().hide()
          } else if (cmd?.createRootStep) {
            dispatch({ type: 'PUSH_STEP', step: cmd.createRootStep(configRef.current) })
          } else {
            // Step rows aren't commands — fall back to the normal selection
            // path so the step's onSelect runs
            await handleSelectRef.current?.(item)
          }
        },
      })
    }

    switch (item.source) {
      case 'file':
        actions.push({
          id: 'open',
          label: 'Open file',
          shortcut: '↵',
          handler: async () => { await openPath(item.data as string); await getCurrentWindow().hide() },
        })
        actions.push({
          id: 'reveal',
          label: IS_MAC ? 'Reveal in Finder' : IS_LINUX ? 'Reveal in File Manager' : 'Reveal in File Explorer',
          shortcut: 'R',
          icon: 'folder',
          handler: async () => { await revealPath(item.data as string); await getCurrentWindow().hide() },
        })
        pushCopy('Copy path', item.data as string, 'C')
        break
      case 'clipboard': {
        const clip = item.data as ClipboardItem
        if (clip && typeof clip === 'object' && 'text' in clip) {
          actions.push({
            id: 'paste',
            label: 'Paste to active app',
            shortcut: '↵',
            handler: async () => {
              try {
                const pasted = await pasteToPrevious(clip.text)
                if (!pasted) toast('Copied — press Ctrl+V to paste', 'success')
              } catch (err) {
                toast('Failed to paste', 'error')
                throw err
              }
            },
          })
          actions.push({
            id: 'copy',
            label: 'Copy to clipboard',
            shortcut: 'C',
            icon: 'copy',
            handler: async () => {
              await writeClipboardText(clip.text)
              toast('Copied to clipboard', 'success')
              await getCurrentWindow().hide()
            },
          })
        } else {
          runPrimary('open', 'Open')
        }
        break
      }
      case 'calculator':
        pushCopy('Copy result', item.label, 'C')
        break
      case 'script':
        runPrimary('run', 'Run script')
        break
      case 'system':
        runPrimary('run', 'Run command')
        break
      case 'quicklink': {
        const q = item.data as Quicklink
        runPrimary('open', 'Open link')
        pushCopy('Copy URL', q.url, 'C')
        actions.push({
          id: 'delete',
          label: 'Delete quick link',
          shortcut: '⌫',
          icon: 'trash',
          handler: async () => {
            const all = await readQuicklinks()
            await writeQuicklinks(all.filter(x => x.id !== q.id))
            appEvents.refreshCommands?.()
            reloadStepRef.current()
            toast('Quick link deleted', 'success')
          },
        })
        break
      }
      case 'note': {
        const n = item.data as Note
        runPrimary('copy', 'Copy note')
        actions.push({
          id: 'delete',
          label: 'Delete note',
          shortcut: '⌫',
          icon: 'trash',
          handler: async () => {
            const all = await readNotes()
            await writeNotes(all.filter(x => x.id !== n.id))
            appEvents.refreshCommands?.()
            reloadStepRef.current()
            toast('Note deleted', 'success')
          },
        })
        break
      }
      case 'bookmark': {
        const b = item.data as Bookmark
        actions.push({
          id: 'open',
          label: 'Open in browser',
          shortcut: '↵',
          handler: async () => { await openUrl(b.url); await getCurrentWindow().hide() },
        })
        pushCopy('Copy URL', b.url, 'C')
        break
      }
      default:
        runPrimary('open', 'Open')
        pushCopy('Copy name', item.label, 'C')
    }

    // Alias, pin & hotkey actions for persistent root commands (not for
    // dynamic step/search rows, whose ids never appear in the root list)
    if (commandsRef.current.some(c => c.id === item.id)) {
      const ov = overridesRef.current[item.id]
      actions.push({
        id: 'pin',
        label: ov?.pinned ? 'Unpin' : 'Pin',
        icon: 'bookmark',
        handler: async () => {
          const pinned = !ov?.pinned
          await setOverride(item.id, { pinned })
          await refreshOverrides()
          toast(pinned ? 'Pinned — boosts search rank' : 'Unpinned', 'success')
        },
      })
      actions.push({
        id: 'show-at-root',
        label: ov?.showAtRoot ? 'Hide from Root' : 'Show in Root',
        icon: 'pin',
        handler: async () => {
          const showAtRoot = !ov?.showAtRoot
          await setOverride(item.id, { showAtRoot })
          await refreshOverrides()
          toast(showAtRoot ? 'Shown on main page' : 'Hidden from main page', 'success')
        },
      })
      actions.push({
        id: 'alias',
        label: ov?.alias ? `Change Alias (${ov.alias})` : 'Set Alias…',
        icon: 'edit',
        handler: async () => {
          dispatch({
            type: 'PUSH_STEP',
            step: {
              id: `overrides:alias:${item.id}`,
              label: `Alias: ${item.label}`,
              placeholder: 'Type an alias (leave empty to clear)…',
              isInputStep: true,
              onSelect: async () => ({ type: 'done' }),
              onCommitQuery: async (query) => {
                await setOverride(item.id, { alias: query.trim() || undefined })
                await refreshOverrides()
                return { type: 'pop' }
              },
            },
          })
        },
      })
      actions.push({
        id: 'hotkey',
        label: ov?.hotkey ? `Change Hotkey (${ov.hotkey})` : 'Set Global Hotkey…',
        icon: 'keyboard',
        handler: async () => {
          dispatch({
            type: 'PUSH_STEP',
            step: {
              id: `overrides:hotkey:${item.id}`,
              label: `Hotkey: ${item.label}`,
              placeholder: 'e.g. Ctrl+Alt+L (leave empty to clear)…',
              isInputStep: true,
              onSelect: async () => ({ type: 'done' }),
              onCommitQuery: async (query) => {
                await setCommandHotkey(item.id, query.trim() || null)
                // The backend wrote overrides.json directly — drop the cache
                // so the action label reflects the new hotkey immediately
                invalidateOverridesCache()
                await refreshOverrides()
                return { type: 'pop' }
              },
            },
          })
        },
      })
    }

    return actions
  }, [resolveCommand, toast, refreshOverrides])

  const actionItems = selectedItem && !isInputStep && !isSliderStep && !isFormStep
    ? buildActions(selectedItem)
    : []
  const actionPanelClampedIndex = Math.min(actionPanelIndex, Math.max(0, actionItems.length - 1))

  const handleFormSubmit = useCallback(async () => {
    if (!currentStep?.isFormStep || !currentStep.onSubmit) return
    try {
      const result = await currentStep.onSubmit(formValues, configRef.current)
      if (result.type === 'done') {
        dispatch({ type: 'RESET' })
        await getCurrentWindow().hide()
      } else if (result.type === 'push') {
        dispatch({ type: 'PUSH_STEP', step: result.step })
      } else if (result.type === 'replace') {
        dispatch({ type: 'REPLACE_STEP', step: result.step })
      } else if (result.type === 'pop') {
        dispatch({ type: 'POP_STEP' })
      }
    } catch (err) {
      dispatch({ type: 'SET_ERROR', error: String(err) })
    }
  }, [currentStep, formValues])

  // Live preview on highlight change (arrow keys or hover). The first
  // highlight after a step mounts/reloads is skipped — it's the default
  // selection, not the user moving.
  const highlightReady = useRef(false)
  useEffect(() => { highlightReady.current = false }, [currentStep])
  useEffect(() => {
    if (!currentStep?.onHighlight || !selectedItem) return
    if (!highlightReady.current) {
      highlightReady.current = true
      return
    }
    currentStep.onHighlight(selectedItem)
  }, [selectedItem]) // eslint-disable-line react-hooks/exhaustive-deps

  // Keyboard handler
  const handleKeyDown = useCallback(async (e: React.KeyboardEvent) => {
    // Action panel mode: it owns the keyboard until closed
    if (actionPanelOpen) {
      if (e.key === 'Escape') {
        e.preventDefault()
        setActionPanelOpen(false)
        setActionPanelIndex(0)
        return
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setActionPanelIndex(i => Math.min(i + 1, Math.max(0, actionItems.length - 1)))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setActionPanelIndex(i => Math.max(0, i - 1))
        return
      }

      const runAction = async (action: ActionItem) => {
        if (selectedItem) recordUse(selectedItem.id)
        try { await action.handler() } catch (err) { dispatch({ type: 'SET_ERROR', error: String(err) }) }
        setActionPanelOpen(false)
        setActionPanelIndex(0)
      }

      if (e.key === 'Enter') {
        e.preventDefault()
        const action = actionItems[actionPanelClampedIndex]
        if (action) await runAction(action)
        else { setActionPanelOpen(false); setActionPanelIndex(0) }
        return
      }
      // Number shortcuts 1-9
      const digit = parseInt(e.key, 10)
      if (!Number.isNaN(digit) && digit >= 1 && digit <= actionItems.length) {
        e.preventDefault()
        await runAction(actionItems[digit - 1])
        return
      }
      // Letter shortcut matching action.shortcut
      if (/^[a-z]$/i.test(e.key)) {
        const action = actionItems.find(a => a.shortcut?.toLowerCase() === e.key.toLowerCase())
        if (action) {
          e.preventDefault()
          await runAction(action)
          return
        }
      }
      return
    }

    if (e.key === 'Escape') {
      e.preventDefault()
      // Esc walks back through menus one level at a time (sliders apply
      // live, so the adjusted value is kept); it only hides the launcher
      // from the root screen.
      if (state.stepStack.length > 0) {
        dispatch({ type: 'POP_STEP' })
        return
      }
      dispatch({ type: 'RESET' })
      await getCurrentWindow().hide()
      return
    }

    // Let native form inputs handle their own keystrokes (Enter to move/submit,
    // arrows to navigate fields). The main search input is exempt: nav keys
    // must drive the results list from here.
    const target = e.target as HTMLElement
    const isSearchInput =
      target instanceof HTMLInputElement && target.dataset.paletteSearch !== undefined
    if (
      !isSearchInput &&
      (target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement)
    ) {
      return
    }

    if (e.key.toLowerCase() === 'g' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault()
      onToggleGameMode()
      return
    }

    if (e.key.toLowerCase() === 'k' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault()
      if (selectedItem && actionItems.length > 0) {
        setActionPanelOpen(true)
        setActionPanelIndex(0)
      }
      return
    }

    if (e.key === ',' && (e.ctrlKey || e.metaKey) && settingsCmd?.createRootStep) {
      e.preventDefault()
      handleOpenSettings()
      return
    }

    if (e.key === 'Backspace' && !state.query) {
      e.preventDefault()
      if (state.stepStack.length > 0) {
        dispatch({ type: 'POP_STEP' })
      } else {
        dispatch({ type: 'RESET' })
        await getCurrentWindow().hide()
      }
      return
    }

    // Slider steps: arrows nudge the value by stepValue (applies live)
    if (isSliderStep && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
      e.preventDefault()
      const min = currentStep?.minValue ?? 0
      const max = currentStep?.maxValue ?? 100
      const stepBy = currentStep?.stepValue ?? 1
      const delta = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? stepBy : -stepBy
      const next = Math.min(max, Math.max(min, sliderValue + delta))
      if (next !== sliderValue) {
        setSliderValue(next)
        currentStep?.onSliderChange?.(next, configRef.current).catch(err => {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        })
      }
      return
    }

    // Left/Right walk menus when the query is empty (with text they keep
    // moving the caret): Left goes back a level, Right enters the selected
    // item's submenu. Right never *runs* anything — only items that open a
    // step (folders, device lists, settings pages) respond, so arrowing
    // around can't fire an action.
    if (e.key === 'ArrowLeft' && !state.query && !isGridStep) {
      e.preventDefault()
      if (state.stepStack.length > 0) dispatch({ type: 'POP_STEP' })
      return
    }
    if (e.key === 'ArrowRight' && !state.query && !isGridStep) {
      const selected = visibleItems[clampedIndex]
      const opensSubmenu = selected
        ? (currentStep ? selected.isFolder === true : !!resolveCommand(selected.id)?.createRootStep)
        : false
      if (selected && opensSubmenu) {
        e.preventDefault()
        await handleSelect(selected)
      }
      return
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const next = Math.min(clampedIndex + 1, Math.max(0, visibleItems.length - 1))
      dispatch({ type: 'MOVE_SELECTION', delta: next - state.selectedIndex })
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      const next = Math.max(0, clampedIndex - 1)
      dispatch({ type: 'MOVE_SELECTION', delta: next - state.selectedIndex })
      return
    }

    if (e.key === 'Enter') {
      e.preventDefault()

      // Slider step: the value is already applied; Enter confirms and goes back
      if (isSliderStep) {
        dispatch({ type: 'POP_STEP' })
        return
      }

      // Input step: commit raw query
      if (isInputStep && currentStep?.onCommitQuery) {
        try {
          const result = await currentStep.onCommitQuery(state.query, configRef.current)
          if (result.type === 'done') {
            dispatch({ type: 'RESET' })
            await getCurrentWindow().hide()
          } else if (result.type === 'push') {
            dispatch({ type: 'PUSH_STEP', step: result.step })
          } else if (result.type === 'replace') {
            dispatch({ type: 'REPLACE_STEP', step: result.step })
          } else if (result.type === 'pop') {
            dispatch({ type: 'POP_STEP' })
          }
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }

      // @calc / @time: copy the inline preview and stay open
      if ((calcMode || timeMode) && previewResult) {
        try {
          await navigator.clipboard.writeText(previewResult.copy)
          toast('Copied to clipboard', 'success')
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }

      const selected = visibleItems[clampedIndex]
      if (!selected) return
      await handleSelect(selected)
      return
    }
  }, [state, currentStep, isInputStep, isSliderStep, sliderValue, visibleItems, clampedIndex, actionPanelOpen, actionItems, actionPanelClampedIndex, selectedItem, previewResult, calcMode, timeMode]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSelect = useCallback(async (item: PaletteItem) => {
    // Root level: find command and either run action or push step
    if (!currentStep) {
      // @ suggestion: insert the prefix into the query, stay open
      if (item.id.startsWith('at:')) {
        dispatch({ type: 'SET_QUERY', query: `${item.data as string} ` })
        return
      }
      // @calc/@time results: copy and stay open
      if (item.id === 'calc:result' || item.id === 'time:result') {
        try {
          await navigator.clipboard.writeText(item.data as string)
          toast('Copied to clipboard', 'success')
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }
      // @web row: open the browser search
      if (item.id.startsWith('web:')) {
        try {
          await openUrl(`https://www.google.com/search?q=${encodeURIComponent(item.data as string)}`)
          dispatch({ type: 'RESET' })
          await getCurrentWindow().hide()
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }
      // @search/@find results are files, not commands
      if (item.id.startsWith('file:')) {
        try {
          await openFileItem(item)
          recordUse(item.id)
          dispatch({ type: 'RESET' })
          await getCurrentWindow().hide()
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }
      // Inline script: Enter force-refreshes its captured output (re-runs the
      // script and updates the live sublabel). Stays open so the row updates.
      if (item.liveOutputKey) {
        await refreshInline(item.liveOutputKey)
        toast('Refreshed', 'info')
        return
      }
      // Fallback commands (web / files / GitHub) shown when a query matched
      // nothing. Files hands off to @find mode in-place; the rest open a URL.
      if (item.id.startsWith('fallback:')) {
        const data = item.data as { kind: string; q: string }
        try {
          if (data.kind === 'files') {
            dispatch({ type: 'SET_QUERY', query: `@find ${data.q}` })
            return
          }
          const url = data.kind === 'github'
            ? `https://github.com/search?q=${encodeURIComponent(data.q)}`
            : `https://www.google.com/search?q=${encodeURIComponent(data.q)}`
          await openUrl(url)
          recordUse(item.id)
          dispatch({ type: 'RESET' })
          await getCurrentWindow().hide()
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }
      const cmd = resolveCommand(item.id)
      if (!cmd) return
      if (cmd.action) {
        try {
          await cmd.action(configRef.current)
          recordUse(cmd.id)
          if (cmd.noClose) return
          localStorage.setItem(LAST_CMD_KEY, cmd.id)
          dispatch({ type: 'RESET' })
          await getCurrentWindow().hide()
        } catch (err) {
          dispatch({ type: 'SET_ERROR', error: String(err) })
        }
        return
      }
      if (cmd.createRootStep) {
        recordUse(cmd.id)
        const step = cmd.createRootStep(configRef.current)
        dispatch({ type: 'PUSH_STEP', step })
      }
      return
    }

    try {
      const result = await currentStep.onSelect(item, configRef.current)
      if (result.type === 'done') {
        dispatch({ type: 'RESET' })
        await getCurrentWindow().hide()
      } else if (result.type === 'push') {
        dispatch({ type: 'PUSH_STEP', step: result.step })
      } else if (result.type === 'replace') {
        // Same-id replace = the step refreshing itself; keep the user's spot
        dispatch({ type: 'REPLACE_STEP', step: result.step, preserveSelection: result.step.id === currentStep.id })
      } else if (result.type === 'pop') {
        dispatch({ type: 'POP_STEP' })
      }
    } catch (err) {
      dispatch({ type: 'SET_ERROR', error: String(err) })
    }
  }, [currentStep])
  handleSelectRef.current = handleSelect

  // Focus input whenever visible (the container on slider steps, so
  // Escape/Backspace keep working without a text input). Form steps own
  // their own field focus.
  useEffect(() => {
    if (isSliderStep) containerRef.current?.focus()
    else if (!isFormStep) inputRef.current?.focus()
  })

  // Keep the window sized to its content.
  //
  // Windows: a single setSize per height change — the window is positioned
  // once per show (Rust side, top fixed at ~20% of the monitor), so resizes
  // only move the bottom edge and typing stays smooth. A small dead-band
  // skips sub-2px churn; user resizing is prevented by resizable: false in
  // tauri.conf.json. Re-asserted on focus because a size set while hidden
  // isn't always honoured.
  //
  // Linux/Wayland (cosmic-comp): the palette is a layer-shell surface whose
  // size comes from the GTK size request, so resizes go through the backend's
  // resize_palette (in-place, no flicker). Linux/X11 has no layer shell — the
  // window is a normal toplevel positioned by the backend on show, so it uses
  // the same setSize path as Windows.
  // sizeRef is the *unscaled* wrapper we measure; its height already includes
  // the inner zoom (the zoomed content lays out scaled in the wrapper), so it is
  // the final logical window height. Width is derived from the scale directly.
  const sizeRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const lastHeightRef = useRef(0)
  const lastWidthRef = useRef(0)
  const applySize = useCallback(async () => {
    const el = sizeRef.current
    if (!el) return
    const h = Math.ceil(el.getBoundingClientRect().height)
    if (!h) return
    const w = Math.round(PALETTE_WIDTH * scaleRef.current)
    const widthChanged = w !== lastWidthRef.current
    // Skip only when nothing meaningful changed (small height churn while typing
    // is absorbed by the dead-band; a width change always goes through).
    if (!widthChanged && Math.abs(h - lastHeightRef.current) < 2) return
    lastHeightRef.current = h
    if (IS_LINUX && (await envInfo()).wayland) {
      await invoke('resize_palette', { height: h, width: w })
      lastWidthRef.current = w
      return
    }
    const win = getCurrentWindow()
    // setSize keeps the top-left corner fixed, so a width change would grow the
    // window rightward and drift off-center. When the width changes, shift the
    // window left by half the delta so it grows symmetrically about its center.
    if (widthChanged && lastWidthRef.current > 0) {
      try {
        const factor = await win.scaleFactor()
        const pos = await win.outerPosition() // physical px
        const deltaLogical = (w - lastWidthRef.current) / 2
        const x = pos.x / factor - deltaLogical
        const y = pos.y / factor
        await win.setSize(new LogicalSize(w, h))
        await win.setPosition(new LogicalPosition(x, y))
      } catch {
        await win.setSize(new LogicalSize(w, h))
      }
    } else {
      await win.setSize(new LogicalSize(w, h))
    }
    lastWidthRef.current = w
  }, [])

  // Re-apply the window size whenever the scale changes, even if the measured
  // height happens to land within the dead-band (width still needs updating).
  useEffect(() => {
    lastHeightRef.current = 0
    void applySize()
  }, [scale, applySize])

  useEffect(() => {
    const el = sizeRef.current
    if (!el) return
    const observer = new ResizeObserver(() => { void applySize() })
    observer.observe(el)
    let unlisten: (() => void) | undefined
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) {
          // Force a re-apply even if the height didn't change while hidden
          lastHeightRef.current = 0
          void applySize()
          // Drop the cached file list: each show may target a different
          // Explorer folder (and invalidate any in-flight load)
          folderLoad.current = { token: folderLoad.current.token + 1, loaded: false, loading: false }
          dispatch({ type: 'SET_ITEMS', stepId: '__folder__', items: [] })
        }
      })
      .then(fn => { unlisten = fn })
    return () => { observer.disconnect(); unlisten?.() }
  }, [applySize])

  const placeholder = isInputStep
    ? (currentStep?.placeholder ?? 'Enter value...')
    : (currentStep?.placeholder ?? 'Search commands...')

  return (
    // Outer wrapper is unscaled and full-width: we measure its height (which
    // already reflects the inner zoom) to size the window. The inner container
    // is a fixed base width scaled by `zoom`, so it renders at PALETTE_WIDTH ×
    // scale — exactly the window width applySize sets.
    <div ref={sizeRef} style={{ width: '100%' }}>
    <div
      ref={containerRef}
      tabIndex={-1}
      style={{
        outline: 'none',
        position: 'relative',
        width: PALETTE_WIDTH,
        zoom: scale,
        background: 'var(--bg)',
        backdropFilter: 'blur(60px) saturate(180%)',
        WebkitBackdropFilter: 'blur(60px) saturate(180%)',
        // Windows rounds the OS window itself via DWM (DWMWCP_ROUND), so only
        // round in CSS on platforms that don't: macOS 12px (matches the
        // vibrancy radius applied natively), Linux 8px (layer-shell surface
        // has no compositor rounding).
        borderRadius: IS_MAC ? 12 : IS_LINUX ? 8 : undefined,
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'var(--font)',
        overflow: 'hidden',
        color: 'var(--text)',
      }}
      onKeyDown={handleKeyDown}
    >
      <ToastContainer toasts={toasts} />

      {isSliderStep && currentStep ? (
        <SliderInput
          value={sliderValue}
          min={currentStep.minValue ?? 0}
          max={currentStep.maxValue ?? 100}
          step={currentStep.stepValue ?? 1}
          icon={currentStep.icon ?? 'eye'}
          onChange={value => {
            setSliderValue(value)
            currentStep.onSliderChange?.(value, configRef.current).catch(err => {
              dispatch({ type: 'SET_ERROR', error: String(err) })
            })
          }}
        />
      ) : isFormStep ? null : (
        <SearchInput
          ref={inputRef}
          value={state.query}
          placeholder={placeholder}
          loading={state.loading}
          onChange={q => dispatch({ type: 'SET_QUERY', query: q })}
          preview={previewResult}
        />
      )}

      {state.error && (
        <div style={{
          padding: '4px 12px',
          color: '#f7768e',
          fontSize: 12,
          fontFamily: 'var(--font)',
          borderBottom: '1px solid var(--border)',
        }}>
          {state.error}
        </div>
      )}

      {state.stepStack.length > 0 && (
        <StepBreadcrumb steps={state.stepStack} />
      )}

      {!isInputStep && !isSliderStep && !state.loading && noMatches && state.query
        && !(findMode && !findQuery.trim()) && !(webMode && !webQuery)
        && !calcMode && !timeMode && (
        <div style={{
          padding: '12px 14px',
          display: 'flex',
          flexDirection: 'column',
          gap: 6,
        }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            color: 'var(--text-dim)',
            fontSize: 12,
            fontFamily: 'var(--font)',
          }}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.3-4.3" />
            </svg>
            {folderMode || findMode
              ? `No files matching '${folderMode ? folderQuery : findQuery}'`
              : `No commands matching '${state.query}'`}
          </div>
        </div>
      )}

      {!isInputStep && !isSliderStep && !isFormStep && visibleItems.length > 0 && (
        <div style={{ display: 'flex', minHeight: 0 }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            {isGridStep ? (
              <ResultsGrid
                items={displayItems}
                selectedIndex={clampedIndex}
                query={state.query}
                columns={currentStep?.gridColumns}
                onSelect={handleSelect}
                onHover={i => dispatch({ type: 'MOVE_SELECTION', delta: i - clampedIndex })}
              />
            ) : (
              <ResultsList
                items={displayItems}
                selectedIndex={clampedIndex}
                onSelect={handleSelect}
                onHover={i => dispatch({ type: 'MOVE_SELECTION', delta: i - clampedIndex })}
              />
            )}
          </div>
          {showPreview && (
            <DetailPane item={selectedItem} />
          )}
        </div>
      )}

      {isFormStep && currentStep && (
        <FormView
          fields={currentStep.fields ?? []}
          values={formValues}
          onChange={(id, value) => setFormValues(prev => ({ ...prev, [id]: value }))}
          onSubmit={handleFormSubmit}
        />
      )}

      {actionPanelOpen && actionItems.length > 0 && (
        <ActionPanel
          items={actionItems}
          selectedIndex={actionPanelClampedIndex}
          onSelect={async item => {
            if (selectedItem) recordUse(selectedItem.id)
            try { await item.handler() } catch (err) { dispatch({ type: 'SET_ERROR', error: String(err) }) }
            setActionPanelOpen(false)
            setActionPanelIndex(0)
          }}
          onHover={i => setActionPanelIndex(i)}
        />
      )}

      {claudeUsageVisible && <ClaudeUsage />}
      {systemStatsVisible && <SystemStatsPanel />}
      <Footer
        selectedItem={selectedItem}
        primaryAction={primaryAction}
        onOpenSettings={handleOpenSettings}
        settingsVisible={!!settingsCmd}
        gameModeEnabled={gameModeEnabled}
        onToggleGameMode={onToggleGameMode}
      />
    </div>
    </div>
  )
}
