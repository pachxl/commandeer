// Provider registry: each provider contributes static commands to the root
// list (getCommands) and/or per-query inline results (search). The script and
// settings sources still live in App.tsx's command assembly — only the newer
// feature families register here.
import type { AppConfig, Command, CommandProvider } from '../types'
import { appLauncherProvider } from './appLauncher'
import { bookmarksProvider } from './bookmarks'
import { calculatorProvider } from './calculator'
import { clipboardProvider } from './clipboard'
import { notesProvider } from './notes'
import { processesProvider } from './processes'
import { quicklinksProvider } from './quicklinks'
import { screenshotProvider } from './screenshot'
import { systemProvider } from './system'
import { toolsProvider } from './tools'
import { volumeProvider } from './volume'

export const providers: CommandProvider[] = [
  appLauncherProvider,
  systemProvider,
  volumeProvider,
  clipboardProvider,
  processesProvider,
  toolsProvider,
  screenshotProvider,
  calculatorProvider,
  bookmarksProvider,
  quicklinksProvider,
  notesProvider,
]

export async function loadProviderCommands(config: AppConfig): Promise<Command[]> {
  const sorted = [...providers].sort((a, b) => b.priority - a.priority)
  const results = await Promise.all(
    sorted.map(async p => {
      try {
        return p.getCommands ? await p.getCommands(config) : []
      } catch (err) {
        console.error(`provider ${p.id} getCommands failed:`, err)
        return []
      }
    }),
  )
  return results.flat()
}

export async function searchAllProviders(query: string, config: AppConfig): Promise<Command[]> {
  const sorted = [...providers].sort((a, b) => b.priority - a.priority)
  const results = await Promise.all(
    sorted.map(async p => {
      try {
        return p.search ? await p.search(query, config) : []
      } catch (err) {
        console.error(`provider ${p.id} search failed:`, err)
        return []
      }
    }),
  )
  return results.flat()
}
