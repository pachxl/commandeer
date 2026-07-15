// UI style presets: structural skins (spacing, fonts, radii, and for Onix a
// Raycast/Vicinae-inspired color palette) applied as CSS variables on :root.
// Themes still own the base colorway; styles layer structural overrides on top.

export interface UIStyle {
  name: string
  variables: Record<string, string>
}

const DEFAULT_VARIABLES: Record<string, string> = {
  // Typography
  '--font': "'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif",
  '--font-ui': "'JetBrains Mono', 'Fira Code', Consolas, monospace",

  // List rows
  '--row-padding': '4px 10px',
  '--row-gap': '8px',
  '--row-radius': '5px',
  '--row-inset': '0px',
  '--row-selected-bg': 'var(--accent)',
  '--row-selected-fg': '#ffffff',
  '--row-selected-sublabel-fg': 'rgba(255,255,255,0.78)',
  '--row-hover-bg': 'var(--bg-hover)',
  '--row-active-indicator-bg': '#30d158',

  // Icons / text inside rows
  '--icon-size': '18px',
  '--icon-font-size': '14px',
  '--label-font-size': '13px',
  '--sublabel-font-size': '11px',
  '--accessory-font-size': '10px',
  '--accessory-radius': '3px',
  '--accessory-padding': '1px 5px',

  // Search bar
  '--search-padding': '8px 14px',
  '--search-height': 'auto',
  '--search-font-size': '15px',
  '--search-icon-size': '16px',
  '--preview-label-font-size': '15px',
  '--preview-sublabel-font-size': '11px',

  // Footer
  '--footer-padding': '4px 10px',
  '--footer-height': '26px',
  '--footer-font-size': '11px',
  '--footer-button-radius': '4px',
  '--footer-button-padding': '2px 8px',

  // Keyboard hints
  '--kbd-font-size': '10px',
  '--kbd-padding': '1px 5px',
  '--kbd-radius': '3px',

  // Action panel
  '--action-panel-width': '240px',
  '--action-panel-padding': '8px 6px',
  '--action-row-padding': '5px 8px',
  '--action-row-radius': '5px',
  '--action-row-selected-bg': 'var(--accent)',
  '--action-row-selected-fg': '#ffffff',
  '--action-icon-size': '14px',
  '--action-font-size': '12px',
  '--action-kbd-bg': 'rgba(255,255,255,0.06)',

  // Grid
  '--grid-padding': '8px 6px',
  '--grid-gap': '6px',
  '--grid-cell-radius': '6px',
  '--grid-cell-padding': '8px 4px',
  '--grid-icon-size': '28px',
  '--grid-icon-font-size': '20px',
  '--grid-label-font-size': '11px',
  '--grid-sublabel-font-size': '9px',

  // Detail pane
  '--detail-width': '40%',
  '--detail-padding': '10px 12px',
  '--detail-radius': '4px',

  // Form
  '--form-padding': '8px 12px',
  '--form-field-radius': '6px',

  // Breadcrumb
  '--breadcrumb-font-size': '11px',

  // Results list container
  '--results-list-padding': '4px 6px',
}

const defaultStyle: UIStyle = {
  name: 'Default',
  variables: DEFAULT_VARIABLES,
}

// Raycast/Vicinae-inspired skin: darker, more opaque panels, warm gold accent,
// generous spacing, larger icons, rounded/inset list selections, and Inter/SF
// typography. Functionality is unchanged — only the visual treatment differs.
const onixStyle: UIStyle = {
  name: 'Onix',
  variables: {
    ...DEFAULT_VARIABLES,

    // Colorway: Vicinae Inkwell with the warm-gold accent
    '--bg': 'rgba(15, 16, 20, 0.96)',
    '--bg-tab': 'rgba(21, 22, 27, 0.98)',
    '--bg-hover': 'rgba(55, 57, 67, 0.45)',
    '--bg-select': '#272831',
    '--bg-select-hover': 'rgba(55, 57, 67, 0.65)',
    '--bg-elevated': 'rgba(21, 22, 27, 0.98)',
    '--border': 'rgba(55, 56, 66, 0.8)',
    '--border-strong': 'rgba(75, 76, 88, 0.9)',
    '--text': '#e7e5e4',
    '--text-dim': '#7a7a7a',
    '--accent': '#b8944e',

    // Typography
    '--font': "Inter, 'SF Pro Display', system-ui, sans-serif",
    '--font-ui': "'SF Mono', 'JetBrains Mono', Consolas, monospace",

    // List rows: 38 px effective height, 12 px horizontal padding, inset selection
    '--row-padding': '6px 12px',
    '--row-gap': '10px',
    '--row-radius': '10px',
    '--row-inset': '6px',
    '--row-selected-bg': '#272831',
    '--row-selected-fg': '#e7e5e4',
    '--row-selected-sublabel-fg': '#7a7a7a',
    '--row-hover-bg': 'rgba(55, 57, 67, 0.45)',
    '--row-active-indicator-bg': '#3A9C61',
    '--icon-size': '26px',
    '--icon-font-size': '16px',
    '--label-font-size': '14px',
    '--sublabel-font-size': '12px',
    '--accessory-font-size': '11px',
    '--accessory-radius': '6px',
    '--accessory-padding': '2px 7px',

    // Search bar: 60 px height, 16 px horizontal margins
    '--search-padding': '18px 16px',
    '--search-height': '60px',
    '--search-font-size': '17px',
    '--search-icon-size': '20px',
    '--preview-label-font-size': '17px',
    '--preview-sublabel-font-size': '12px',

    // Footer: 40 px height, 16 px horizontal padding
    '--footer-padding': '8px 16px',
    '--footer-height': '40px',
    '--footer-font-size': '12px',
    '--footer-button-radius': '6px',
    '--footer-button-padding': '4px 10px',
    '--kbd-font-size': '11px',
    '--kbd-padding': '2px 6px',
    '--kbd-radius': '4px',

    // Action panel: wider, more breathable
    '--action-panel-width': '300px',
    '--action-panel-padding': '8px 8px',
    '--action-row-padding': '7px 10px',
    '--action-row-radius': '10px',
    '--action-row-selected-bg': '#272831',
    '--action-row-selected-fg': '#e7e5e4',
    '--action-icon-size': '18px',
    '--action-font-size': '13px',
    '--action-kbd-bg': 'rgba(255,255,255,0.08)',

    // Grid
    '--grid-padding': '12px 10px',
    '--grid-gap': '10px',
    '--grid-cell-radius': '10px',
    '--grid-cell-padding': '12px 6px',
    '--grid-icon-size': '36px',
    '--grid-icon-font-size': '24px',
    '--grid-label-font-size': '12px',
    '--grid-sublabel-font-size': '10px',

    // Detail pane
    '--detail-width': '35%',
    '--detail-padding': '14px 16px',
    '--detail-radius': '8px',

    // Form
    '--form-padding': '12px 14px',
    '--form-field-radius': '8px',

    // Breadcrumb
    '--breadcrumb-font-size': '12px',

    // Results list container
    '--results-list-padding': '6px 8px',
  },
}

const BUILTIN_STYLES: UIStyle[] = [defaultStyle, onixStyle]

let appliedStyleKeys: string[] = []

export function applyStyle(name: string | undefined | null) {
  const style =
    BUILTIN_STYLES.find(
      s => s.name.toLowerCase() === (name ?? 'default').toLowerCase(),
    ) ?? defaultStyle
  const root = document.documentElement
  for (const key of appliedStyleKeys) {
    root.style.removeProperty(key)
  }
  appliedStyleKeys = []
  for (const [key, value] of Object.entries(style.variables)) {
    if (!key.startsWith('--') || typeof value !== 'string') continue
    root.style.setProperty(key, value)
    appliedStyleKeys.push(key)
  }
  root.setAttribute('data-style', style.name.toLowerCase())
}

export function getAllStyles(): UIStyle[] {
  return BUILTIN_STYLES
}

export function getStyleName(name: string | undefined | null): string {
  const style =
    BUILTIN_STYLES.find(
      s => s.name.toLowerCase() === (name ?? 'default').toLowerCase(),
    ) ?? defaultStyle
  return style.name
}
