import type { AppConfig, Command, PaletteItem, Step, StepResult } from '../types'
import { settingsCommand } from './settings'

const guideItems = (welcome: boolean): PaletteItem[] => [
  {
    id: 'guide:welcome',
    label: welcome ? 'Welcome to Commandeer' : 'Commandeer at a Glance',
    sublabel: 'A keyboard-first launcher, search tool, and desktop utility belt',
    icon: 'app',
    accessories: [{ text: 'Start here' }],
    actionLabel: 'Read',
    detailMarkdown:
      '# Commandeer\n\nStart typing to find a command, app, script, note, link, or system action. Use the arrow keys to move and Enter to run. Escape always moves one level back before dismissing the palette.',
  },
  {
    id: 'guide:actions',
    label: 'Discover Every Action',
    sublabel: 'Open contextual actions for the highlighted result',
    icon: 'keyboard',
    accessories: [{ text: 'Ctrl K' }],
    actionLabel: 'Read',
    detailMarkdown:
      'Press **Ctrl+K** on any highlighted result to reveal actions such as copy, pin, rename, assign a shortcut, reveal in Finder/File Explorer, or delete when supported.',
  },
  {
    id: 'guide:find',
    label: 'Search Files Everywhere',
    sublabel: 'Use the indexed global file search',
    icon: 'search',
    accessories: [{ text: '@find' }],
    actionLabel: 'Read',
    detailMarkdown:
      'Type **`@find`** followed by a filename to search configured roots through Commandeer’s local index. Results stay on-device and support shell icons and previews.',
  },
  {
    id: 'guide:search',
    label: 'Search the Active Folder',
    sublabel: 'Search the Finder or Explorer folder you were using',
    icon: 'folder',
    accessories: [{ text: '@search' }],
    actionLabel: 'Read',
    detailMarkdown:
      'Type **`@search`** to search the folder that was active before Commandeer opened. On macOS this uses Finder when available; Linux falls back to your home folder.',
  },
  {
    id: 'guide:screenshot',
    label: 'Capture and Annotate',
    sublabel: 'Select a region, draw annotations, or pick a color',
    icon: 'camera',
    accessories: [{ text: 'Tools' }],
    actionLabel: 'Read',
    detailMarkdown:
      'Run **Take Screenshot**, drag a region, then draw marker strokes. Press Enter to finish, Ctrl+Z to undo, or hold Alt/Option to inspect and copy the raw pixel color.',
  },
  {
    id: 'guide:scripts',
    label: 'Make Commandeer Yours',
    sublabel: 'Add scripts with arguments, metadata, icons, and live output',
    icon: 'script',
    accessories: [{ text: 'Scripts' }],
    actionLabel: 'Read',
    detailMarkdown:
      'Open the Scripts folder from Settings and add a supported executable or script. Raycast/Vicinae-style metadata can provide titles, descriptions, keywords, arguments, confirmation, icons, folders, and refresh intervals.',
  },
  {
    id: 'guide:settings',
    label: 'Personalize Commandeer',
    sublabel: 'Themes, styles, scale, transparency, hotkeys, panels, and permissions',
    icon: 'settings',
    isFolder: true,
    accessories: [{ text: 'Settings' }],
    actionLabel: 'Open Settings',
  },
  {
    id: 'guide:done',
    label: welcome ? 'Start Using Commandeer' : 'Close Guide',
    sublabel: welcome ? 'You can reopen this guide any time by searching for it' : 'Return to your desktop',
    icon: 'power',
    actionLabel: welcome ? 'Get Started' : 'Close',
  },
]

export function guideStep(config: AppConfig, welcome = false): Step {
  return {
    id: welcome ? 'guide:welcome-step' : 'guide:step',
    label: welcome ? 'Welcome' : 'Commandeer Guide',
    placeholder: 'Explore what Commandeer can do...',
    load: async () => guideItems(welcome),
    onSelect: async (item): Promise<StepResult> => {
      if (item.id === 'guide:settings') {
        const step = settingsCommand(config).createRootStep?.(config)
        return step ? { type: 'push', step } : { type: 'stay' }
      }
      if (item.id === 'guide:done') return { type: 'done' }
      return { type: 'stay' }
    },
  }
}

export function guideCommand(config: AppConfig): Command {
  return {
    id: 'builtin:guide',
    label: 'Commandeer Guide',
    description: 'Shortcuts, search modes, screenshots, scripts, and personalization',
    icon: 'keyboard',
    source: 'builtin',
    keywords: ['help', 'welcome', 'getting started', 'tutorial', 'shortcuts', 'onboarding'],
    createRootStep: () => guideStep(config),
  }
}
