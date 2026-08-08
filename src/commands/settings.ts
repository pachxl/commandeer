import type { AppConfig, Command, PaletteItem, Step, StepResult } from '../types'
import {
  dataDir,
  getAutostart,
  getPermissionStatus,
  openPath,
  openPermissionSettings,
  setAutostart,
  setGlobalHotkey,
  setPerMonitorAltTab,
  setScreenshotHotkey,
  setWindowDrag,
  setWindowTransparency,
  startScreenshot,
  writeConfig,
} from '../lib/tauri'
import { appEvents } from '../lib/appEvents'
import { normalizeAbsolutePath, parseSearchRoots, type PathPlatform } from '../lib/configPaths'
import { applyTheme, applyThemeByName, getAllThemes, type Theme } from '../lib/themes'
import { applyStyle, getAllStyles, getStyleName, type UIStyle } from '../lib/styles'

// The screenshot hotkey is a global shortcut on Windows and macOS; on Linux
// the trigger is a managed COSMIC binding, so we hide the setting there.
const IS_LINUX = typeof navigator !== 'undefined' && navigator.userAgent.includes('Linux')
const IS_MAC = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac')
const IS_WINDOWS = !IS_LINUX && !IS_MAC
const PATH_PLATFORM: PathPlatform = IS_WINDOWS ? 'windows' : 'unix'
const DEFAULT_SCREENSHOT_HOTKEY = IS_MAC ? '' : 'Insert'
// Mirrors the Rust defaults in shortcuts.rs (kept in sync by hand — these are
// only display fallbacks for an unset config value).
const DEFAULT_HOTKEY = IS_MAC ? 'Cmd+Shift+Space' : 'Ctrl+Space'
const DEFAULT_GAME_HOTKEY = 'Alt+Space'

// The Scale slider runs 0–100% and maps onto a 0.5×–1.5× zoom factor, so 50% =
// 1.0× (the default size). These convert between the two representations.
const scaleToPercent = (factor: number) => Math.round((factor - 0.5) * 100)
const percentToScale = (percent: number) => 0.5 + percent / 100
const CONFIG_WRITE_DEBOUNCE_MS = 300

// Slider movement is intentionally live, but persistence must be trailing and
// serialized: dozens of independent whole-file writes can otherwise finish out
// of order and leave an older value on disk.
function createConfigPersister(config: AppConfig) {
  let timer: ReturnType<typeof setTimeout> | undefined
  let pending: AppConfig | null = null
  let inFlight = Promise.resolve()

  const writePending = () => {
    if (!pending) return inFlight
    const snapshot = pending
    pending = null
    inFlight = inFlight
      .then(() => writeConfig(snapshot))
      .catch(error => {
        console.error('Failed to persist slider setting:', error)
        appEvents.toast?.('Failed to save setting', 'error')
      })
    return inFlight
  }

  return {
    schedule(next: AppConfig) {
      // Keep the shared config current immediately so any concurrently-created
      // settings step starts from the visible value, not the last disk write.
      Object.assign(config, next)
      pending = { ...config }
      if (timer !== undefined) clearTimeout(timer)
      timer = setTimeout(() => {
        timer = undefined
        void writePending()
      }, CONFIG_WRITE_DEBOUNCE_MS)
    },
    flush() {
      if (timer !== undefined) {
        clearTimeout(timer)
        timer = undefined
      }
      return writePending()
    },
  }
}

const configPersisters = new WeakMap<AppConfig, ReturnType<typeof createConfigPersister>>()

function configPersister(config: AppConfig) {
  const existing = configPersisters.get(config)
  if (existing) return existing
  const created = createConfigPersister(config)
  configPersisters.set(config, created)
  return created
}

function createLatestApplier<T>(apply: (value: T) => Promise<void>) {
  let pending: T | undefined
  let running: Promise<void> | null = null

  const drain = async () => {
    while (pending !== undefined) {
      const value = pending
      pending = undefined
      try {
        await apply(value)
      } catch (error) {
        console.error('Failed to apply live slider setting:', error)
      }
    }
  }

  const start = (): Promise<void> => {
    if (!running) {
      running = drain().finally(() => {
        running = null
        if (pending !== undefined) void start()
      })
    }
    return running
  }

  return (value: T) => {
    pending = value
    return start()
  }
}

function settingsStep(config: AppConfig): Step {
  const transparencyPercent = Math.round((config.transparency ?? 0) * 100)
  const scalePercent = scaleToPercent(config.palette_scale ?? 1)

  return {
    id: 'settings:root',
    label: 'Settings',
    placeholder: 'Select a setting...',
    load: async (): Promise<PaletteItem[]> => [
      {
        id: 'settings:choose-style',
        label: 'Choose Style…',
        sublabel: `Current: ${getStyleName(config.ui_style)}`,
        icon: 'layout',
        isFolder: true,
        actionLabel: 'Open',
      },
      {
        id: 'settings:choose-theme',
        label: 'Choose Theme…',
        sublabel: `Current: ${config.theme ?? 'Tokyo Night'}`,
        icon: 'moon',
        isFolder: true,
        actionLabel: 'Open',
      },
      {
        id: 'settings:transparency',
        label: 'Window Transparency',
        sublabel: `Current: ${transparencyPercent}% - Use slider to adjust`,
        icon: 'eye',
        isFolder: true,
        actionLabel: 'Open',
      },
      {
        id: 'settings:scale',
        label: 'Scale',
        sublabel: `Current: ${scalePercent}% - Use slider to adjust`,
        icon: 'scale',
        isFolder: true,
        actionLabel: 'Open',
      },
      {
        id: 'settings:autostart',
        label: 'Start at Login',
        sublabel: (await getAutostart().catch(() => false)) ? 'On' : 'Off',
        icon: 'power',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:auto-update',
        label: 'Automatic Updates',
        sublabel:
          (config.auto_update ?? true) ? 'On — installs new releases in the background' : 'Off — never updates itself',
        icon: 'refresh',
        actionLabel: 'Toggle',
      },
      ...(IS_MAC
        ? [
            {
              id: 'settings:permissions',
              label: 'Permissions & Diagnostics…',
              sublabel: 'Screen Recording, Accessibility, Automation, and feature tests',
              icon: 'lock',
              isFolder: true,
              actionLabel: 'Open',
            } as PaletteItem,
          ]
        : []),
      {
        id: 'settings:toggle-game-mode',
        label: 'Game Mode',
        sublabel: `${appEvents.isGameMode?.() ? 'On' : 'Off'} — uses the game hotkey (Ctrl+G)`,
        icon: 'gamepad',
        iconColor: '#39ff14',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-hotkey',
        label: 'Toggle Hotkey',
        sublabel: `Current: ${config.global_hotkey || DEFAULT_HOTKEY} — opens the palette`,
        icon: 'keyboard',
        isFolder: true,
        actionLabel: 'Change',
      },
      {
        id: 'settings:game-hotkey',
        label: 'Game Mode Hotkey',
        sublabel: `Current: ${config.global_hotkey_game || DEFAULT_GAME_HOTKEY} — used when Game Mode is on`,
        icon: 'gamepad',
        isFolder: true,
        actionLabel: 'Change',
      },
      ...(IS_LINUX
        ? []
        : [
            {
              id: 'settings:screenshot-hotkey',
              label: 'Screenshot Hotkey',
              sublabel: `Current: ${config.screenshot_hotkey || DEFAULT_SCREENSHOT_HOTKEY} — starts region capture`,
              icon: 'camera',
              isFolder: true,
              actionLabel: 'Change',
            } as PaletteItem,
          ]),
      ...(IS_WINDOWS
        ? [
            {
              id: 'settings:per-monitor-alt-tab',
              label: 'Per-Monitor Alt+Tab',
              sublabel: config.per_monitor_alt_tab
                ? 'On — local windows + top maximized windows from other displays; Ctrl+Alt+1/2 focuses displays'
                : 'Off — use the standard Windows switcher',
              icon: 'window',
              actionLabel: 'Toggle',
            } as PaletteItem,
          ]
        : []),
      // Alt-drag window management is Windows/macOS only — Wayland forbids a
      // client from moving other apps' windows (COSMIC provides it natively).
      ...(IS_LINUX
        ? []
        : [
            {
              id: 'settings:window-drag',
              label: 'Alt-Drag Windows',
              sublabel: `${config.window_drag ? 'On' : 'Off'} — hold Alt to move, Alt + right-drag to resize any window`,
              icon: 'window',
              actionLabel: 'Toggle',
            } as PaletteItem,
          ]),
      {
        id: 'settings:toggle-claude-usage',
        label: 'Claude Usage Panel',
        sublabel: appEvents.isClaudeUsageVisible?.() ? 'On' : 'Off',
        icon: 'chart',
        iconColor: '#D97757',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-codex-usage',
        label: 'Codex Usage Panel',
        sublabel: appEvents.isCodexUsageVisible?.() ? 'On' : 'Off',
        icon: 'chart',
        iconColor: '#10A37F',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-system-stats',
        label: 'System Stats Panel',
        sublabel: appEvents.isSystemStatsVisible?.() ? `On — CPU, RAM${IS_MAC ? '' : ', GPU'}` : 'Off',
        icon: 'cpu',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-web-search',
        label: 'Web Search Command',
        sublabel: appEvents.isWebSearchVisible?.() ? 'On' : 'Off',
        icon: 'search',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:scripts-directory',
        label: 'Scripts Directory…',
        sublabel: config.scripts_dir || 'Not configured',
        icon: 'folder',
        isFolder: true,
        actionLabel: 'Change',
      },
      {
        id: 'settings:open-scripts',
        label: 'Open Scripts Folder',
        sublabel: config.scripts_dir || 'Not configured',
        icon: 'folder',
        actionLabel: 'Open Folder',
      },
      {
        id: 'settings:search-roots',
        label: 'File Search Roots…',
        sublabel: config.search_paths?.length
          ? `${config.search_paths.length} custom ${config.search_paths.length === 1 ? 'root' : 'roots'} — restart after changing`
          : 'Platform defaults — Desktop, Documents, Downloads',
        icon: 'search',
        isFolder: true,
        actionLabel: 'Configure',
      },
      {
        id: 'settings:open-data',
        label: 'Open Data Folder',
        sublabel: 'Notes, themes, config',
        icon: 'folder',
        actionLabel: 'Open Folder',
      },
    ],
    onSelect: async (item): Promise<StepResult> => {
      if (item.id === 'settings:choose-style') {
        return { type: 'push', step: chooseStyleStep(config) }
      }
      if (item.id === 'settings:choose-theme') {
        return { type: 'push', step: chooseThemeStep(config) }
      }
      if (item.id === 'settings:transparency') {
        return { type: 'push', step: transparencyStep(config) }
      }
      if (item.id === 'settings:scale') {
        return { type: 'push', step: scaleStep(config) }
      }
      if (item.id === 'settings:screenshot-hotkey') {
        return { type: 'push', step: screenshotHotkeyStep(config) }
      }
      if (item.id === 'settings:toggle-hotkey') {
        return { type: 'push', step: hotkeyStep(config, 'toggle') }
      }
      if (item.id === 'settings:game-hotkey') {
        return { type: 'push', step: hotkeyStep(config, 'game') }
      }
      if (item.id === 'settings:permissions') {
        return { type: 'push', step: permissionsStep() }
      }
      if (item.id === 'settings:window-drag') {
        const next = !(config.window_drag ?? false)
        try {
          // Start/stop the OS hook first; only persist if it actually took
          // (e.g. macOS throws here until Accessibility is granted).
          await setWindowDrag(next)
          Object.assign(config, { window_drag: next })
          await writeConfig(config)
          appEvents.toast?.(next ? 'Alt-drag windows enabled' : 'Alt-drag windows disabled', 'success')
        } catch (err) {
          appEvents.toast?.(`Couldn't toggle Alt-drag: ${String(err)}`, 'error')
        }
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:autostart') {
        const current = await getAutostart().catch(() => false)
        await setAutostart(!current)
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:auto-update') {
        const next = !(config.auto_update ?? true)
        try {
          await writeConfig({ ...config, auto_update: next })
          Object.assign(config, { auto_update: next })
          appEvents.toast?.(next ? 'Automatic updates enabled' : 'Automatic updates disabled', 'success')
        } catch (err) {
          appEvents.toast?.(`Couldn't save setting: ${String(err)}`, 'error')
        }
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-game-mode') {
        appEvents.toggleGameMode?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-claude-usage') {
        appEvents.toggleClaudeUsage?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:per-monitor-alt-tab') {
        const next = !(config.per_monitor_alt_tab ?? false)
        try {
          await setPerMonitorAltTab(next)
          Object.assign(config, { per_monitor_alt_tab: next })
          await writeConfig(config)
          appEvents.toast?.(next ? 'Per-monitor Alt+Tab enabled' : 'Windows Alt+Tab restored', 'success')
        } catch (err) {
          appEvents.toast?.(`Couldn't toggle per-monitor Alt+Tab: ${String(err)}`, 'error')
        }
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-codex-usage') {
        appEvents.toggleCodexUsage?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-system-stats') {
        appEvents.toggleSystemStats?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-web-search') {
        appEvents.toggleWebSearch?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:scripts-directory') {
        return { type: 'push', step: scriptsDirectoryForm(config) }
      }
      if (item.id === 'settings:open-scripts') {
        if (config.scripts_dir) await openPath(config.scripts_dir)
        return { type: 'done' }
      }
      if (item.id === 'settings:search-roots') {
        return { type: 'push', step: searchRootsStep(config) }
      }
      if (item.id === 'settings:open-data') {
        const dir = await dataDir()
        await openPath(dir)
        return { type: 'done' }
      }
      return { type: 'done' }
    },
  }
}

function scriptsDirectoryForm(config: AppConfig): Step {
  return {
    id: 'settings:scripts-directory',
    label: 'Scripts Directory',
    placeholder: 'Set the directory Commandeer scans for scripts',
    isFormStep: true,
    fields: [
      {
        id: 'scripts_dir',
        label: 'Absolute directory path',
        type: 'text',
        defaultValue: config.scripts_dir,
        placeholder: IS_WINDOWS ? 'C:\\Users\\you\\commandeer\\scripts' : '/Users/you/commandeer/scripts',
        description: 'Use a full path rather than ~. Script commands reload immediately after saving.',
      },
    ],
    submitLabel: 'Save Directory',
    onSubmit: async values => {
      const path = normalizeAbsolutePath(String(values.scripts_dir ?? ''), PATH_PLATFORM)
      if (!path) {
        appEvents.toast?.('Enter a full absolute directory path (do not use ~)', 'error')
        return { type: 'stay' }
      }

      try {
        const next: AppConfig = { ...config, scripts_dir: path }
        await writeConfig(next)
        Object.assign(config, next)
        appEvents.refreshCommands?.()
        appEvents.toast?.('Scripts directory saved and commands reloaded', 'success')
        return { type: 'done' }
      } catch (error) {
        appEvents.toast?.(`Couldn't save scripts directory: ${String(error)}`, 'error')
        return { type: 'stay' }
      }
    },
    onSelect: async () => ({ type: 'stay' }),
  }
}

function searchRootsStep(config: AppConfig): Step {
  return {
    id: 'settings:search-roots',
    label: 'File Search Roots',
    placeholder: 'Edit, reset, or open a configured root...',
    load: async (): Promise<PaletteItem[]> => {
      const configured = config.search_paths ?? []
      return [
        {
          id: 'search-roots:edit',
          label: configured.length ? 'Edit Search Roots…' : 'Set Custom Search Roots…',
          sublabel: 'One absolute directory per line; restart required after saving',
          icon: 'settings',
          isFolder: true,
          actionLabel: 'Edit',
        },
        ...(configured.length
          ? [
              {
                id: 'search-roots:reset',
                label: 'Reset to Platform Defaults',
                sublabel: 'Use Desktop, Documents, and Downloads after restart',
                icon: 'refresh',
                actionLabel: 'Reset',
              } as PaletteItem,
              ...configured.map((path, index) => ({
                id: `search-roots:open:${index}`,
                label: path,
                sublabel: 'Open in the system file manager',
                icon: 'folder',
                data: path,
                actionLabel: 'Open Folder',
              })),
            ]
          : [
              {
                id: 'search-roots:defaults',
                label: 'Using Platform Defaults',
                sublabel: 'Desktop, Documents, and Downloads',
                icon: 'search',
                actionLabel: 'Current',
              } as PaletteItem,
            ]),
      ]
    },
    onSelect: async (item): Promise<StepResult> => {
      if (item.id === 'search-roots:edit') {
        return { type: 'push', step: searchRootsForm(config) }
      }
      if (item.id === 'search-roots:reset') {
        try {
          const next: AppConfig = { ...config }
          delete next.search_paths
          await writeConfig(next)
          delete config.search_paths
          appEvents.toast?.('Search roots reset — restart Commandeer to rebuild the index', 'success')
          return { type: 'replace', step: searchRootsStep(config) }
        } catch (error) {
          appEvents.toast?.(`Couldn't reset search roots: ${String(error)}`, 'error')
          return { type: 'stay' }
        }
      }
      if (item.id.startsWith('search-roots:open:') && typeof item.data === 'string') {
        await openPath(item.data)
        return { type: 'done' }
      }
      return { type: 'stay' }
    },
  }
}

function searchRootsForm(config: AppConfig): Step {
  return {
    id: 'settings:search-roots:edit',
    label: 'Edit File Search Roots',
    placeholder: 'Enter one absolute directory per line',
    isFormStep: true,
    fields: [
      {
        id: 'search_paths',
        label: 'Absolute directory paths',
        type: 'textarea',
        defaultValue: config.search_paths?.join('\n') ?? '',
        placeholder: IS_WINDOWS ? 'C:\\Users\\you\\Desktop\nD:\\Projects' : '/Users/you/Desktop\n/Users/you/Projects',
        description:
          'Blank lines and duplicates are ignored. Restart Commandeer after saving so the background index can watch the new roots. Ctrl/Cmd+Enter saves.',
      },
    ],
    submitLabel: 'Save Search Roots',
    onSubmit: async values => {
      const parsed = parseSearchRoots(String(values.search_paths ?? ''), PATH_PLATFORM)
      if (parsed.invalid.length) {
        appEvents.toast?.(`Every root must be an absolute path. Invalid: ${parsed.invalid[0]}`, 'error')
        return { type: 'stay' }
      }
      if (!parsed.paths.length) {
        appEvents.toast?.('Enter at least one root, or use Reset to Platform Defaults', 'error')
        return { type: 'stay' }
      }

      try {
        const next: AppConfig = { ...config, search_paths: parsed.paths }
        await writeConfig(next)
        Object.assign(config, next)
        appEvents.toast?.('Search roots saved — restart Commandeer to rebuild the index', 'success')
        return { type: 'done' }
      } catch (error) {
        appEvents.toast?.(`Couldn't save search roots: ${String(error)}`, 'error')
        return { type: 'stay' }
      }
    },
    onSelect: async () => ({ type: 'stay' }),
  }
}

function permissionItem(id: string, label: string, granted: boolean | null, featureDescription: string): PaletteItem {
  const known = granted !== null
  return {
    id,
    label,
    sublabel: known
      ? granted
        ? `Granted — ${featureDescription}`
        : `Not granted — ${featureDescription}`
      : `Status unavailable — ${featureDescription}`,
    icon: granted ? 'lock' : 'settings',
    iconColor: granted ? '#34c759' : '#ff9f0a',
    actionLabel: granted ? 'Recheck' : 'Open Settings',
    accessories: [{ text: granted ? 'Granted' : known ? 'Required' : 'Unknown' }],
  }
}

function permissionsStep(): Step {
  return {
    id: 'settings:permissions',
    label: 'Permissions & Diagnostics',
    placeholder: 'Check a permission or run a test...',
    load: async (): Promise<PaletteItem[]> => {
      const status = await getPermissionStatus()
      return [
        permissionItem(
          'permissions:screen-recording',
          'Screen Recording',
          status.screen_recording,
          'needed for screenshots',
        ),
        permissionItem(
          'permissions:accessibility',
          'Accessibility',
          status.accessibility,
          'needed for paste-to-previous and Alt-drag',
        ),
        {
          id: 'permissions:automation',
          label: 'Automation',
          sublabel: 'Requested by macOS when Finder or System Events is first used',
          icon: 'settings',
          actionLabel: 'Open Settings',
          accessories: [{ text: 'On demand' }],
          detailMarkdown:
            'Automation permission is requested separately for Finder and System Events. Use Finder-aware `@search`, Empty Trash, or a power/session action to exercise the exact integration.',
        },
        {
          id: 'permissions:test-screenshot',
          label: 'Test Screenshot Capture',
          sublabel: 'Start a real region capture; press Escape to cancel',
          icon: 'camera',
          actionLabel: 'Run Test',
        },
        {
          id: 'permissions:verify-alt-drag',
          label: 'Verify Alt-Drag',
          sublabel: 'Enable it in Settings, then Option-drag another window',
          icon: 'window',
          actionLabel: 'Show Instructions',
          detailMarkdown:
            'With **Alt-Drag Windows** enabled, hold Option and left-drag another window to move it. Option + right-drag resizes from the cursor region. Verify raising, Retina coordinates, and each attached display.',
        },
        {
          id: 'permissions:refresh',
          label: 'Refresh Permission Status',
          sublabel: 'Re-read the current macOS grants',
          icon: 'refresh',
          actionLabel: 'Refresh',
        },
      ]
    },
    onSelect: async (item): Promise<StepResult> => {
      if (item.id === 'permissions:test-screenshot') {
        try {
          await startScreenshot()
          return { type: 'done' }
        } catch (error) {
          appEvents.toast?.(`Screenshot test failed: ${String(error)}`, 'error')
          return { type: 'stay' }
        }
      }
      if (item.id === 'permissions:verify-alt-drag') return { type: 'stay' }
      if (item.id === 'permissions:refresh') return { type: 'replace', step: permissionsStep() }

      const permission = item.id.replace('permissions:', '')
      if (permission === 'screen-recording' || permission === 'accessibility' || permission === 'automation') {
        if (item.accessories?.some(accessory => accessory.text === 'Granted')) {
          return { type: 'replace', step: permissionsStep() }
        }
        await openPermissionSettings(permission)
        appEvents.toast?.('Opened macOS Privacy & Security', 'success')
        return { type: 'replace', step: permissionsStep() }
      }
      return { type: 'stay' }
    },
  }
}

function chooseStyleStep(config: AppConfig): Step {
  return {
    id: 'settings:choose-style',
    label: 'Choose Style',
    placeholder: 'Select a style...',
    load: async (): Promise<PaletteItem[]> => {
      const current = getStyleName(config.ui_style).toLowerCase()
      return getAllStyles().map(s => ({
        id: `style:${s.name}`,
        label: s.name,
        sublabel: s.name.toLowerCase() === current ? 'Current' : undefined,
        icon: 'layout',
        data: s,
        actionLabel: 'Apply Style',
      }))
    },
    // Live preview while browsing; onExit re-applies the saved style.
    onHighlight: item => applyStyle((item.data as UIStyle).name),
    onExit: () => {
      applyStyle(config.ui_style)
    },
    onSelect: async (item): Promise<StepResult> => {
      const style = item.data as UIStyle
      applyStyle(style.name)
      const next: AppConfig = { ...config, ui_style: style.name }
      await writeConfig(next)
      Object.assign(config, next)
      return { type: 'replace', step: chooseStyleStep(next) }
    },
  }
}

function chooseThemeStep(config: AppConfig): Step {
  return {
    id: 'settings:choose-theme',
    label: 'Choose Theme',
    placeholder: 'Select a theme...',
    load: async (): Promise<PaletteItem[]> => {
      const current = config.theme ?? 'Tokyo Night'
      return (await getAllThemes()).map(t => ({
        id: `theme:${t.name}`,
        label: t.name,
        sublabel: t.name.toLowerCase() === current.toLowerCase() ? 'Current' : undefined,
        icon: 'moon',
        data: t,
        actionLabel: 'Apply Theme',
      }))
    },
    // Live preview while browsing; onExit re-applies the saved theme, which
    // undoes an uncommitted preview (and is a visual no-op after a commit,
    // since config.theme is updated before the step is replaced)
    onHighlight: item => {
      applyTheme(item.data as Theme)
      // Re-assert style-owned structural/material overrides. The previewed
      // theme still supplies inherited colors and Onix's accent.
      applyStyle(config.ui_style)
    },
    onExit: () => {
      applyThemeByName(config.theme)
        .then(() => applyStyle(config.ui_style))
        .catch(console.error)
    },
    onSelect: async (item): Promise<StepResult> => {
      const theme = item.data as Theme
      applyTheme(theme)
      const next: AppConfig = { ...config, theme: theme.name }
      await writeConfig(next)
      Object.assign(config, next)
      // Re-assert style-owned structural/material variables after the commit.
      applyStyle(next.ui_style)
      // Replace so the "Current" marker updates while the user previews themes
      return { type: 'replace', step: chooseThemeStep(next) }
    },
  }
}

function transparencyStep(config: AppConfig): Step {
  const currentPercent = Math.round((config.transparency ?? 0) * 100)
  const persister = configPersister(config)
  const applyTransparency = createLatestApplier(setWindowTransparency)

  return {
    id: 'settings:transparency',
    label: 'Window Transparency',
    placeholder: `Adjust window transparency (Current: ${currentPercent}%)`,
    icon: 'eye',
    isSliderStep: true,
    minValue: 0,
    maxValue: 100,
    stepValue: 1,
    // Transparency is stored cubically eased ((percent/100)^3), so the
    // slider position is recovered with a cube root
    loadSliderValue: async () => Math.round(Math.cbrt(config.transparency ?? 0) * 100),
    onSliderChange: async (value: number): Promise<void> => {
      const percent = Math.round(value)
      // Cubic easing: transparency = (percent/100)^3 for a slower start,
      // making the slider less sensitive at lower values
      const transparency = Math.pow(percent / 100, 3)

      // Record the event-order value synchronously, then coalesce backend
      // invokes so an older transparency call cannot finish after the latest.
      persister.schedule({ ...config, transparency })
      await applyTransparency(transparency)
    },
    onExit: () => {
      void persister.flush()
    },
    load: async () => [],
    onSelect: async () => ({ type: 'pop' }),
  }
}

// Slider (0–100%) that scales the whole palette via a CSS zoom. 50% = 1.0× (the
// default); dragging left shrinks (down to 0.5×), right grows (up to 1.5×). The
// scale is applied live through appEvents so the palette resizes as you drag,
// and persisted to config.palette_scale.
function scaleStep(config: AppConfig): Step {
  const currentPercent = scaleToPercent(config.palette_scale ?? 1)
  const persister = configPersister(config)

  return {
    id: 'settings:scale',
    label: 'Scale',
    placeholder: `Adjust palette scale (Current: ${currentPercent}%)`,
    icon: 'scale',
    isSliderStep: true,
    minValue: 0,
    maxValue: 100,
    stepValue: 1,
    loadSliderValue: async () => scaleToPercent(config.palette_scale ?? 1),
    onSliderChange: async (value: number): Promise<void> => {
      const scale = percentToScale(Math.round(value))
      // Apply immediately for real-time feedback (drives the App-level zoom).
      appEvents.setScale?.(scale)
      persister.schedule({ ...config, palette_scale: scale })
    },
    onExit: () => {
      void persister.flush()
    },
    load: async () => [],
    onSelect: async () => ({ type: 'pop' }),
  }
}

// Free-text step to rebind the palette toggle / game-mode hotkey. The binding
// is validated on the Rust side (set_global_hotkey rejects unparseable strings)
// and re-registered immediately, including the COSMIC/GNOME managed binding on
// Linux. An empty commit leaves the binding unchanged.
function hotkeyStep(config: AppConfig, which: 'toggle' | 'game'): Step {
  const current =
    which === 'toggle' ? config.global_hotkey || DEFAULT_HOTKEY : config.global_hotkey_game || DEFAULT_GAME_HOTKEY
  const label = which === 'toggle' ? 'Toggle Hotkey' : 'Game Mode Hotkey'
  return {
    id: `settings:${which}-hotkey`,
    label,
    placeholder: `Type a hotkey (e.g. ${DEFAULT_HOTKEY}, Alt+Shift+P). Current: ${current}`,
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const binding = query.trim()
      if (!binding) return { type: 'pop' }
      try {
        // set_global_hotkey re-registers the base hotkey and (on Linux) rewrites
        // the COSMIC/GNOME managed binding. We pass the *other* hotkey through
        // unchanged so editing one doesn't clobber the other.
        const gameMode = appEvents.isGameMode?.() ?? false
        if (which === 'toggle') {
          const game = config.global_hotkey_game ?? DEFAULT_GAME_HOTKEY
          await setGlobalHotkey(binding, game, gameMode)
          Object.assign(config, { global_hotkey: binding })
        } else {
          const base = config.global_hotkey ?? DEFAULT_HOTKEY
          await setGlobalHotkey(base, binding, gameMode)
          Object.assign(config, { global_hotkey_game: binding })
        }
        appEvents.toast?.(`${label} set to ${binding}`, 'success')
        return { type: 'pop' }
      } catch (err) {
        appEvents.toast?.(`Couldn't set hotkey: ${String(err)}`, 'error')
        return { type: 'stay' }
      }
    },
    load: async () => [],
  }
}

// Free-text step to rebind the region-screenshot global hotkey. The binding is
// validated on the Rust side (set_screenshot_hotkey rejects unparseable
// strings) and re-registered immediately. Windows/macOS only — reached solely
// from the platform-gated settings entry above.
function screenshotHotkeyStep(config: AppConfig): Step {
  const current = config.screenshot_hotkey || DEFAULT_SCREENSHOT_HOTKEY
  return {
    id: 'settings:screenshot-hotkey',
    label: 'Screenshot Hotkey',
    placeholder: `Type a hotkey (e.g. Insert, Ctrl+Shift+S). Current: ${current}`,
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const binding = query.trim()
      // Empty commit: leave the binding unchanged.
      if (!binding) return { type: 'pop' }
      try {
        await setScreenshotHotkey(binding)
        Object.assign(config, { screenshot_hotkey: binding })
        appEvents.toast?.(`Screenshot hotkey set to ${binding}`, 'success')
        return { type: 'pop' }
      } catch (err) {
        appEvents.toast?.(`Couldn't set hotkey: ${String(err)}`, 'error')
        // Stay on the step so the user can correct it.
        return { type: 'stay' }
      }
    },
    load: async () => [],
  }
}

export const SETTINGS_COMMAND_ID = 'builtin:settings'

export function settingsCommand(config: AppConfig): Command {
  return {
    id: SETTINGS_COMMAND_ID,
    label: 'Settings',
    description: 'Configure Commandeer',
    icon: 'settings',
    keywords: ['settings', 'config', 'preferences', 'theme', 'transparency', 'scripts', 'directory', 'search roots'],
    actionLabel: 'Open Settings',
    createRootStep: () => settingsStep(config),
  }
}
