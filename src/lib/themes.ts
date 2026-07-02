// Theme handling: built-in palettes plus user themes from
// <app-data>/themes/*.json, applied as CSS variables on :root (inline
// styles override the stylesheet defaults in index.css).
import { readThemes, type Theme } from './tauri'

export type { Theme }

const tokyoNight: Theme = {
  name: 'Tokyo Night',
  variables: {
    '--bg': 'rgba(26, 27, 38, 0.85)',
    '--bg-tab': 'rgba(36, 40, 59, 0.9)',
    '--bg-hover': 'rgba(122, 162, 247, 0.08)',
    '--bg-select': 'rgba(122, 162, 247, 0.14)',
    '--bg-select-hover': 'rgba(122, 162, 247, 0.20)',
    '--bg-elevated': 'rgba(30, 32, 48, 0.95)',
    '--border': 'rgba(86, 95, 137, 0.25)',
    '--border-strong': 'rgba(86, 95, 137, 0.45)',
    '--text': '#c0caf5',
    '--text-dim': '#565f89',
    '--accent': '#7aa2f7',
  },
}

const light: Theme = {
  name: 'Light',
  variables: {
    '--bg': 'rgba(245, 246, 250, 0.92)',
    '--bg-tab': 'rgba(235, 238, 245, 0.95)',
    '--bg-hover': 'rgba(122, 162, 247, 0.10)',
    '--bg-select': 'rgba(122, 162, 247, 0.22)',
    '--bg-select-hover': 'rgba(122, 162, 247, 0.30)',
    '--bg-elevated': 'rgba(255, 255, 255, 0.98)',
    '--border': 'rgba(150, 160, 190, 0.35)',
    '--border-strong': 'rgba(150, 160, 190, 0.55)',
    '--text': '#2e3440',
    '--text-dim': '#6c7a96',
    '--accent': '#5e81ac',
  },
}

const catppuccin: Theme = {
  name: 'Catppuccin Mocha',
  variables: {
    '--bg': 'rgba(30, 30, 46, 0.85)',
    '--bg-tab': 'rgba(49, 50, 68, 0.9)',
    '--bg-hover': 'rgba(137, 180, 250, 0.08)',
    '--bg-select': 'rgba(137, 180, 250, 0.14)',
    '--bg-select-hover': 'rgba(137, 180, 250, 0.20)',
    '--bg-elevated': 'rgba(24, 24, 37, 0.95)',
    '--border': 'rgba(108, 112, 134, 0.25)',
    '--border-strong': 'rgba(108, 112, 134, 0.45)',
    '--text': '#cdd6f4',
    '--text-dim': '#6c7086',
    '--accent': '#89b4fa',
  },
}

const nord: Theme = {
  name: 'Nord',
  variables: {
    '--bg': 'rgba(46, 52, 64, 0.85)',
    '--bg-tab': 'rgba(67, 76, 94, 0.9)',
    '--bg-hover': 'rgba(136, 192, 208, 0.08)',
    '--bg-select': 'rgba(136, 192, 208, 0.14)',
    '--bg-select-hover': 'rgba(136, 192, 208, 0.20)',
    '--bg-elevated': 'rgba(59, 66, 82, 0.95)',
    '--border': 'rgba(76, 86, 106, 0.35)',
    '--border-strong': 'rgba(76, 86, 106, 0.55)',
    '--text': '#d8dee9',
    '--text-dim': '#7b88a1',
    '--accent': '#88c0d0',
  },
}

export const BUILTIN_THEMES: Theme[] = [tokyoNight, light, catppuccin, nord]

// Legacy config values from the old dark/light toggle
const LEGACY_NAMES: Record<string, string> = {
  dark: 'Tokyo Night',
  light: 'Light',
}

export async function getAllThemes(): Promise<Theme[]> {
  let user: Theme[] = []
  try {
    user = await readThemes()
  } catch (err) {
    console.error(err)
  }
  const names = new Set(BUILTIN_THEMES.map(t => t.name.toLowerCase()))
  return [...BUILTIN_THEMES, ...user.filter(t => t.name && !names.has(t.name.toLowerCase()))]
}

let appliedKeys: string[] = []

export function applyTheme(theme: Theme) {
  const root = document.documentElement
  for (const key of appliedKeys) root.style.removeProperty(key)
  appliedKeys = []
  for (const [key, value] of Object.entries(theme.variables)) {
    if (!key.startsWith('--') || typeof value !== 'string') continue
    root.style.setProperty(key, value)
    appliedKeys.push(key)
  }
  // Keep the attribute for anything still keying off index.css light rules
  root.setAttribute('data-theme', theme.name.toLowerCase() === 'light' ? 'light' : 'dark')
}

export async function applyThemeByName(name: string | undefined | null) {
  const wanted = (name && (LEGACY_NAMES[name.toLowerCase()] ?? name)) || 'Tokyo Night'
  const all = await getAllThemes()
  const theme = all.find(t => t.name.toLowerCase() === wanted.toLowerCase()) ?? BUILTIN_THEMES[0]
  applyTheme(theme)
}
