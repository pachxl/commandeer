import { useReducer, useEffect, useRef, useCallback, MutableRefObject } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { fuzzyFilter } from '../lib/fuzzy'
import type { AppConfig, Command, PaletteAction, PaletteItem, PaletteState } from '../types'
import SearchInput from './SearchInput'
import ResultsList from './ResultsList'
// ── Root items (the command list) ────────────────────────────────────────────

function commandsToItems(commands: Command[]): PaletteItem[] {
  return commands.map(cmd => ({
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.description,
    icon: cmd.icon,
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
        selectedIndex: 0,
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
      return initialState({ scripts_dir: '' })

    default:
      return state
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

const LAST_CMD_KEY = 'commandeer:last'

interface PaletteProps {
  config: AppConfig
  commands: Command[]
  onConfigChange: (config: AppConfig) => void
  resetRef: MutableRefObject<(() => void) | null>
}

export default function Palette({ config, commands, onConfigChange: _onConfigChange, resetRef }: PaletteProps) {
  const [state, dispatch] = useReducer(reducer, config, initialState)
  const inputRef = useRef<HTMLInputElement>(null)
  const configRef = useRef(config)
  const commandsRef = useRef(commands)
  configRef.current = config
  commandsRef.current = commands

  // Expose reset function to App
  useEffect(() => {
    resetRef.current = () => dispatch({ type: 'RESET' })
  }, [resetRef])

  // Reinitialise root items when commands list changes, last-used floats to top
  useEffect(() => {
    const lastId = localStorage.getItem(LAST_CMD_KEY)
    const sorted = lastId
      ? [...commands].sort((a, b) => (a.id === lastId ? -1 : b.id === lastId ? 1 : 0))
      : commands
    dispatch({ type: 'SET_ITEMS', stepId: '__root__', items: commandsToItems(sorted) })
  }, [commands])

  // Current step key
  const currentStep = state.stepStack[state.stepStack.length - 1] ?? null
  const cacheKey = currentStep?.id ?? '__root__'

  // Load items when a new step is pushed
  useEffect(() => {
    if (!currentStep?.load) return
    dispatch({ type: 'SET_LOADING', loading: true })
    currentStep.load(configRef.current)
      .then(items => dispatch({ type: 'SET_ITEMS', stepId: currentStep.id, items }))
      .catch(err => dispatch({ type: 'SET_ERROR', error: String(err) }))
  }, [currentStep?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  // Derived filtered items
  const rawItems = state.itemCache[cacheKey] ?? []
  const isInputStep = currentStep?.isInputStep ?? false
  const visibleItems = isInputStep ? [] : fuzzyFilter(rawItems, state.query, i => i.label + ' ' + (i.sublabel ?? ''))
  const clampedIndex = Math.min(state.selectedIndex, Math.max(0, visibleItems.length - 1))

  // Keyboard handler
  const handleKeyDown = useCallback(async (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      if (state.stepStack.length > 0) {
        dispatch({ type: 'POP_STEP' })
      } else if (state.query) {
        dispatch({ type: 'SET_QUERY', query: '' })
      } else {
        await getCurrentWindow().hide()
      }
      return
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      dispatch({ type: 'MOVE_SELECTION', delta: 1 })
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      dispatch({ type: 'MOVE_SELECTION', delta: -1 })
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
      }
    } catch (err) {
      dispatch({ type: 'SET_ERROR', error: String(err) })
    }
  }, [currentStep])

  // Focus input whenever visible
  useEffect(() => {
    inputRef.current?.focus()
  })

  // Auto-resize window to match content height
  const containerRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const win = getCurrentWindow()
    const observer = new ResizeObserver(entries => {
      const h = entries[0]?.contentRect.height
      if (h) win.setSize(new LogicalSize(726, Math.ceil(h)))
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const placeholder = isInputStep
    ? (currentStep?.placeholder ?? 'Enter value...')
    : (currentStep?.placeholder ?? 'Search commands...')

  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        background: 'var(--bg)',
        backdropFilter: 'blur(60px) saturate(160%)',
        WebkitBackdropFilter: 'blur(60px) saturate(160%)',
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'var(--font)',
        overflow: 'hidden',
        border: '1px solid var(--border)',
      }}
      onKeyDown={handleKeyDown}
    >
      <SearchInput
        ref={inputRef}
        value={state.query}
        placeholder={placeholder}
        loading={state.loading}
        onChange={q => dispatch({ type: 'SET_QUERY', query: q })}
      />

      {state.error && (
        <div style={{
          padding: '4px 12px',
          color: '#f48771',
          fontSize: 12,
          fontFamily: 'var(--font)',
          borderBottom: '1px solid var(--border)',
        }}>
          {state.error}
        </div>
      )}

      {!isInputStep && visibleItems.length > 0 && (
        <ResultsList
          items={visibleItems.slice(0, 50)}
          selectedIndex={clampedIndex}
          onSelect={handleSelect}
          onHover={i => dispatch({ type: 'MOVE_SELECTION', delta: i - clampedIndex })}
        />
      )}

      {!isInputStep && !state.loading && visibleItems.length === 0 && state.query && (
        <div style={{
          padding: '10px 12px',
          color: 'var(--text-dim)',
          fontSize: 13,
          fontFamily: 'var(--font)',
        }}>
          No commands matching '{state.query}'
        </div>
      )}
    </div>
  )
}
