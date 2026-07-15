// App launcher: installed applications (shell AppsFolder — win32 + UWP/Store)
// as static commands grouped under the Apps virtual folder, so they rank
// through the same fuzzy + frecency pipeline as everything else. The list is
// cached in localStorage for instant cold starts and refreshed in the
// background at most once per TTL (refresh() runs on every palette show).
// Icons resolve lazily per visible row via iconPath → path_icon.
import type { Command, CommandProvider } from '../types'
import { appEvents } from '../lib/appEvents'
import { listApps, runningAppPaths, runApp, type AppInfo } from '../lib/tauri'

const TTL_MS = 5 * 60_000
// Running state changes far faster than the installed-app list, so it has its
// own short TTL and is refreshed in the background on every palette show.
const RUNNING_TTL_MS = 3_000
const APPS_CACHE_KEY = 'commandeer:apps'

function loadCachedApps(): AppInfo[] {
  try {
    const raw = localStorage.getItem(APPS_CACHE_KEY)
    return raw ? (JSON.parse(raw) as AppInfo[]) : []
  } catch {
    return []
  }
}

let apps: AppInfo[] = loadCachedApps()
let fetchedAt = 0
let inflight: Promise<void> | null = null

let running = new Set<string>()
let runningFetchedAt = 0
let runningInflight: Promise<void> | null = null

function refreshRunning(): Promise<void> {
  runningInflight ??= runningAppPaths()
    .then(paths => {
      runningFetchedAt = Date.now()
      const next = new Set(paths)
      // Re-render only when the running set actually changed (symmetric diff),
      // so this can't loop against refresh().
      const changed = next.size !== running.size || [...next].some(p => !running.has(p))
      running = next
      if (changed) appEvents.refreshCommands?.()
    })
    .catch(err => {
      console.error('running_app_paths failed:', err)
      runningFetchedAt = Date.now()
    })
    .finally(() => {
      runningInflight = null
    })
  return runningInflight
}

function refreshApps(): Promise<void> {
  inflight ??= listApps()
    .then(next => {
      fetchedAt = Date.now()
      const serialized = JSON.stringify(next)
      const changed = serialized !== JSON.stringify(apps)
      apps = next
      if (changed) {
        localStorage.setItem(APPS_CACHE_KEY, serialized)
        // Re-render only when the list actually changed, so this can't loop
        appEvents.refreshCommands?.()
      }
    })
    .catch(err => {
      console.error('list_apps failed:', err)
      fetchedAt = Date.now()
    })
    .finally(() => {
      inflight = null
    })
  return inflight
}

function appToCommand(app: AppInfo): Command {
  return {
    id: `app:${app.path}`,
    label: app.name,
    description: 'Application',
    icon: 'app',
    iconPath: app.path,
    source: 'app',
    folderName: 'Apps',
    actionLabel: 'Open',
    running: running.has(app.path),
    action: async () => {
      await runApp(app.path)
    },
  }
}

export const appLauncherProvider: CommandProvider = {
  id: 'apps',
  name: 'Applications',
  priority: 60,
  getCommands: async (): Promise<Command[]> => {
    if (Date.now() - fetchedAt > TTL_MS) {
      // Only the very first load (no cache yet) blocks on the fetch; after
      // that the cached list is served and refreshed in the background
      if (apps.length === 0) await refreshApps()
      else void refreshApps()
    }
    // Running state is always refreshed in the background (never blocks); the
    // dots appear on the next re-render once the set is back.
    if (Date.now() - runningFetchedAt > RUNNING_TTL_MS) void refreshRunning()
    return apps.map(appToCommand)
  },
}
