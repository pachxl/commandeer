import type { AppConfig, Command, PaletteItem, Step, StepResult } from '../types'
import { dataDir, openPath, setWindowTransparency, writeConfig } from '../lib/tauri'
import { appEvents } from '../lib/appEvents'
import { applyTheme, getAllThemes, type Theme } from '../lib/themes'

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
        icon: '🎨',
        actionLabel: 'Open',
      },
      {
        id: 'settings:transparency',
        label: 'Window Transparency',
        sublabel: `Current: ${transparencyPercent}% - Use slider to adjust`,
        icon: '🫥',
        actionLabel: 'Open',
      },
      {
        id: 'settings:toggle-game-mode',
        label: 'Game Mode',
        sublabel: `${appEvents.isGameMode?.() ? 'On' : 'Off'} — hotkey ${appEvents.isGameMode?.() ? 'Alt' : 'Ctrl'}+Space (Ctrl+G)`,
        icon: '🎮',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:toggle-claude-usage',
        label: 'Claude Usage Panel',
        sublabel: appEvents.isClaudeUsageVisible?.() ? 'On' : 'Off',
        icon: '📊',
        actionLabel: 'Toggle',
      },
      {
        id: 'settings:open-scripts',
        label: 'Open Scripts Folder',
        sublabel: config.scripts_dir || 'Not configured',
        icon: '📁',
        actionLabel: 'Open Folder',
      },
      {
        id: 'settings:open-data',
        label: 'Open Data Folder',
        sublabel: 'Snippets, themes, config',
        icon: '📁',
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
      if (item.id === 'settings:toggle-game-mode') {
        appEvents.toggleGameMode?.()
        return { type: 'replace', step: settingsStep(config) }
      }
      if (item.id === 'settings:toggle-claude-usage') {
        appEvents.toggleClaudeUsage?.()
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
        icon: '🎨',
        data: t,
        actionLabel: 'Apply Theme',
      }))
    },
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
    isSliderStep: true,
    minValue: 0,
    maxValue: 100,
    stepValue: 1,
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
    icon: '⚙️',
    keywords: ['settings', 'config', 'preferences', 'theme', 'transparency'],
    actionLabel: 'Open Settings',
    createRootStep: () => settingsStep(config),
  }
}
