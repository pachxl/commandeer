import { useReducer, useEffect, useRef, useState, useCallback, MutableRefObject } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { fuzzyFilter } from '../lib/fuzzy'
import { SETTINGS_COMMAND_ID } from '../commands/settings'
import type { AppConfig, Command, PaletteAction, PaletteItem, PaletteState } from '../types'
import SearchInput, { SliderInput } from './SearchInput'
import ResultsList from './ResultsList'
import ClaudeUsage from './ClaudeUsage'
import Footer from './Footer'
// ── Root items (the command list) ────────────────────────────────────────────

// Extra search terms (folder name, keywords) folded into the fuzzy-match text
function searchTextFor(cmd: Command, prefix?: string): string | undefined {
  if (!prefix && !cmd.keywords?.length) return undefined
  return [prefix, cmd.label, cmd.description, ...(cmd.keywords ?? [])].filter(Boolean).join(' ')
}

// Hierarchical root view: folders first, then root scripts
function commandsToItems(commands: Command[]): PaletteItem[] {
  return commands.map(cmd => ({
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.isFolder ? undefined : cmd.description,
    icon: cmd.icon,
    isFolder: cmd.isFolder,
    actionLabel: cmd.actionLabel,
    searchText: searchTextFor(cmd),
    data: cmd.id,
  }))
}

// Flat view for cross-folder search: all scripts with folder as sublabel + searchText
function commandsToFlatItems(commands: Command[]): PaletteItem[] {
  return commands.map(cmd => ({
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.folderName,
    icon: cmd.icon,
    actionLabel: cmd.actionLabel,
    searchText: searchTextFor(cmd, cmd.folderName),
    data: cmd.id,
  }))
}

// ── Reducer ───────────────────────────────────────────────────────────────────

function initialState(_config: AppConfig): PaletteState {
  return {
    query: '',
    stepStack: [],
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
        query: '',
        selectedIndex: 0,
        loading: false,
        error: null,
      }

    case 'POP_STEP':
      return {
        ...state,
        stepStack: state.stepStack.slice(0, -1),
        query: '',
        selectedIndex: 0,
        loading: false,
        error: null,
      }

    case 'REPLACE_STEP':
      return {
        ...state,
        stepStack: [...state.stepStack.slice(0, -1), action.step],
        query: '',
        selectedIndex: 0,
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

// On Linux the palette is a wlr-layer-shell surface (set up in the Rust backend),
// which can be resized in place; a plain setSize is enough. On Windows we also
// lock min == max so the user can't drag-resize it.
const IS_LINUX = typeof navigator !== 'undefined' && navigator.userAgent.includes('Linux')

interface PaletteProps {
  config: AppConfig
  commands: Command[]
  onConfigChange: (config: AppConfig) => void
  resetRef: MutableRefObject<(() => void) | null>
  onToggleGameMode: () => void
  claudeUsageVisible: boolean
}

export default function Palette({
  config,
  commands,
  onConfigChange: _onConfigChange,
  resetRef,
  onToggleGameMode,
  claudeUsageVisible,
}: PaletteProps) {
  const [state, dispatch] = useReducer(reducer, config, initialState)
  const [sliderValue, setSliderValue] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const configRef = useRef(config)
  const commandsRef = useRef(commands)
  configRef.current = config
  commandsRef.current = commands

  // Expose reset function to App
  useEffect(() => {
    resetRef.current = () => dispatch({ type: 'RESET' })
  }, [resetRef])

  // Reinitialise root items when commands list changes. The Settings command is
  // kept out of both lists — it's appended as the always-last row below.
  useEffect(() => {
    const lastId = localStorage.getItem(LAST_CMD_KEY)

    // Hierarchical view: folders at top, then root scripts with last-used floating up.
    // searchOnly commands are excluded here but stay in the flat search list.
    const folderCmds = commands.filter(c => c.isFolder)
    const rootScripts = commands.filter(c => !c.isFolder && !c.folderName && !c.searchOnly && c.id !== SETTINGS_COMMAND_ID)
    const sortedScripts = lastId
      ? [...rootScripts].sort((a, b) => (a.id === lastId ? -1 : b.id === lastId ? 1 : 0))
      : rootScripts
    dispatch({ type: 'SET_ITEMS', stepId: '__root__', items: commandsToItems([...folderCmds, ...sortedScripts]), preserveSelection: true })

    // Flat view: all scripts (no folder nav items) for cross-folder search
    const allScripts = commands.filter(c => !c.isFolder && c.id !== SETTINGS_COMMAND_ID)
    dispatch({ type: 'SET_ITEMS', stepId: '__root_flat__', items: commandsToFlatItems(allScripts), preserveSelection: true })
  }, [commands])

  // Current step key
  const currentStep = state.stepStack[state.stepStack.length - 1] ?? null
  const cacheKey = currentStep?.id ?? '__root__'

  // Load items when a new step is pushed or replaced. Keyed on the step object
  // (not its id) so a REPLACE_STEP with the same id still reloads.
  useEffect(() => {
    if (!currentStep?.load) return
    dispatch({ type: 'SET_LOADING', loading: true })
    currentStep.load(configRef.current)
      .then(items => dispatch({ type: 'SET_ITEMS', stepId: currentStep.id, items }))
      .catch(err => dispatch({ type: 'SET_ERROR', error: String(err) }))
  }, [currentStep]) // eslint-disable-line react-hooks/exhaustive-deps

  // Initialize slider position when a slider step is pushed. Transparency is
  // stored cubically eased ((percent/100)^3), so invert with a cube root.
  useEffect(() => {
    if (!currentStep?.isSliderStep) return
    if (currentStep.id === 'settings:transparency') {
      const transparency = configRef.current.transparency ?? 0
      setSliderValue(Math.round(Math.cbrt(transparency) * 100))
    } else {
      setSliderValue(currentStep.minValue ?? 0)
    }
  }, [currentStep])

  // Derived filtered items
  // At root with a query: search the flat list (all scripts across all folders)
  // At root without a query, or inside a step: use the current step's items
  const rawItems = currentStep
    ? (state.itemCache[cacheKey] ?? [])
    : state.query
      ? (state.itemCache['__root_flat__'] ?? [])
      : (state.itemCache['__root__'] ?? [])
  const isInputStep = currentStep?.isInputStep ?? false
  const isSliderStep = currentStep?.isSliderStep ?? false
  const matchedItems = (isInputStep || isSliderStep) ? [] : fuzzyFilter(rawItems, state.query, i =>
    i.searchText ?? (i.label + ' ' + (i.sublabel ?? ''))
  )
  const noMatches = matchedItems.length === 0

  // The Settings row is always the last item at root, query or not
  const settingsCmd = !currentStep && !isInputStep
    ? commands.find(c => c.id === SETTINGS_COMMAND_ID)
    : undefined
  const visibleItems = settingsCmd
    ? [...matchedItems.slice(0, 50), ...commandsToItems([settingsCmd])]
    : matchedItems.slice(0, 50)
  const clampedIndex = Math.min(state.selectedIndex, Math.max(0, visibleItems.length - 1))
  const selectedItem = visibleItems[clampedIndex] ?? null
  const primaryAction = selectedItem
    ? (selectedItem.actionLabel
      ?? (selectedItem.isFolder
        ? 'Open Folder'
        : selectedItem.id.startsWith('script:') ? 'Run Script' : 'Select'))
    : null

  // Keyboard handler
  const handleKeyDown = useCallback(async (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      dispatch({ type: 'RESET' })
      await getCurrentWindow().hide()
      return
    }

    if (e.key.toLowerCase() === 'g' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault()
      onToggleGameMode()
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

      const selected = visibleItems[clampedIndex]
      if (!selected) return
      await handleSelect(selected)
      return
    }
  }, [state, currentStep, isInputStep, visibleItems, clampedIndex]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSelect = useCallback(async (item: PaletteItem) => {
    // Root level: find command and either run action or push step
    if (!currentStep) {
      const cmd = commandsRef.current.find(c => c.id === item.id)
      if (!cmd) return
      if (cmd.action) {
        try {
          await cmd.action(configRef.current)
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
        dispatch({ type: 'REPLACE_STEP', step: result.step })
      } else if (result.type === 'pop') {
        dispatch({ type: 'POP_STEP' })
      }
    } catch (err) {
      dispatch({ type: 'SET_ERROR', error: String(err) })
    }
  }, [currentStep])

  // Focus input whenever visible (the container on slider steps, so
  // Escape/Backspace keep working without a text input)
  useEffect(() => {
    if (isSliderStep) containerRef.current?.focus()
    else inputRef.current?.focus()
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
  // Linux/Wayland (cosmic-comp): a mapped window can't be resized at all, so we
  // don't try — the window is a fixed, tall, border/shadow-less transparent
  // surface and only this content panel is opaque, so it *looks* content-sized
  // with no OS resize (and therefore no flicker). Nothing to do here.
  const containerRef = useRef<HTMLDivElement>(null)
  const lastHeightRef = useRef(0)
  const applySize = useCallback(async () => {
    const el = containerRef.current
    if (!el) return
    const h = Math.ceil(el.getBoundingClientRect().height)
    if (!h || Math.abs(h - lastHeightRef.current) < 2) return
    lastHeightRef.current = h
    if (IS_LINUX) {
      // Layer-shell surface: the compositor keeps it centered (no anchors) and
      // resizes it in place (no flicker). Its size comes from the GTK size
      // request, so go through the backend rather than setSize.
      await invoke('resize_palette', { height: h })
      return
    }
    await getCurrentWindow().setSize(new LogicalSize(669, h))
  }, [])

  useEffect(() => {
    const el = containerRef.current
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
        }
      })
      .then(fn => { unlisten = fn })
    return () => { observer.disconnect(); unlisten?.() }
  }, [applySize])

  const placeholder = isInputStep
    ? (currentStep?.placeholder ?? 'Enter value...')
    : (currentStep?.placeholder ?? 'Search commands...')

  return (
    <div
      ref={containerRef}
      tabIndex={-1}
      style={{
        outline: 'none',
        width: '100%',
        background: 'var(--bg)',
        backdropFilter: 'blur(60px) saturate(180%)',
        WebkitBackdropFilter: 'blur(60px) saturate(180%)',
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'var(--font)',
        overflow: 'hidden',
        color: 'var(--text)',
      }}
      onKeyDown={handleKeyDown}
    >
      {isSliderStep && currentStep ? (
        <SliderInput
          value={sliderValue}
          min={currentStep.minValue ?? 0}
          max={currentStep.maxValue ?? 100}
          step={currentStep.stepValue ?? 1}
          onChange={value => {
            setSliderValue(value)
            currentStep.onSliderChange?.(value, configRef.current).catch(err => {
              dispatch({ type: 'SET_ERROR', error: String(err) })
            })
          }}
        />
      ) : (
        <SearchInput
          ref={inputRef}
          value={state.query}
          placeholder={placeholder}
          loading={state.loading}
          onChange={q => dispatch({ type: 'SET_QUERY', query: q })}
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

      {!isInputStep && !isSliderStep && !state.loading && noMatches && state.query && (
        <div style={{
          padding: '8px 12px',
          color: 'var(--text-dim)',
          fontSize: 12,
          fontFamily: 'var(--font)',
        }}>
          No commands matching '{state.query}'
        </div>
      )}

      {!isInputStep && !isSliderStep && visibleItems.length > 0 && (
        <ResultsList
          items={visibleItems}
          selectedIndex={clampedIndex}
          onSelect={handleSelect}
          onHover={i => dispatch({ type: 'MOVE_SELECTION', delta: i - clampedIndex })}
        />
      )}

      {claudeUsageVisible && <ClaudeUsage />}
      <Footer selectedItem={selectedItem} primaryAction={primaryAction} />
    </div>
  )
}
