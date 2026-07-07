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

const dracula: Theme = {
  name: 'Dracula',
  variables: {
    '--bg': 'rgba(40, 42, 54, 0.85)',
    '--bg-tab': 'rgba(68, 71, 90, 0.9)',
    '--bg-hover': 'rgba(189, 147, 249, 0.08)',
    '--bg-select': 'rgba(189, 147, 249, 0.14)',
    '--bg-select-hover': 'rgba(189, 147, 249, 0.20)',
    '--bg-elevated': 'rgba(33, 34, 44, 0.95)',
    '--border': 'rgba(98, 114, 164, 0.25)',
    '--border-strong': 'rgba(98, 114, 164, 0.45)',
    '--text': '#f8f8f2',
    '--text-dim': '#6272a4',
    '--accent': '#bd93f9',
  },
}

const oneDark: Theme = {
  name: 'One Dark',
  variables: {
    '--bg': 'rgba(40, 44, 52, 0.85)',
    '--bg-tab': 'rgba(44, 49, 58, 0.9)',
    '--bg-hover': 'rgba(97, 175, 239, 0.08)',
    '--bg-select': 'rgba(97, 175, 239, 0.14)',
    '--bg-select-hover': 'rgba(97, 175, 239, 0.20)',
    '--bg-elevated': 'rgba(33, 37, 43, 0.95)',
    '--border': 'rgba(92, 99, 112, 0.25)',
    '--border-strong': 'rgba(92, 99, 112, 0.45)',
    '--text': '#abb2bf',
    '--text-dim': '#5c6370',
    '--accent': '#61afef',
  },
}

const monokai: Theme = {
  name: 'Monokai',
  variables: {
    '--bg': 'rgba(39, 40, 34, 0.85)',
    '--bg-tab': 'rgba(62, 61, 50, 0.9)',
    '--bg-hover': 'rgba(249, 38, 114, 0.08)',
    '--bg-select': 'rgba(249, 38, 114, 0.14)',
    '--bg-select-hover': 'rgba(249, 38, 114, 0.20)',
    '--bg-elevated': 'rgba(30, 31, 28, 0.95)',
    '--border': 'rgba(117, 113, 94, 0.25)',
    '--border-strong': 'rgba(117, 113, 94, 0.45)',
    '--text': '#f8f8f2',
    '--text-dim': '#75715e',
    '--accent': '#f92672',
  },
}

const gruvboxDark: Theme = {
  name: 'Gruvbox Dark',
  variables: {
    '--bg': 'rgba(40, 40, 40, 0.85)',
    '--bg-tab': 'rgba(60, 56, 54, 0.9)',
    '--bg-hover': 'rgba(250, 189, 47, 0.08)',
    '--bg-select': 'rgba(250, 189, 47, 0.14)',
    '--bg-select-hover': 'rgba(250, 189, 47, 0.20)',
    '--bg-elevated': 'rgba(29, 32, 33, 0.95)',
    '--border': 'rgba(102, 92, 84, 0.30)',
    '--border-strong': 'rgba(102, 92, 84, 0.50)',
    '--text': '#ebdbb2',
    '--text-dim': '#928374',
    '--accent': '#fabd2f',
  },
}

const gruvboxLight: Theme = {
  name: 'Gruvbox Light',
  variables: {
    '--bg': 'rgba(251, 241, 199, 0.92)',
    '--bg-tab': 'rgba(235, 219, 178, 0.95)',
    '--bg-hover': 'rgba(181, 118, 20, 0.10)',
    '--bg-select': 'rgba(181, 118, 20, 0.20)',
    '--bg-select-hover': 'rgba(181, 118, 20, 0.28)',
    '--bg-elevated': 'rgba(249, 245, 215, 0.98)',
    '--border': 'rgba(189, 174, 147, 0.45)',
    '--border-strong': 'rgba(189, 174, 147, 0.65)',
    '--text': '#3c3836',
    '--text-dim': '#7c6f64',
    '--accent': '#b57614',
  },
}

const solarizedDark: Theme = {
  name: 'Solarized Dark',
  variables: {
    '--bg': 'rgba(0, 43, 54, 0.85)',
    '--bg-tab': 'rgba(7, 54, 66, 0.9)',
    '--bg-hover': 'rgba(38, 139, 210, 0.08)',
    '--bg-select': 'rgba(38, 139, 210, 0.14)',
    '--bg-select-hover': 'rgba(38, 139, 210, 0.20)',
    '--bg-elevated': 'rgba(0, 33, 43, 0.95)',
    '--border': 'rgba(88, 110, 117, 0.30)',
    '--border-strong': 'rgba(88, 110, 117, 0.50)',
    '--text': '#93a1a1',
    '--text-dim': '#586e75',
    '--accent': '#268bd2',
  },
}

const solarizedLight: Theme = {
  name: 'Solarized Light',
  variables: {
    '--bg': 'rgba(253, 246, 227, 0.92)',
    '--bg-tab': 'rgba(238, 232, 213, 0.95)',
    '--bg-hover': 'rgba(38, 139, 210, 0.10)',
    '--bg-select': 'rgba(38, 139, 210, 0.20)',
    '--bg-select-hover': 'rgba(38, 139, 210, 0.28)',
    '--bg-elevated': 'rgba(255, 252, 240, 0.98)',
    '--border': 'rgba(147, 161, 161, 0.40)',
    '--border-strong': 'rgba(147, 161, 161, 0.60)',
    '--text': '#586e75',
    '--text-dim': '#93a1a1',
    '--accent': '#268bd2',
  },
}

const githubDark: Theme = {
  name: 'GitHub Dark',
  variables: {
    '--bg': 'rgba(13, 17, 23, 0.85)',
    '--bg-tab': 'rgba(22, 27, 34, 0.9)',
    '--bg-hover': 'rgba(88, 166, 255, 0.08)',
    '--bg-select': 'rgba(88, 166, 255, 0.14)',
    '--bg-select-hover': 'rgba(88, 166, 255, 0.20)',
    '--bg-elevated': 'rgba(10, 13, 18, 0.95)',
    '--border': 'rgba(139, 148, 158, 0.25)',
    '--border-strong': 'rgba(139, 148, 158, 0.45)',
    '--text': '#e6edf3',
    '--text-dim': '#8b949e',
    '--accent': '#58a6ff',
  },
}

const githubLight: Theme = {
  name: 'GitHub Light',
  variables: {
    '--bg': 'rgba(255, 255, 255, 0.92)',
    '--bg-tab': 'rgba(246, 248, 250, 0.95)',
    '--bg-hover': 'rgba(9, 105, 218, 0.08)',
    '--bg-select': 'rgba(9, 105, 218, 0.16)',
    '--bg-select-hover': 'rgba(9, 105, 218, 0.24)',
    '--bg-elevated': 'rgba(255, 255, 255, 0.98)',
    '--border': 'rgba(140, 149, 159, 0.35)',
    '--border-strong': 'rgba(140, 149, 159, 0.55)',
    '--text': '#24292f',
    '--text-dim': '#57606a',
    '--accent': '#0969da',
  },
}

const rosePine: Theme = {
  name: 'Rosé Pine',
  variables: {
    '--bg': 'rgba(25, 23, 36, 0.85)',
    '--bg-tab': 'rgba(38, 35, 58, 0.9)',
    '--bg-hover': 'rgba(235, 188, 186, 0.08)',
    '--bg-select': 'rgba(235, 188, 186, 0.14)',
    '--bg-select-hover': 'rgba(235, 188, 186, 0.20)',
    '--bg-elevated': 'rgba(31, 29, 46, 0.95)',
    '--border': 'rgba(110, 106, 134, 0.25)',
    '--border-strong': 'rgba(110, 106, 134, 0.45)',
    '--text': '#e0def4',
    '--text-dim': '#6e6a86',
    '--accent': '#ebbcba',
  },
}

const everforest: Theme = {
  name: 'Everforest',
  variables: {
    '--bg': 'rgba(45, 53, 59, 0.85)',
    '--bg-tab': 'rgba(61, 72, 77, 0.9)',
    '--bg-hover': 'rgba(167, 192, 128, 0.08)',
    '--bg-select': 'rgba(167, 192, 128, 0.14)',
    '--bg-select-hover': 'rgba(167, 192, 128, 0.20)',
    '--bg-elevated': 'rgba(35, 42, 46, 0.95)',
    '--border': 'rgba(133, 146, 137, 0.25)',
    '--border-strong': 'rgba(133, 146, 137, 0.45)',
    '--text': '#d3c6aa',
    '--text-dim': '#859289',
    '--accent': '#a7c080',
  },
}

const ayuMirage: Theme = {
  name: 'Ayu Mirage',
  variables: {
    '--bg': 'rgba(31, 36, 48, 0.85)',
    '--bg-tab': 'rgba(36, 41, 54, 0.9)',
    '--bg-hover': 'rgba(255, 204, 102, 0.08)',
    '--bg-select': 'rgba(255, 204, 102, 0.14)',
    '--bg-select-hover': 'rgba(255, 204, 102, 0.20)',
    '--bg-elevated': 'rgba(26, 31, 41, 0.95)',
    '--border': 'rgba(112, 122, 140, 0.25)',
    '--border-strong': 'rgba(112, 122, 140, 0.45)',
    '--text': '#cbccc6',
    '--text-dim': '#707a8c',
    '--accent': '#ffcc66',
  },
}

const kanagawa: Theme = {
  name: 'Kanagawa',
  variables: {
    '--bg': 'rgba(31, 31, 40, 0.85)',
    '--bg-tab': 'rgba(42, 42, 55, 0.9)',
    '--bg-hover': 'rgba(126, 156, 216, 0.08)',
    '--bg-select': 'rgba(126, 156, 216, 0.14)',
    '--bg-select-hover': 'rgba(126, 156, 216, 0.20)',
    '--bg-elevated': 'rgba(22, 22, 29, 0.95)',
    '--border': 'rgba(114, 113, 105, 0.25)',
    '--border-strong': 'rgba(114, 113, 105, 0.45)',
    '--text': '#dcd7ba',
    '--text-dim': '#727169',
    '--accent': '#7e9cd8',
  },
}

const nightOwl: Theme = {
  name: 'Night Owl',
  variables: {
    '--bg': 'rgba(1, 22, 39, 0.85)',
    '--bg-tab': 'rgba(11, 41, 66, 0.9)',
    '--bg-hover': 'rgba(130, 170, 255, 0.08)',
    '--bg-select': 'rgba(130, 170, 255, 0.14)',
    '--bg-select-hover': 'rgba(130, 170, 255, 0.20)',
    '--bg-elevated': 'rgba(1, 17, 29, 0.95)',
    '--border': 'rgba(95, 126, 151, 0.30)',
    '--border-strong': 'rgba(95, 126, 151, 0.50)',
    '--text': '#d6deeb',
    '--text-dim': '#5f7e97',
    '--accent': '#82aaff',
  },
}

const synthwave: Theme = {
  name: "Synthwave '84",
  variables: {
    '--bg': 'rgba(38, 35, 53, 0.85)',
    '--bg-tab': 'rgba(52, 41, 79, 0.9)',
    '--bg-hover': 'rgba(255, 126, 219, 0.08)',
    '--bg-select': 'rgba(255, 126, 219, 0.14)',
    '--bg-select-hover': 'rgba(255, 126, 219, 0.20)',
    '--bg-elevated': 'rgba(36, 27, 47, 0.95)',
    '--border': 'rgba(132, 139, 189, 0.25)',
    '--border-strong': 'rgba(132, 139, 189, 0.45)',
    '--text': '#f0eff1',
    '--text-dim': '#848bbd',
    '--accent': '#ff7edb',
  },
}

// --- Commandeer originals ---

const crimson: Theme = {
  name: 'Crimson',
  variables: {
    '--bg': 'rgba(24, 12, 14, 0.85)',
    '--bg-tab': 'rgba(42, 20, 24, 0.9)',
    '--bg-hover': 'rgba(229, 72, 77, 0.08)',
    '--bg-select': 'rgba(229, 72, 77, 0.14)',
    '--bg-select-hover': 'rgba(229, 72, 77, 0.20)',
    '--bg-elevated': 'rgba(18, 9, 11, 0.95)',
    '--border': 'rgba(178, 72, 80, 0.25)',
    '--border-strong': 'rgba(178, 72, 80, 0.45)',
    '--text': '#f0d8da',
    '--text-dim': '#9a6b70',
    '--accent': '#e5484d',
  },
}

const matrix: Theme = {
  name: 'Matrix',
  variables: {
    '--bg': 'rgba(5, 12, 7, 0.85)',
    '--bg-tab': 'rgba(10, 26, 14, 0.9)',
    '--bg-hover': 'rgba(0, 255, 65, 0.06)',
    '--bg-select': 'rgba(0, 255, 65, 0.12)',
    '--bg-select-hover': 'rgba(0, 255, 65, 0.18)',
    '--bg-elevated': 'rgba(3, 8, 5, 0.95)',
    '--border': 'rgba(60, 160, 90, 0.25)',
    '--border-strong': 'rgba(60, 160, 90, 0.45)',
    '--text': '#b3f0c4',
    '--text-dim': '#4d8a60',
    '--accent': '#00ff41',
  },
}

const ember: Theme = {
  name: 'Ember',
  variables: {
    '--bg': 'rgba(26, 17, 12, 0.85)',
    '--bg-tab': 'rgba(43, 28, 19, 0.9)',
    '--bg-hover': 'rgba(255, 140, 66, 0.08)',
    '--bg-select': 'rgba(255, 140, 66, 0.14)',
    '--bg-select-hover': 'rgba(255, 140, 66, 0.20)',
    '--bg-elevated': 'rgba(20, 13, 9, 0.95)',
    '--border': 'rgba(197, 120, 64, 0.25)',
    '--border-strong': 'rgba(197, 120, 64, 0.45)',
    '--text': '#f3e3d3',
    '--text-dim': '#a08468',
    '--accent': '#ff8c42',
  },
}

const midnightOcean: Theme = {
  name: 'Midnight Ocean',
  variables: {
    '--bg': 'rgba(6, 16, 28, 0.85)',
    '--bg-tab': 'rgba(12, 28, 44, 0.9)',
    '--bg-hover': 'rgba(45, 212, 191, 0.08)',
    '--bg-select': 'rgba(45, 212, 191, 0.14)',
    '--bg-select-hover': 'rgba(45, 212, 191, 0.20)',
    '--bg-elevated': 'rgba(4, 12, 22, 0.95)',
    '--border': 'rgba(70, 120, 150, 0.30)',
    '--border-strong': 'rgba(70, 120, 150, 0.50)',
    '--text': '#d0e6f0',
    '--text-dim': '#5c7f96',
    '--accent': '#2dd4bf',
  },
}

const ultraviolet: Theme = {
  name: 'Ultraviolet',
  variables: {
    '--bg': 'rgba(20, 14, 34, 0.85)',
    '--bg-tab': 'rgba(34, 24, 56, 0.9)',
    '--bg-hover': 'rgba(167, 139, 250, 0.08)',
    '--bg-select': 'rgba(167, 139, 250, 0.14)',
    '--bg-select-hover': 'rgba(167, 139, 250, 0.20)',
    '--bg-elevated': 'rgba(15, 10, 26, 0.95)',
    '--border': 'rgba(140, 100, 220, 0.25)',
    '--border-strong': 'rgba(140, 100, 220, 0.45)',
    '--text': '#e2d9f7',
    '--text-dim': '#7d6f9e',
    '--accent': '#a78bfa',
  },
}

const sakura: Theme = {
  name: 'Sakura',
  variables: {
    '--bg': 'rgba(253, 242, 246, 0.92)',
    '--bg-tab': 'rgba(247, 228, 236, 0.95)',
    '--bg-hover': 'rgba(210, 86, 139, 0.08)',
    '--bg-select': 'rgba(210, 86, 139, 0.16)',
    '--bg-select-hover': 'rgba(210, 86, 139, 0.24)',
    '--bg-elevated': 'rgba(255, 250, 252, 0.98)',
    '--border': 'rgba(210, 150, 175, 0.35)',
    '--border-strong': 'rgba(210, 150, 175, 0.55)',
    '--text': '#4a2b38',
    '--text-dim': '#a3798c',
    '--accent': '#d2568b',
  },
}

const sakuraNight: Theme = {
  name: 'Sakura Night',
  variables: {
    '--bg': 'rgba(26, 16, 21, 0.85)',
    '--bg-tab': 'rgba(43, 26, 34, 0.9)',
    '--bg-hover': 'rgba(242, 119, 168, 0.08)',
    '--bg-select': 'rgba(242, 119, 168, 0.14)',
    '--bg-select-hover': 'rgba(242, 119, 168, 0.20)',
    '--bg-elevated': 'rgba(20, 12, 16, 0.95)',
    '--border': 'rgba(180, 120, 150, 0.25)',
    '--border-strong': 'rgba(180, 120, 150, 0.45)',
    '--text': '#f0dce5',
    '--text-dim': '#9a7386',
    '--accent': '#f277a8',
  },
}

// Raycast/Vicinae-style colorway: neutral near-black panels, soft hovers,
// light hairline borders, and the Raycast-red accent.
const beacon: Theme = {
  name: 'Beacon',
  variables: {
    '--bg': 'rgba(26, 26, 26, 0.85)',
    '--bg-tab': 'rgba(40, 40, 40, 0.9)',
    '--bg-hover': 'rgba(255, 99, 99, 0.08)',
    '--bg-select': 'rgba(255, 99, 99, 0.14)',
    '--bg-select-hover': 'rgba(255, 99, 99, 0.20)',
    '--bg-elevated': 'rgba(20, 20, 20, 0.95)',
    '--border': 'rgba(255, 255, 255, 0.10)',
    '--border-strong': 'rgba(255, 255, 255, 0.18)',
    '--text': '#f2f2f2',
    '--text-dim': '#7a7a7a',
    '--accent': '#ff6363',
  },
}

const BUILTIN_THEMES: Theme[] = [
  tokyoNight,
  light,
  catppuccin,
  nord,
  dracula,
  oneDark,
  monokai,
  gruvboxDark,
  gruvboxLight,
  solarizedDark,
  solarizedLight,
  githubDark,
  githubLight,
  rosePine,
  everforest,
  ayuMirage,
  kanagawa,
  nightOwl,
  synthwave,
  crimson,
  matrix,
  ember,
  midnightOcean,
  ultraviolet,
  sakura,
  sakuraNight,
  beacon,
]

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

function parseColor(value: string): [number, number, number] | null {
  const hex = value.trim().match(/^#([0-9a-f]{6})$/i)
  if (hex) {
    const n = parseInt(hex[1], 16)
    return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff]
  }
  const rgb = value.trim().match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i)
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])]
  return null
}

function isLightTheme(theme: Theme): boolean {
  const c = parseColor(theme.variables['--bg'] ?? '')
  if (!c) return false
  return (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255 > 0.6
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
  root.setAttribute('data-theme', isLightTheme(theme) ? 'light' : 'dark')
}

export async function applyThemeByName(name: string | undefined | null) {
  const wanted = (name && (LEGACY_NAMES[name.toLowerCase()] ?? name)) || 'Tokyo Night'
  const all = await getAllThemes()
  const theme = all.find(t => t.name.toLowerCase() === wanted.toLowerCase()) ?? BUILTIN_THEMES[0]
  applyTheme(theme)
}
