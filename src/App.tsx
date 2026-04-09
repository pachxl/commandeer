import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands } from './commands'
import type { AppConfig, Command } from './types'
import Palette from './components/Palette'

const SCRIPTS_DIR = 'C:/dev/commandeer/commands'
const EMPTY_CONFIG: AppConfig = { scripts_dir: SCRIPTS_DIR }

export default function App() {
  const [commands, setCommands] = useState<Command[]>([])
  const resetRef = useRef<(() => void) | null>(null)

  async function refresh() {
    const scriptCmds = await loadScriptCommands(EMPTY_CONFIG).catch(() => [])
    setCommands(scriptCmds)
  }

  useEffect(() => {
    refresh().catch(console.error)
    const win = getCurrentWindow()
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) refresh().then(() => resetRef.current?.()).catch(console.error)
    })
    return () => { unlisten.then(fn => fn()) }
  }, [])

  return <Palette config={EMPTY_CONFIG} commands={commands} onConfigChange={() => {}} resetRef={resetRef} />
}
