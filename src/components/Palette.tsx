import { useReducer, useEffect, useRef, useState, useCallback, useMemo, MutableRefObject } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { recordUse } from '../lib/frecency'
import { getOverrides } from '../lib/overrides'
import { SETTINGS_COMMAND_ID } from '../commands/settings'
import { VOLUME_MIXER_COMMAND_ID } from '../providers/volume'
import { loadActiveFolderItems, openFileItem } from '../commands/fileSearch'
import { loadGlobalFileResults } from '../commands/globalFileSearch'
import { searchAllProviders } from '../providers'
import { IS_LINUX, IS_MAC, openUrl } from '../lib/tauri'
import type { ActionItem, AppConfig, Command, PaletteItem } from '../types'
import { commandsToItems, commandsToFlatItems } from '../lib/paletteItems'
import { applyOverride, type Overrides } from '../lib/paletteRanking'
import { parseAtQuery, computeMatchedItems, computePreviewResult } from '../lib/paletteModes'
import { buildItemActions } from '../lib/paletteActions'
import { useInlineScripts, type InlineScript } from '../hooks/useInlineScripts'
import { usePaletteWindowSize, DEFAULT_PALETTE_WIDTH, ONIX_PALETTE_WIDTH } from '../hooks/usePaletteWindowSize'
import { usePaletteFeedback } from '../hooks/usePaletteFeedback'
import { clampSelectionIndex, initialState, reducer } from '../lib/paletteReducer'
import { applyStepResult } from '../lib/paletteNavigation'
import SearchInput, { SliderInput } from './SearchInput'
import ResultsList from './ResultsList'
import ResultsGrid from './ResultsGrid'
import FormView from './FormView'
import ActionPanel from './ActionPanel'
import DetailPane from './DetailPane'
import { ToastContainer } from './Toast'
import HudOverlay from './HudOverlay'
import ConfirmOverlay from './ConfirmOverlay'
import ClaudeUsage from './ClaudeUsage'
import CodexUsage from './CodexUsage'
import SystemStatsPanel from './SystemStats'
import Footer from './Footer'
import StepBreadcrumb from './StepBreadcrumb'
import VolumeMixer from './VolumeMixer'
// ── Component ─────────────────────────────────────────────────────────────────

const LAST_CMD_KEY = 'commandeer:last'

// Debounce between keystrokes and the global-search IPC round trip
const FIND_DEBOUNCE_MS = 120

// Debounce between keystrokes and the provider search fan-out (kill <name>,
// calculator, apps, …)
const PROVIDER_DEBOUNCE_MS = 150

const invalidateSequence = (sequence: MutableRefObject<number>) => {
  sequence.current++
}

export type { InlineScript }

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
  codexUsageVisible: boolean
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
  codexUsageVisible,
  systemStatsVisible,
}: PaletteProps) {
  const [state, dispatch] = useReducer(reducer, config, initialState)
  const [sliderValue, setSliderValue] = useState(0)
  const [providerCommands, setProviderCommands] = useState<Command[]>([])
  const [actionPanelOpen, setActionPanelOpen] = useState(false)
  const [actionPanelIndex, setActionPanelIndex] = useState(0)
  // Submenu navigation within the action panel: each entry is a nested menu the
  // user drilled into (empty = the root action list). See ActionItem.submenu.
  const [actionMenuStack, setActionMenuStack] = useState<ActionItem[]>([])
  const [formValues, setFormValues] = useState<Record<string, unknown>>({})
  // Live-refreshing inline scripts: captured stdout overlays each row's
  // sublabel at render time (see displayItems), kept outside the reducer so
  // refreshes never re-rank the list.
  const { inlineOutputs, refreshInline } = useInlineScripts(inlineScripts)
  const inputRef = useRef<HTMLInputElement>(null)
  const configRef = useRef(config)
  const commandsRef = useRef(commands)
  const providerCommandsRef = useRef(providerCommands)
  const providerRequestRef = useRef(0)
  const providerTimeoutRef = useRef<number | null>(null)
  configRef.current = config
  commandsRef.current = commands
  providerCommandsRef.current = providerCommands
  // Onix follows Vicinae's launcher width; Default retains Commandeer's
  // established compact width. Both remain subject to the user's scale.
  const paletteWidth = config.ui_style?.toLowerCase() === 'onix' ? ONIX_PALETTE_WIDTH : DEFAULT_PALETTE_WIDTH
  // Sizes the window to its content and returns the wrapper/container refs.
  const { sizeRef, containerRef } = usePaletteWindowSize(scale, paletteWidth)
  // Toasts, the HUD pill, and the confirm dialog (also registered on appEvents).
  const {
    toast,
    toasts,
    hud,
    showHud,
    requestConfirm,
    resolveConfirm,
    confirmReq,
    confirmRemember,
    setConfirmRemember,
    confirmFocus,
    setConfirmFocus,
    resetFeedback,
  } = usePaletteFeedback(dispatch)

  // Expose a whole-session reset to App. Focus loss must settle confirmations
  // and cancel HUD timers as well as clearing reducer/navigation state.
  useEffect(() => {
    resetRef.current = () => {
      resetFeedback()
      setActionPanelOpen(false)
      setActionPanelIndex(0)
      setActionMenuStack([])
      dispatch({ type: 'RESET' })
    }
    return () => {
      resetRef.current = null
    }
  }, [resetRef, resetFeedback])

  // Commands can come from the static list (scripts, settings) or
  // from a provider's per-query search results
  const resolveCommand = useCallback((id: string): Command | undefined => {
    return commandsRef.current.find(c => c.id === id) ?? providerCommandsRef.current.find(c => c.id === id)
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
  const handleCommandHotkey = useCallback(
    async (commandId: string) => {
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
    },
    [resolveCommand],
  )

  useEffect(() => {
    if (!commandHotkeyRef) return
    commandHotkeyRef.current = handleCommandHotkey
    return () => {
      commandHotkeyRef.current = null
    }
  }, [commandHotkeyRef, handleCommandHotkey])

  // Reinitialise root items when the commands list or overrides change. The
  // Settings command is kept out of both lists — it's appended as the
  // always-last row below.
  useEffect(() => {
    const lastId = localStorage.getItem(LAST_CMD_KEY)
    const withOverrides = (items: PaletteItem[]) => items.map(i => applyOverride(i, overrides[i.id]))

    // Hierarchical view: everything from the user's commands folder first
    // (script folders, then loose scripts with last-used floating up), then
    // built-in virtual folders (Apps, System, Tools) and any other builtins.
    // searchOnly commands are excluded here but stay in the flat search list.
    const isScript = (c: Command) => c.source === 'script'
    const rootLoose = commands.filter(
      c =>
        !c.isFolder && !c.searchOnly && c.id !== SETTINGS_COMMAND_ID && (!c.folderName || overrides[c.id]?.showAtRoot),
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
    dispatch({
      type: 'SET_ITEMS',
      stepId: '__root_flat__',
      items: withOverrides(commandsToFlatItems(allScripts)),
      preserveSelection: true,
    })
  }, [commands, overrides])

  // Current step key
  const currentStep = state.stepStack[state.stepStack.length - 1] ?? null
  const cacheKey = currentStep?.id ?? '__root__'

  // Root @-mode parsing (@find/@search/@web/@calc/@time) — see paletteModes.
  const at = parseAtQuery(state.query, !!currentStep)
  const {
    atRaw,
    folderMode,
    folderQuery,
    findMode,
    findQuery,
    webMode,
    webQuery,
    calcMode,
    calcQuery,
    timeMode,
    timeQuery,
  } = at

  // "@search" → file search in the active Explorer folder. The file list is
  // fetched once per palette show (walked in parallel on the Rust side) and
  // cached; keystrokes then filter it client-side.
  const folderLoad = useRef({ token: 0, loaded: false, loading: false })

  useEffect(() => {
    if (!folderMode) return
    const fl = folderLoad.current
    if (fl.loaded || fl.loading) return
    fl.loading = true
    const token = ++fl.token
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
      .finally(() => {
        if (folderLoad.current.token === token) folderLoad.current.loading = false
      })
    return () => {
      if (folderLoad.current.token !== token) return
      folderLoad.current.token++
      folderLoad.current.loading = false
      dispatch({ type: 'SET_LOADING', loading: false })
    }
  }, [folderMode])

  // "@find" → global file search, one debounced backend call per keystroke; the
  // index does the narrowing and results arrive pre-ranked (fzf + multipliers).
  const findToken = useRef(0)

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
    return () => {
      clearTimeout(timer)
      invalidateSequence(findToken)
      dispatch({ type: 'SET_LOADING', loading: false })
    }
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
    setActionMenuStack([])
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

  const stepLoadSeq = useRef(0)
  const currentStepRef = useRef(currentStep)
  currentStepRef.current = currentStep

  // Load items when a new step is pushed or replaced. Keyed on the step object
  // (not its id) so a REPLACE_STEP with the same id still reloads.
  useEffect(() => {
    const seq = ++stepLoadSeq.current
    const step = currentStep
    if (!step?.load) return
    dispatch({ type: 'SET_LOADING', loading: true })
    step
      .load(configRef.current)
      // preserveSelection: PUSH/POP already reset the index to 0; same-id
      // REPLACEs deliberately keep the highlighted row across the reload
      .then(items => {
        if (stepLoadSeq.current !== seq || currentStepRef.current !== step) return
        dispatch({ type: 'SET_ITEMS', stepId: step.id, items, preserveSelection: true })
      })
      .catch(err => {
        if (stepLoadSeq.current !== seq || currentStepRef.current !== step) return
        dispatch({ type: 'SET_ERROR', error: String(err) })
      })
    return () => {
      invalidateSequence(stepLoadSeq)
    }
  }, [currentStep])

  // Notify the step when it leaves the top of the stack (pop, replace,
  // reset/hide) so uncommitted previews can be undone
  useEffect(() => {
    return () => {
      currentStep?.onExit?.()
    }
  }, [currentStep])

  // Initialize slider position when a slider step is pushed: seed from the
  // step's loadSliderValue (current volume, stored transparency, …), showing
  // min until it resolves.
  useEffect(() => {
    if (!currentStep?.isSliderStep) return
    setSliderValue(currentStep.minValue ?? 0)
    if (!currentStep.loadSliderValue) return
    let cancelled = false
    currentStep
      .loadSliderValue()
      .then(value => {
        if (!cancelled) setSliderValue(value)
      })
      .catch(err => console.error('loadSliderValue failed:', err))
    return () => {
      cancelled = true
    }
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
  const isVolumeMixerStep = currentStep?.isVolumeMixerStep ?? false

  // Live preview shown on the right side of the search input for calculator /
  // time-zone modes (root prefixes and Tools input steps).
  const previewResult = computePreviewResult({
    currentStep,
    query: state.query,
    calcMode,
    calcQuery,
    timeMode,
    timeQuery,
  })

  const matchedItems = useMemo(
    () =>
      computeMatchedItems({
        at,
        currentStep,
        isInputStep,
        isSliderStep,
        isFormStep,
        rawItems,
        query: state.query,
        providerCommands,
        overrides,
      }),
    [at, currentStep, isInputStep, isSliderStep, isFormStep, rawItems, state.query, providerCommands, overrides],
  )
  const noMatches = matchedItems.length === 0

  const visibleItems = useMemo(() => matchedItems.slice(0, 50), [matchedItems])
  // Overlay live inline-script outputs onto the displayed rows: an inline
  // item's sublabel becomes the script's captured stdout (or "…" until the
  // first refresh resolves). Done at render time, outside the ranked search
  // text, so a changing output never re-ranks the list mid-tick.
  const displayItems = useMemo(
    () =>
      Object.keys(inlineOutputs).length === 0
        ? visibleItems
        : visibleItems.map(i => {
            const key = i.liveOutputKey
            if (!key) return i
            const out = inlineOutputs[key]
            if (out === undefined) return i
            return { ...i, sublabel: out }
          }),
    [visibleItems, inlineOutputs],
  )
  const clampedIndex = clampSelectionIndex(state.selectedIndex, visibleItems.length)
  const selectedItem = displayItems[clampedIndex] ?? null

  // Preserve-selection reloads can shrink a list beneath the old index. Keep
  // reducer state aligned with the highlight that is actually rendered so the
  // next pointer/keyboard action cannot target a different row.
  useEffect(() => {
    if (state.selectedIndex !== clampedIndex) {
      dispatch({ type: 'SET_SELECTION', index: clampedIndex })
    }
  }, [state.selectedIndex, clampedIndex])

  // Settings and the Windows mixer are reachable from fixed root-footer buttons.
  const settingsCmd =
    !currentStep && !isInputStep && atRaw === null ? commands.find(c => c.id === SETTINGS_COMMAND_ID) : undefined
  const handleOpenSettings = useCallback(() => {
    if (!settingsCmd?.createRootStep) return
    dispatch({ type: 'PUSH_STEP', step: settingsCmd.createRootStep(configRef.current) })
  }, [settingsCmd])
  const volumeMixerCmd =
    !currentStep && !isInputStep && atRaw === null ? commands.find(c => c.id === VOLUME_MIXER_COMMAND_ID) : undefined
  const handleOpenVolumeMixer = useCallback(() => {
    if (!volumeMixerCmd?.createRootStep) return
    dispatch({ type: 'PUSH_STEP', step: volumeMixerCmd.createRootStep(configRef.current) })
  }, [volumeMixerCmd])
  const handleVolumeMixerError = useCallback((error: string | null) => {
    dispatch({ type: 'SET_ERROR', error })
  }, [])
  const primaryAction = previewResult
    ? 'Copy'
    : selectedItem
      ? (selectedItem.actionLabel ??
        (selectedItem.isFolder ? 'Open Folder' : selectedItem.id.startsWith('script:') ? 'Run Script' : 'Select'))
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
    const seq = ++stepLoadSeq.current
    dispatch({ type: 'SET_LOADING', loading: true })
    step
      .load(configRef.current)
      .then(items => {
        if (stepLoadSeq.current !== seq || currentStepRef.current !== step) return
        dispatch({ type: 'SET_ITEMS', stepId: step.id, items, preserveSelection: true })
      })
      .catch(err => {
        if (stepLoadSeq.current !== seq || currentStepRef.current !== step) return
        dispatch({ type: 'SET_ERROR', error: String(err) })
      })
  }

  // Ctrl+K action panel: secondary actions for the highlighted item, keyed off
  // its provider source (see paletteActions). The refs/dispatch are stable, so
  // only the feedback callbacks need to be in the dep list.
  const buildActions = useCallback(
    (item: PaletteItem): ActionItem[] =>
      buildItemActions(item, {
        dispatch,
        configRef,
        commandsRef,
        overridesRef,
        handleSelectRef,
        reloadStepRef,
        resolveCommand,
        toast,
        showHud,
        requestConfirm,
        refreshOverrides,
      }),
    [resolveCommand, toast, showHud, requestConfirm, refreshOverrides],
  )

  const actionItems = selectedItem && !isInputStep && !isSliderStep && !isFormStep ? buildActions(selectedItem) : []
  // The menu currently shown: a drilled-into submenu, or the root action list.
  const currentActionMenu =
    actionMenuStack.length > 0 ? (actionMenuStack[actionMenuStack.length - 1].submenu ?? []) : actionItems
  const actionMenuTitle = actionMenuStack.length > 0 ? actionMenuStack[actionMenuStack.length - 1].label : undefined
  const actionPanelClampedIndex = Math.min(actionPanelIndex, Math.max(0, currentActionMenu.length - 1))

  const handleFormSubmit = useCallback(async () => {
    if (!currentStep?.isFormStep || !currentStep.onSubmit) return
    try {
      const result = await currentStep.onSubmit(formValues, configRef.current)
      await applyStepResult(dispatch, result)
    } catch (err) {
      dispatch({ type: 'SET_ERROR', error: String(err) })
    }
  }, [currentStep, formValues])

  // Live preview on highlight change (arrow keys or hover). The first
  // highlight after a step mounts/reloads is skipped — it's the default
  // selection, not the user moving.
  const highlightReady = useRef(false)
  useEffect(() => {
    highlightReady.current = false
  }, [currentStep])
  useEffect(() => {
    if (!currentStep?.onHighlight || !selectedItem) return
    if (!highlightReady.current) {
      highlightReady.current = true
      return
    }
    currentStep.onHighlight(selectedItem)
  }, [selectedItem]) // eslint-disable-line react-hooks/exhaustive-deps

  // Keyboard handler
  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent) => {
      // Confirm dialog owns the keyboard while pending (above the action panel).
      if (confirmReq) {
        e.preventDefault()
        if (e.key === 'Escape') {
          resolveConfirm(false)
          return
        }
        if (e.key === 'Enter') {
          resolveConfirm(confirmFocus === 'confirm')
          return
        }
        if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Tab') {
          setConfirmFocus(f => (f === 'confirm' ? 'cancel' : 'confirm'))
          return
        }
        // R or Space toggles "Don't ask again" (only meaningful for keyed prompts)
        if ((e.key.toLowerCase() === 'r' || e.key === ' ') && confirmReq.options.key) {
          setConfirmRemember(v => !v)
          return
        }
        return
      }

      // Action panel mode: it owns the keyboard until closed
      if (actionPanelOpen) {
        const closePanel = () => {
          setActionPanelOpen(false)
          setActionPanelIndex(0)
          setActionMenuStack([])
        }
        const popMenu = () => {
          setActionMenuStack(s => s.slice(0, -1))
          setActionPanelIndex(0)
        }
        if (e.key === 'Escape') {
          e.preventDefault()
          // Esc backs out one submenu level at a time, then closes the panel.
          if (actionMenuStack.length > 0) popMenu()
          else closePanel()
          return
        }
        if (e.key === 'ArrowLeft' && actionMenuStack.length > 0) {
          e.preventDefault()
          popMenu()
          return
        }
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setActionPanelIndex(i => Math.min(i + 1, Math.max(0, currentActionMenu.length - 1)))
          return
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setActionPanelIndex(i => Math.max(0, i - 1))
          return
        }

        // Selecting a row: drill into a submenu, or run the leaf handler.
        const activate = async (action: ActionItem) => {
          if (action.submenu) {
            setActionMenuStack(s => [...s, action])
            setActionPanelIndex(0)
            return
          }
          if (selectedItem) recordUse(selectedItem.id)
          try {
            await action.handler?.()
          } catch (err) {
            dispatch({ type: 'SET_ERROR', error: String(err) })
          }
          closePanel()
        }

        if (e.key === 'Enter' || e.key === 'ArrowRight') {
          e.preventDefault()
          const action = currentActionMenu[actionPanelClampedIndex]
          if (action) await activate(action)
          else if (e.key === 'Enter') closePanel()
          return
        }
        // Number shortcuts 1-9
        const digit = parseInt(e.key, 10)
        if (!Number.isNaN(digit) && digit >= 1 && digit <= currentActionMenu.length) {
          e.preventDefault()
          await activate(currentActionMenu[digit - 1])
          return
        }
        // Letter shortcut matching action.shortcut
        if (/^[a-z]$/i.test(e.key)) {
          const action = currentActionMenu.find(a => a.shortcut?.toLowerCase() === e.key.toLowerCase())
          if (action) {
            e.preventDefault()
            await activate(action)
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
      const isSearchInput = target instanceof HTMLInputElement && target.dataset.paletteSearch !== undefined
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
          setActionMenuStack([])
        }
        return
      }

      if (e.key === ',' && (e.ctrlKey || e.metaKey) && settingsCmd?.createRootStep) {
        e.preventDefault()
        handleOpenSettings()
        return
      }

      if (e.key.toLowerCase() === 'm' && (e.ctrlKey || e.metaKey) && volumeMixerCmd?.createRootStep) {
        e.preventDefault()
        handleOpenVolumeMixer()
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
          ? currentStep
            ? selected.isFolder === true
            : !!resolveCommand(selected.id)?.createRootStep
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
        dispatch({ type: 'SET_SELECTION', index: next })
        return
      }

      if (e.key === 'ArrowUp') {
        e.preventDefault()
        const next = Math.max(0, clampedIndex - 1)
        dispatch({ type: 'SET_SELECTION', index: next })
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
            await applyStepResult(dispatch, result)
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
    },
    // This central handler intentionally reads the live palette closure; adding
    // every callback would recreate it without changing the event semantics.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      state,
      currentStep,
      isInputStep,
      isSliderStep,
      sliderValue,
      visibleItems,
      clampedIndex,
      actionPanelOpen,
      actionItems,
      currentActionMenu,
      actionMenuStack,
      actionPanelClampedIndex,
      selectedItem,
      previewResult,
      calcMode,
      timeMode,
      confirmReq,
      confirmFocus,
    ],
  )

  const handleSelect = useCallback(
    async (item: PaletteItem) => {
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
            const url =
              data.kind === 'github'
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
        // Same-id replace = the step refreshing itself; keep the user's spot
        await applyStepResult(dispatch, result, {
          preserveSelectionOnReplace: result.type === 'replace' && result.step.id === currentStep.id,
        })
      } catch (err) {
        dispatch({ type: 'SET_ERROR', error: String(err) })
      }
      // The ref assignment below keeps the latest closure available to callers;
      // deliberately keyed on currentStep only.
    },
    // The ref below exposes the latest closure; step changes are the lifecycle
    // boundary that must recreate the selection handler.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [currentStep],
  )
  handleSelectRef.current = handleSelect

  // Focus input whenever visible (the container on slider steps, so
  // Escape/Backspace keep working without a text input). Form steps own
  // their own field focus.
  useEffect(() => {
    if (isSliderStep) containerRef.current?.focus()
    else if (!isFormStep) inputRef.current?.focus()
  })

  // On each re-focus (every palette show), drop the cached @search file list:
  // each show may target a different Explorer/Finder folder, so invalidate any
  // in-flight load and clear the cache. (Window resizing on focus is handled by
  // usePaletteWindowSize.)
  useEffect(() => {
    const unlistenPromise = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) {
        folderLoad.current = { token: folderLoad.current.token + 1, loaded: false, loading: false }
        dispatch({ type: 'SET_ITEMS', stepId: '__folder__', items: [] })
        dispatch({ type: 'SET_LOADING', loading: false })
      }
    })
    return () => {
      void unlistenPromise.then(unlisten => unlisten())
    }
  }, [])

  const placeholder = isInputStep
    ? (currentStep?.placeholder ?? 'Enter value...')
    : (currentStep?.placeholder ?? 'Search commands...')

  return (
    // Outer wrapper is unscaled and full-width: usePaletteWindowSize measures
    // its height (which already reflects the inner zoom) to size the window. The
    // inner container is a fixed base width scaled by `zoom`, so it renders at
    // paletteWidth × scale — exactly the window width the hook sets.
    <div ref={sizeRef} style={{ width: '100%' }}>
      <div
        ref={containerRef}
        data-palette-root
        tabIndex={-1}
        style={{
          outline: 'none',
          position: 'relative',
          width: paletteWidth,
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

        {hud && <HudOverlay message={hud.message} icon={hud.icon} />}

        {confirmReq && (
          <ConfirmOverlay
            options={confirmReq.options}
            remember={confirmRemember}
            focus={confirmFocus}
            onToggleRemember={() => setConfirmRemember(v => !v)}
            onResolve={resolveConfirm}
          />
        )}

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
        ) : isFormStep || isVolumeMixerStep ? null : (
          <SearchInput
            ref={inputRef}
            value={state.query}
            placeholder={placeholder}
            loading={state.loading}
            onChange={q => dispatch({ type: 'SET_QUERY', query: q })}
            preview={previewResult}
            showBack={config.ui_style?.toLowerCase() === 'onix' && state.stepStack.length > 0}
            onBack={() => dispatch({ type: 'POP_STEP' })}
          />
        )}

        {state.error && (
          <div
            style={{
              padding: '4px 12px',
              color: '#f7768e',
              fontSize: 12,
              fontFamily: 'var(--font)',
              borderBottom: '1px solid var(--border)',
            }}
          >
            {state.error}
          </div>
        )}

        {state.stepStack.length > 0 && !isVolumeMixerStep && (
          <div data-step-breadcrumb>
            <StepBreadcrumb steps={state.stepStack} />
          </div>
        )}

        {!isInputStep &&
          !isSliderStep &&
          !isVolumeMixerStep &&
          !state.loading &&
          noMatches &&
          state.query &&
          !(findMode && !findQuery.trim()) &&
          !(webMode && !webQuery) &&
          !calcMode &&
          !timeMode && (
            <div
              style={{
                padding: '12px 14px',
                display: 'flex',
                flexDirection: 'column',
                gap: 6,
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  color: 'var(--text-dim)',
                  fontSize: 12,
                  fontFamily: 'var(--font)',
                }}
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <circle cx="11" cy="11" r="8" />
                  <path d="m21 21-4.3-4.3" />
                </svg>
                {folderMode || findMode
                  ? `No files matching '${folderMode ? folderQuery : findQuery}'`
                  : `No commands matching '${state.query}'`}
              </div>
            </div>
          )}

        {!isInputStep && !isSliderStep && !isFormStep && !isVolumeMixerStep && visibleItems.length > 0 && (
          <div style={{ display: 'flex', minHeight: 0 }}>
            <div style={{ flex: 1, minWidth: 0 }}>
              {isGridStep ? (
                <ResultsGrid
                  items={displayItems}
                  selectedIndex={clampedIndex}
                  query={state.query}
                  columns={currentStep?.gridColumns}
                  onSelect={handleSelect}
                  onHover={i => dispatch({ type: 'SET_SELECTION', index: i })}
                />
              ) : (
                <ResultsList
                  items={displayItems}
                  selectedIndex={clampedIndex}
                  onSelect={handleSelect}
                  onHover={i => dispatch({ type: 'SET_SELECTION', index: i })}
                />
              )}
            </div>
            {showPreview && <DetailPane item={selectedItem} />}
          </div>
        )}

        {isVolumeMixerStep && <VolumeMixer onError={handleVolumeMixerError} />}

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
            items={currentActionMenu}
            selectedIndex={actionPanelClampedIndex}
            title={actionMenuTitle}
            onBack={() => {
              setActionMenuStack(s => s.slice(0, -1))
              setActionPanelIndex(0)
            }}
            onSelect={async item => {
              if (item.submenu) {
                setActionMenuStack(s => [...s, item])
                setActionPanelIndex(0)
                return
              }
              if (selectedItem) recordUse(selectedItem.id)
              try {
                await item.handler?.()
              } catch (err) {
                dispatch({ type: 'SET_ERROR', error: String(err) })
              }
              setActionPanelOpen(false)
              setActionPanelIndex(0)
              setActionMenuStack([])
            }}
            onHover={i => setActionPanelIndex(i)}
          />
        )}

        {claudeUsageVisible && <ClaudeUsage />}
        {codexUsageVisible && <CodexUsage />}
        {systemStatsVisible && <SystemStatsPanel />}
        <Footer
          selectedItem={selectedItem}
          primaryAction={primaryAction}
          onOpenSettings={handleOpenSettings}
          settingsVisible={!!settingsCmd}
          onOpenVolumeMixer={handleOpenVolumeMixer}
          volumeMixerVisible={!!volumeMixerCmd}
          gameModeEnabled={gameModeEnabled}
          onToggleGameMode={onToggleGameMode}
          navigationTitle={currentStep?.label}
          navigationIcon={currentStep?.icon}
        />
      </div>
    </div>
  )
}
