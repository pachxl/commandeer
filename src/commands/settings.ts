import type { AppConfig, Command, PaletteItem, Step, StepResult } from '../types'
import { dataDir, getAutostart, openPath, setAutostart, setGlobalHotkey, setPerMonitorAltTab, setScreenshotHotkey, setWindowDrag, setWindowTransparency, writeConfig } from '../lib/tauri'
import { appEvents } from '../lib/appEvents'
import { applyTheme, applyThemeByName, getAllThemes, type Theme } from '../lib/themes'

// The screenshot hotkey is a global shortcut on Windows and macOS; on Linux
// the trigger is a managed COSMIC binding, so we hide the setting there.
const IS_LINUX = typeof navigator !== 'undefined' && navigator.userAgent.includes('Linux')
const IS_MAC = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac')
const IS_WINDOWS = !IS_LINUX && !IS_MAC
const DEFAULT_SCREENSHOT_HOTKEY = IS_MAC ? '' : 'Insert'
// Mirrors the Rust defaults in shortcuts.rs (kept in sync by hand — these are
// only display fallbacks for an unset config value).
const DEFAULT_HOTKEY = IS_MAC ? 'Cmd+Shift+Space' : 'Ctrl+Space'
const DEFAULT_GAME_HOTKEY = 'Alt+Space'

// The Scale slider runs 0–100% and maps onto a 0.5×–1.5× zoom factor, so 50% =
// 1.0× (the default size). These convert between the two representations.
const scaleToPercent = (factor: number) => Math.round((factor - 0.5) * 100)
const percentToScale = (percent: number) => 0.5 + percent / 100

function settingsStep(config: AppConfig): Step {
  const transparencyPercent = Math.round((config.transparency ?? 0) * 100)
  const scalePercent = scaleToPercent(config.palette_scale ?? 1)

  return {
    id: 'settings:root',
    label: 'Settings',
    placeholder: 'Select a setting...',
    load: async (): Promise<PaletteItem[]> => [
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
      ...(IS_LINUX ? [] : [{
        id: 'settings:screenshot-hotkey',
        label: 'Screenshot Hotkey',
        sublabel: `Current: ${config.screenshot_hotkey || DEFAULT_SCREENSHOT_HOTKEY} — starts region capture`,
        icon: 'camera',
        isFolder: true,
        actionLabel: 'Change',
      } as PaletteItem]),
      ...(IS_WINDOWS ? [{
        id: 'settings:per-monitor-alt-tab',
        label: 'Per-Monitor Alt+Tab',
        sublabel: config.per_monitor_alt_tab
          ? 'On — show windows from the monitor under the cursor only'
          : 'Off — use the standard Windows switcher',
        icon: 'window',
        actionLabel: 'Toggle',
      } as PaletteItem] : []),
      // Alt-drag window management is Windows/macOS only — Wayland forbids a
      // client from moving other apps' windows (COSMIC provides it natively).
      ...(IS_LINUX ? [] : [{
        id: 'settings:window-drag',
        label: 'Alt-Drag Windows',
        sublabel: `${config.window_drag ? 'On' : 'Off'} — hold Alt to move, Alt + right-drag to resize any window`,
        icon: 'window',
        actionLabel: 'Toggle',
      } as PaletteItem]),
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
        id: 'settings:open-scripts',
        label: 'Open Scripts Folder',
        sublabel: config.scripts_dir || 'Not configured',
        icon: 'folder',
        actionLabel: 'Open Folder',
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
      if (item.id === 'settings:open-scripts') {
        if (config.scripts_dir) await openPath(config.scripts_dir)
        return { type: 'done' }
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
    onHighlight: item => applyTheme(item.data as Theme),
    onExit: () => { applyThemeByName(config.theme).catch(console.error) },
    onSelect: async (item): Promise<StepResult> => {
      const theme = item.data as Theme
      applyTheme(theme)
      const next: AppConfig = { ...config, theme: theme.name }
      await writeConfig(next)
      Object.assign(config, next)
      // Replace so the "Current" marker updates while the user previews themes
      return { type: 'replace', step: chooseThemeStep(next) }
    },
  }
}

function transparencyStep(config: AppConfig): Step {
  const currentPercent = Math.round((config.transparency ?? 0) * 100)

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

      // Apply immediately for real-time feedback
      try {
        await setWindowTransparency(transparency)
      } catch (error) {
        console.error('Failed to set window transparency:', error)
      }

      const next: AppConfig = { ...config, transparency }
      await writeConfig(next)
      Object.assign(config, next)
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
      const next: AppConfig = { ...config, palette_scale: scale }
      await writeConfig(next)
      Object.assign(config, next)
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
  const current = which === 'toggle'
    ? (config.global_hotkey || DEFAULT_HOTKEY)
    : (config.global_hotkey_game || DEFAULT_GAME_HOTKEY)
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
        appEvents.toast?.(`Invalid hotkey: ${String(err)}`, 'error')
        return { type: 'stay' }
      }
    },
    load: async () => [],
  }
}

// Free-text step to rebind the region-screenshot global hotkey. The binding is
// validated on the Rust side (set_screenshot_hotkey rejects unparseable
// strings) and re-registered immediately. Windows only — reached solely from
// the (Windows-gated) settings entry above.
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
        appEvents.toast?.(`Invalid hotkey: ${String(err)}`, 'error')
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
    keywords: ['settings', 'config', 'preferences', 'theme', 'transparency'],
    actionLabel: 'Open Settings',
    createRootStep: () => settingsStep(config),
  }
}
