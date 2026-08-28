import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands, scriptsToCommands, webSearchCommand } from './commands'
import { settingsCommand } from './commands/settings'
import { guideCommand, guideStep } from './commands/guide'
import { loadProviderCommands } from './providers'
import { loadBookmarkCommands } from './providers/bookmarks'
import { loadQuicklinkCommands } from './providers/quicklinks'
import { loadNoteCommands } from './providers/notes'
import { killProcessCommand } from './providers/processes'
import { toolsFolderCommand, virtualFolderCommand } from './providers/tools'
import { appEvents } from './lib/appEvents'
import { applyThemeByName } from './lib/themes'
import { applyStyle } from './lib/styles'
import { markOnboardingSeen, shouldShowOnboarding } from './lib/onboarding'
import {
  onCommandDeepLink,
  onCommandHotkey,
  readConfig,
  setGameMode,
  setWindowTransparency,
  type ScriptInfo,
} from './lib/tauri'
import type { AppConfig, Command } from './types'
import Palette, { type InlineScript } from './components/Palette'

// Fallback used only until the real config (with a platform-appropriate
// scripts_dir) is loaded from the backend.
const EMPTY_CONFIG: AppConfig = { scripts_dir: '' }
const GAME_MODE_KEY = 'commandeer:gamemode'
const CLAUDE_USAGE_KEY = 'commandeer:claude-usage-visible'
const CODEX_USAGE_KEY = 'commandeer:codex-usage-visible'
const WEB_SEARCH_KEY = 'commandeer:web-search-visible'
const SYSTEM_STATS_KEY = 'commandeer:system-stats-visible'
const SCRIPTS_CACHE_KEY = 'commandeer:scripts'

// Read directly from localStorage (not React state) so refresh() always sees
// the current value without stale-closure issues. Defaults to visible.
const isWebSearchVisible = () => localStorage.getItem(WEB_SEARCH_KEY) !== 'false'

function loadCachedScripts(): ScriptInfo[] {
  try {
    const raw = localStorage.getItem(SCRIPTS_CACHE_KEY)
    return raw ? (JSON.parse(raw) as ScriptInfo[]) : []
  } catch {
    return []
  }
}

export default function App() {
  const [config, setConfig] = useState<AppConfig>(EMPTY_CONFIG)
  // Single mutable config object shared with settings steps: they update it
  // in place (Object.assign) so writes stay visible without re-creating commands.
  const configRef = useRef<AppConfig>({ ...EMPTY_CONFIG })
  const [welcomePending, setWelcomePending] = useState(() => shouldShowOnboarding(key => localStorage.getItem(key)))
  const welcomeStepRef = useRef(welcomePending ? guideStep(configRef.current, true) : undefined)
  const [commands, setCommands] = useState<Command[]>(() => [
    ...scriptsToCommands(loadCachedScripts()),
    ...(isWebSearchVisible() ? [webSearchCommand] : []),
    killProcessCommand,
    guideCommand(configRef.current),
    settingsCommand(configRef.current),
  ])
  const [gameModeEnabled, setGameModeEnabled] = useState(() => localStorage.getItem(GAME_MODE_KEY) === 'true')
  const [claudeUsageVisible, setClaudeUsageVisible] = useState(() => localStorage.getItem(CLAUDE_USAGE_KEY) === 'true')
  const [codexUsageVisible, setCodexUsageVisible] = useState(() => localStorage.getItem(CODEX_USAGE_KEY) === 'true')
  const [systemStatsVisible, setSystemStatsVisible] = useState(() => localStorage.getItem(SYSTEM_STATS_KEY) !== 'false')
  // Palette scale factor (CSS zoom). Seeded from config once it loads; 1.0 =
  // default size. Held in React state so the scale slider updates the palette live.
  const [paletteScale, setPaletteScale] = useState(1)
  // Inline scripts (@vicinae.mode inline + refreshTime) the palette polls for
  // live stdout. Computed from the loaded scripts in refresh().
  const [inlineScripts, setInlineScripts] = useState<InlineScript[]>([])
  const resetRef = useRef<(() => void) | null>(null)
  // Palette registers separate handlers for trusted per-command shortcuts and
  // untrusted external URI navigation.
  const commandHotkeyRef = useRef<((commandId: string) => void) | null>(null)
  const commandDeepLinkRef = useRef<((commandId: string) => void) | null>(null)
  const refreshGenerationRef = useRef(0)
  const refreshPromiseRef = useRef<Promise<void> | null>(null)

  function refresh(): Promise<void> {
    refreshGenerationRef.current += 1
    if (refreshPromiseRef.current) return refreshPromiseRef.current

    const run = async () => {
      try {
        while (true) {
          const generation = refreshGenerationRef.current
          const configSnapshot: AppConfig = {
            ...configRef.current,
            search_paths: configRef.current.search_paths ? [...configRef.current.search_paths] : undefined,
          }
          const webSearchVisible = isWebSearchVisible()

          try {
            const { commands: cmds, scripts } = await loadScriptCommands(configSnapshot)
            if (generation !== refreshGenerationRef.current) continue

            const providerCmds = await loadProviderCommands(configSnapshot).catch(err => {
              console.error(err)
              return [] as Command[]
            })

            // A newer request arrived while loading. Discard the whole snapshot,
            // including its cache and inline-script state, and load that request
            // next instead of briefly rendering mixed generations.
            if (generation !== refreshGenerationRef.current) continue

            localStorage.setItem(SCRIPTS_CACHE_KEY, JSON.stringify(scripts))
            const inline = scripts
              .filter(s => !s.is_folder && s.metadata?.mode === 'inline' && s.metadata.refresh_seconds != null)
              .map(s => ({ path: s.path, refreshSeconds: s.metadata!.refresh_seconds! }))
            setInlineScripts(inline)

            // Commands tagged with a folderName group under virtual folders
            // (like script folders): hidden from root browse, still in search.
            const webSearchCmds = webSearchVisible ? [webSearchCommand] : []
            const appCmds = providerCmds.filter(c => c.folderName === 'Apps')
            const systemCmds = providerCmds.filter(c => c.folderName === 'System')
            const toolsBuiltins = [...providerCmds, ...webSearchCmds].filter(c => c.folderName === 'Tools')
            // Quick Links, Notes and Bookmarks live as sub-folders inside Tools.
            // Dynamic child loaders keep them current without leaving a folder.
            const hasQuicklinks = providerCmds.some(c => c.folderName === 'Quick Links')
            const hasNotes = providerCmds.some(c => c.folderName === 'Notes')
            const hasBookmarks = providerCmds.some(c => c.folderName === 'Bookmarks')
            const toolsChildren: Command[] = [
              ...toolsBuiltins,
              ...(hasQuicklinks ? [virtualFolderCommand('Quick Links', () => loadQuicklinkCommands())] : []),
              ...(hasNotes ? [virtualFolderCommand('Notes', () => loadNoteCommands())] : []),
              ...(hasBookmarks ? [virtualFolderCommand('Bookmarks', () => loadBookmarkCommands())] : []),
            ]
            setCommands([
              ...cmds,
              ...(appCmds.length > 0 ? [virtualFolderCommand('Apps', appCmds)] : []),
              ...(systemCmds.length > 0 ? [virtualFolderCommand('System', systemCmds)] : []),
              ...(toolsChildren.length > 0 ? [toolsFolderCommand(toolsChildren)] : []),
              ...webSearchCmds,
              ...providerCmds,
              guideCommand(configRef.current),
              settingsCommand(configRef.current),
            ])
          } catch (err) {
            console.error(err)
          }

          if (generation === refreshGenerationRef.current) return
        }
      } finally {
        // Clear before this async function settles so a just-arrived request
        // cannot observe a completed promise and get dropped.
        refreshPromiseRef.current = null
      }
    }

    // Defer the drain until the in-flight ref is assigned. This also keeps a
    // future synchronous loader failure from leaving a settled promise behind.
    const running = Promise.resolve().then(run)
    refreshPromiseRef.current = running
    return running
  }

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    let unlistenHotkey: (() => void) | undefined
    let unlistenDeepLink: (() => void) | undefined
    let removeDismissListeners: (() => void) | undefined

    ;(async () => {
      try {
        const cfg = await readConfig()
        // Merge into the shared mutable object so settings steps and the
        // Palette always see the same, current config.
        Object.assign(configRef.current, cfg)
        if (!disposed) setConfig(configRef.current)
        // Apply theme first, then the UI style so structural/material variables
        // win. Onix owns its neutral glass palette but keeps the theme accent.
        applyThemeByName(cfg.theme)
          .then(() => applyStyle(cfg.ui_style))
          .catch(console.error)
        if (cfg.transparency !== undefined) {
          setWindowTransparency(cfg.transparency).catch(console.error)
        }
        if (cfg.palette_scale !== undefined) {
          setPaletteScale(cfg.palette_scale)
        }
      } catch (err) {
        console.error(err)
      }

      await refresh()
      setGameMode(gameModeEnabled).catch(console.error)

      unlistenHotkey = await onCommandHotkey(id => commandHotkeyRef.current?.(id))
      unlistenDeepLink = await onCommandDeepLink(id => commandDeepLinkRef.current?.(id))

      const win = getCurrentWindow()
      // Palette owns Escape while one of its navigation/confirmation states is
      // active. This window-level bubble listener is only a fallback for an
      // event React did not handle; blur remains the focus-loss fallback.
      const dismiss = () => {
        resetRef.current?.()
        void win.hide()
      }
      const onEscape = (event: KeyboardEvent) => {
        if (event.key !== 'Escape' || event.defaultPrevented) return
        event.preventDefault()
        dismiss()
      }
      window.addEventListener('keydown', onEscape)
      window.addEventListener('blur', dismiss)
      removeDismissListeners = () => {
        window.removeEventListener('keydown', onEscape)
        window.removeEventListener('blur', dismiss)
      }

      unlisten = await win.onFocusChanged(({ payload: focused }) => {
        if (focused) {
          refresh()
          // Re-assert the saved transparency every time the launcher is shown.
          // The window is reused across hide/show, but layered-window alpha can
          // be dropped by the OS (and a value set while hidden at startup never
          // sticks), so reapply to keep every open consistent.
          const transparency = configRef.current.transparency
          if (transparency !== undefined) {
            setWindowTransparency(transparency).catch(console.error)
          }
        } else {
          dismiss()
        }
      })
      if (disposed) {
        unlisten?.()
        unlistenHotkey?.()
        unlistenDeepLink?.()
        removeDismissListeners?.()
      }
    })()

    return () => {
      disposed = true
      unlisten?.()
      unlistenHotkey?.()
      unlistenDeepLink?.()
      removeDismissListeners?.()
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  async function toggleGameMode() {
    const next = !gameModeEnabled
    setGameModeEnabled(next)
    localStorage.setItem(GAME_MODE_KEY, String(next))
    await setGameMode(next)
  }

  function toggleClaudeUsage() {
    const next = !claudeUsageVisible
    setClaudeUsageVisible(next)
    localStorage.setItem(CLAUDE_USAGE_KEY, String(next))
  }

  function toggleCodexUsage() {
    const next = !codexUsageVisible
    setCodexUsageVisible(next)
    localStorage.setItem(CODEX_USAGE_KEY, String(next))
  }

  function toggleWebSearch() {
    localStorage.setItem(WEB_SEARCH_KEY, String(!isWebSearchVisible()))
    void refresh()
  }

  function toggleSystemStats() {
    const next = !systemStatsVisible
    setSystemStatsVisible(next)
    localStorage.setItem(SYSTEM_STATS_KEY, String(next))
  }

  // Keep the bridge fresh each render so settings commands see current state
  appEvents.toggleGameMode = () => {
    void toggleGameMode()
  }
  appEvents.toggleClaudeUsage = toggleClaudeUsage
  appEvents.toggleCodexUsage = toggleCodexUsage
  appEvents.toggleWebSearch = toggleWebSearch
  appEvents.toggleSystemStats = toggleSystemStats
  appEvents.isGameMode = () => gameModeEnabled
  appEvents.isClaudeUsageVisible = () => claudeUsageVisible
  appEvents.isCodexUsageVisible = () => codexUsageVisible
  appEvents.isWebSearchVisible = isWebSearchVisible
  appEvents.isSystemStatsVisible = () => systemStatsVisible
  appEvents.getScale = () => paletteScale
  appEvents.setScale = setPaletteScale
  appEvents.refreshCommands = () => {
    void refresh()
  }

  return (
    <Palette
      config={config}
      commands={commands}
      scale={paletteScale}
      inlineScripts={inlineScripts}
      onConfigChange={() => {}}
      resetRef={resetRef}
      commandHotkeyRef={commandHotkeyRef}
      commandDeepLinkRef={commandDeepLinkRef}
      onToggleGameMode={toggleGameMode}
      gameModeEnabled={gameModeEnabled}
      claudeUsageVisible={claudeUsageVisible}
      codexUsageVisible={codexUsageVisible}
      systemStatsVisible={systemStatsVisible}
      initialStep={welcomeStepRef.current}
      onInitialStepOpened={() => {
        markOnboardingSeen((key, value) => localStorage.setItem(key, value))
        welcomeStepRef.current = undefined
        setWelcomePending(false)
      }}
    />
  )
}
