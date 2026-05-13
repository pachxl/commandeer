import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands, scriptsToCommands, builtinCommands } from './commands'
import { setGameMode, type ScriptInfo } from './lib/tauri'
import type { AppConfig, Command } from './types'
import Palette from './components/Palette'

const SCRIPTS_DIR = 'C:/dev/commandeer/commands'
const EMPTY_CONFIG: AppConfig = { scripts_dir: SCRIPTS_DIR }
const GAME_MODE_KEY = 'commandeer:gamemode'
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
  const [commands, setCommands] = useState<Command[]>(() => [...scriptsToCommands(loadCachedScripts()), ...builtinCommands])
  const [gameModeEnabled, setGameModeEnabled] = useState(
    () => localStorage.getItem(GAME_MODE_KEY) === 'true'
  )
  const resetRef = useRef<(() => void) | null>(null)

  async function refresh() {
    try {
      const { commands: cmds, scripts } = await loadScriptCommands(EMPTY_CONFIG)
      localStorage.setItem(SCRIPTS_CACHE_KEY, JSON.stringify(scripts))
      setCommands([...cmds, ...builtinCommands])
    } catch (err) {
      console.error(err)
    }
  }

  useEffect(() => {
    refresh()
    setGameMode(gameModeEnabled).catch(console.error)
    const win = getCurrentWindow()
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        refresh()
      } else {
        resetRef.current?.()
      }
    })
    return () => { unlisten.then(fn => fn()) }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  async function toggleGameMode() {
    const next = !gameModeEnabled
    setGameModeEnabled(next)
    localStorage.setItem(GAME_MODE_KEY, String(next))
    await setGameMode(next)
  }

  return (
    <Palette
      config={EMPTY_CONFIG}
      commands={commands}
      onConfigChange={() => {}}
      resetRef={resetRef}
      gameMode={gameModeEnabled}
      onToggleGameMode={toggleGameMode}
    />
  )
}
