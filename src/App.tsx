import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands, scriptsToCommands, webSearchCommand } from './commands'
import { searchFolderCommand } from './commands/fileSearch'
import { loadSnippetCommands } from './commands/snippets'
import { settingsCommand } from './commands/settings'
import { appEvents } from './lib/appEvents'
import { applyThemeByName } from './lib/themes'
import { explorerLocation, readConfig, setGameMode, setWindowTransparency, type ScriptInfo } from './lib/tauri'
import type { AppConfig, Command } from './types'
import Palette from './components/Palette'

// Fallback used only until the real config (with a platform-appropriate
// scripts_dir) is loaded from the backend.
const EMPTY_CONFIG: AppConfig = { scripts_dir: '' }
const GAME_MODE_KEY = 'commandeer:gamemode'
const CLAUDE_USAGE_KEY = 'commandeer:claude-usage-visible'
const WEB_SEARCH_KEY = 'commandeer:web-search-visible'
const SCRIPTS_CACHE_KEY = 'commandeer:scripts'

// Read directly from localStorage (not React state) so refresh() always sees
// the current value without stale-closure issues. Defaults to visible.
const isWebSearchVisible = () => localStorage.getItem(WEB_SEARCH_KEY) !== 'false'

function loadCachedScripts(): ScriptInfo[] {
  try {
    const raw = localStorage.getItem(SCRIPTS_CACHE_KEY)
    return raw ? JSON.parse(raw) as ScriptInfo[] : []
  } catch {
    return []
  }
}

export default function App() {
  const [config, setConfig] = useState<AppConfig>(EMPTY_CONFIG)
  // Single mutable config object shared with settings steps: they update it
  // in place (Object.assign) so writes stay visible without re-creating commands.
  const configRef = useRef<AppConfig>({ ...EMPTY_CONFIG })
  const [commands, setCommands] = useState<Command[]>(
    () => [
      ...scriptsToCommands(loadCachedScripts()),
      ...(isWebSearchVisible() ? [webSearchCommand] : []),
      settingsCommand(configRef.current),
    ]
  )
  const [gameModeEnabled, setGameModeEnabled] = useState(
    () => localStorage.getItem(GAME_MODE_KEY) === 'true'
  )
  const [claudeUsageVisible, setClaudeUsageVisible] = useState(
    () => localStorage.getItem(CLAUDE_USAGE_KEY) === 'true'
  )
  const resetRef = useRef<(() => void) | null>(null)

  async function refresh() {
    try {
      const { commands: cmds, scripts } = await loadScriptCommands(configRef.current)
      localStorage.setItem(SCRIPTS_CACHE_KEY, JSON.stringify(scripts))
      const snippetCmds = await loadSnippetCommands().catch(err => {
        console.error(err)
        return [] as Command[]
      })
      // Search Folder only makes sense when a File Explorer folder was
      // focused when the palette opened (refresh runs on every focus gain)
      const explorerFolder = await explorerLocation().catch(() => null)
      setCommands([
        ...cmds,
        ...snippetCmds,
        ...(explorerFolder ? [searchFolderCommand] : []),
        ...(isWebSearchVisible() ? [webSearchCommand] : []),
        settingsCommand(configRef.current),
      ])
    } catch (err) {
      console.error(err)
    }
  }

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined

    ;(async () => {
      try {
        const cfg = await readConfig()
        // Merge into the shared mutable object so settings steps and the
        // Palette always see the same, current config.
        Object.assign(configRef.current, cfg)
        if (!disposed) setConfig(configRef.current)
        applyThemeByName(cfg.theme).catch(console.error)
        if (cfg.transparency !== undefined) {
          setWindowTransparency(cfg.transparency).catch(console.error)
        }
      } catch (err) {
        console.error(err)
      }

      await refresh()
      setGameMode(gameModeEnabled).catch(console.error)

      const win = getCurrentWindow()
      unlisten = await win.onFocusChanged(({ payload: focused }) => {
        if (focused) {
          refresh()
        } else {
          resetRef.current?.()
        }
      })
      if (disposed) unlisten?.()
    })()

    return () => { disposed = true; unlisten?.() }
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

  function toggleWebSearch() {
    localStorage.setItem(WEB_SEARCH_KEY, String(!isWebSearchVisible()))
    void refresh()
  }

  // Keep the bridge fresh each render so settings commands see current state
  appEvents.toggleGameMode = () => { void toggleGameMode() }
  appEvents.toggleClaudeUsage = toggleClaudeUsage
  appEvents.toggleWebSearch = toggleWebSearch
  appEvents.isGameMode = () => gameModeEnabled
  appEvents.isClaudeUsageVisible = () => claudeUsageVisible
  appEvents.isWebSearchVisible = isWebSearchVisible
  appEvents.refreshCommands = () => { void refresh() }

  return (
    <Palette
      config={config}
      commands={commands}
      onConfigChange={() => {}}
      resetRef={resetRef}
      onToggleGameMode={toggleGameMode}
      claudeUsageVisible={claudeUsageVisible}
    />
  )
}
