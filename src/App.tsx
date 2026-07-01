import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands, scriptsToCommands, builtinCommands } from './commands'
import { readConfig, setGameMode, type ScriptInfo } from './lib/tauri'
import type { AppConfig, Command } from './types'
import Palette from './components/Palette'

// Fallback used only until the real config (with a platform-appropriate
// scripts_dir) is loaded from the backend.
const EMPTY_CONFIG: AppConfig = { scripts_dir: '' }
const GAME_MODE_KEY = 'commandeer:gamemode'
const CLAUDE_USAGE_KEY = 'commandeer:claude-usage-visible'
const SCRIPTS_CACHE_KEY = 'commandeer:scripts'

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
  const [commands, setCommands] = useState<Command[]>(() => [...scriptsToCommands(loadCachedScripts()), ...builtinCommands])
  const [gameModeEnabled, setGameModeEnabled] = useState(
    () => localStorage.getItem(GAME_MODE_KEY) === 'true'
  )
  const [claudeUsageVisible, setClaudeUsageVisible] = useState(
    () => localStorage.getItem(CLAUDE_USAGE_KEY) === 'true'
  )
  const resetRef = useRef<(() => void) | null>(null)
  const configRef = useRef<AppConfig>(EMPTY_CONFIG)

  async function refresh() {
    try {
      const { commands: cmds, scripts } = await loadScriptCommands(configRef.current)
      localStorage.setItem(SCRIPTS_CACHE_KEY, JSON.stringify(scripts))
      setCommands([...cmds, ...builtinCommands])
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
        configRef.current = cfg
        if (!disposed) setConfig(cfg)
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

  return (
    <Palette
      config={config}
      commands={commands}
      onConfigChange={() => {}}
      resetRef={resetRef}
      gameMode={gameModeEnabled}
      onToggleGameMode={toggleGameMode}
      claudeUsageVisible={claudeUsageVisible}
      onToggleClaudeUsage={toggleClaudeUsage}
    />
  )
}
