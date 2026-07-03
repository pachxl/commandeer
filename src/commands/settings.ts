import type { AppConfig, Command, PaletteItem, Step, StepResult } from '../types'
// The palette hotkeys aren't edited here — set global_hotkey /
// global_hotkey_game in <app-data>/config.json (read at startup).
import { dataDir, getAutostart, openPath, setAutostart, setWindowTransparency, writeConfig } from '../lib/tauri'
import { appEvents } from '../lib/appEvents'
import { applyTheme, applyThemeByName, getAllThemes, type Theme } from '../lib/themes'

function settingsStep(config: AppConfig): Step {
  const transparencyPercent = Math.round((config.transparency ?? 0) * 100)

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
        id: 'settings:toggle-claude-usage',
        label: 'Claude Usage Panel',
        sublabel: appEvents.isClaudeUsageVisible?.() ? 'On' : 'Off',
        icon: 'chart',
        iconColor: '#D97757',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-system-stats',
        label: 'System Stats Panel',
        sublabel: appEvents.isSystemStatsVisible?.() ? 'On — CPU, RAM, GPU' : 'Off',
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
        sublabel: 'Snippets, themes, config',
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
