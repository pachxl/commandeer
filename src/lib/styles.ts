// UI style presets: structural skins (spacing, fonts, radii, and component
// treatment) applied as CSS variables on :root. Themes exclusively own color;
// styles reference theme variables instead of replacing them.

export interface UIStyle {
  name: string
  variables: Record<string, string>
}

const DEFAULT_VARIABLES: Record<string, string> = {
  // Typography
  '--font': "'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif",
  '--font-ui': "'JetBrains Mono', 'Fira Code', Consolas, monospace",

  // Window surface
  '--palette-border': 'transparent',
  '--palette-shadow': 'none',
  '--divider': 'var(--border)',
  '--surface-muted': 'transparent',
  '--detail-bg': 'transparent',
  '--form-field-bg': 'var(--bg-elevated)',

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
  '--row-selected-shadow': 'none',
  '--row-transition': 'none',
  '--sublabel-margin-left': 'auto',
  '--sublabel-font': 'var(--font-ui)',
  '--sublabel-flex-shrink': '0',

  // Icons / text inside rows
  '--icon-size': '18px',
  '--icon-font-size': '14px',
  '--row-svg-size': '16px',
  '--label-font-size': '13px',
  '--sublabel-font-size': '11px',
  '--accessory-font-size': '10px',
  '--accessory-radius': '3px',
  '--accessory-padding': '1px 5px',
  '--accessory-bg': 'rgba(255,255,255,0.06)',
  '--accessory-border': 'transparent',
  '--accessory-border-width': '0px',
  '--accessory-font': 'var(--font-ui)',

  // Search bar
  '--search-padding': '8px 14px',
  '--search-height': 'auto',
  '--search-font-size': '15px',
  '--search-icon-size': '16px',
  '--search-gap': '10px',
  '--search-back-bg': 'transparent',
  '--preview-label-font-size': '15px',
  '--preview-sublabel-font-size': '11px',

  // Footer
  '--footer-padding': '4px 10px',
  '--footer-height': '26px',
  '--footer-font-size': '11px',
  '--footer-button-radius': '4px',
  '--footer-button-padding': '2px 8px',
  '--footer-bg': 'transparent',
  '--footer-hover-bg': 'rgba(255,255,255,0.06)',
  '--footer-primary-fg': 'var(--text-dim)',
  '--footer-font': 'var(--font-ui)',
  '--footer-primary-left-display': 'inline-flex',
  '--footer-primary-right-display': 'none',
  '--footer-selected-icon-display': 'flex',
  '--footer-nav-display': 'none',

  // Keyboard hints
  '--kbd-font-size': '10px',
  '--kbd-padding': '1px 5px',
  '--kbd-radius': '3px',
  '--kbd-bg': 'rgba(255,255,255,0.06)',
  '--kbd-border': 'var(--border)',
  '--kbd-fg': 'var(--text-dim)',
  '--kbd-shadow': 'none',

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
  '--action-kbd-selected-bg': 'rgba(255,255,255,0.18)',
  '--action-panel-radius': '0px',
  '--action-panel-shadow': 'none',

  // Grid
  '--grid-padding': '8px 6px',
  '--grid-gap': '6px',
  '--grid-cell-radius': '6px',
  '--grid-cell-padding': '8px 4px',
  '--grid-icon-size': '28px',
  '--grid-icon-font-size': '20px',
  '--grid-label-font-size': '11px',
  '--grid-sublabel-font-size': '9px',
  '--grid-cell-bg': 'transparent',
  '--grid-cell-selected-bg': 'var(--accent)',
  '--grid-cell-border': 'transparent',
  '--grid-cell-selected-border': 'var(--accent)',
  '--grid-cell-selected-fg': '#ffffff',

  // Detail pane
  '--detail-width': '40%',
  '--detail-padding': '10px 12px',
  '--detail-radius': '4px',

  // Form
  '--form-padding': '8px 12px',
  '--form-field-radius': '6px',

  // Breadcrumb
  '--breadcrumb-font-size': '11px',
  '--breadcrumb-padding': '2px 14px 4px',
  '--breadcrumb-border': 'none',

  // Results list container
  '--results-list-padding': '4px 6px',
}

const defaultStyle: UIStyle = {
  name: 'Default',
  variables: DEFAULT_VARIABLES,
}

// Raycast/Vicinae-inspired skin: generous spacing, larger icons, rounded/inset
// list selections, and Inter/SF typography. Its surfaces and states derive from
// the selected theme, so changing styles never changes the user's colorway.
const onixStyle: UIStyle = {
  name: 'Onix',
  variables: {
    ...DEFAULT_VARIABLES,

    // Layered window treatment: theme surface, one-pixel rim, and inset
    // highlights that remain visible even though the native window clips any
    // exterior CSS shadow.
    '--palette-border': 'var(--border-strong)',
    '--palette-shadow': 'inset 0 1px 0 rgba(255,255,255,0.035), inset 0 -1px 0 rgba(0,0,0,0.30)',
    '--divider': 'var(--border)',
    '--surface-muted': 'var(--bg-tab)',
    '--detail-bg': 'var(--surface-muted)',
    '--form-field-bg': 'var(--surface-muted)',

    // Typography
    '--font': "Inter, 'SF Pro Text', 'Segoe UI Variable', system-ui, sans-serif",
    '--font-ui': "'SF Mono', 'JetBrains Mono', Consolas, monospace",

    // List rows: 38 px effective height, 12 px horizontal padding, inset selection
    '--row-padding': '6px 12px',
    '--row-gap': '10px',
    '--row-radius': '10px',
    '--row-inset': '6px',
    '--row-selected-bg': 'var(--bg-select)',
    '--row-selected-fg': 'var(--text)',
    '--row-selected-sublabel-fg': 'var(--text-dim)',
    '--row-hover-bg': 'var(--bg-hover)',
    '--row-active-indicator-bg': '#3A9C61',
    '--row-selected-shadow': 'inset 0 0 0 1px rgba(255,255,255,0.025), 0 1px 2px rgba(0,0,0,0.16)',
    '--row-transition': 'background-color 100ms ease, box-shadow 100ms ease',
    '--sublabel-margin-left': '0px',
    '--sublabel-font': 'var(--font)',
    '--sublabel-flex-shrink': '1',
    '--icon-size': '26px',
    '--icon-font-size': '16px',
    '--row-svg-size': '20px',
    '--label-font-size': '14px',
    '--sublabel-font-size': '12px',
    '--accessory-font-size': '11px',
    '--accessory-radius': '6px',
    '--accessory-padding': '2px 7px',
    '--accessory-bg': 'var(--bg-hover)',
    '--accessory-border': 'var(--border)',
    '--accessory-border-width': '1px',
    '--accessory-font': 'var(--font)',

    // Search bar: 60 px height, 16 px horizontal margins
    '--search-padding': '17px 16px',
    '--search-height': '60px',
    '--search-font-size': '17px',
    '--search-icon-size': '20px',
    '--search-gap': '12px',
    '--search-back-bg': 'transparent',
    '--preview-label-font-size': '17px',
    '--preview-sublabel-font-size': '12px',

    // Footer: 40 px height, 16 px horizontal padding
    '--footer-padding': '8px 16px',
    '--footer-height': '40px',
    '--footer-font-size': '12px',
    '--footer-button-radius': '6px',
    '--footer-button-padding': '4px 10px',
    '--footer-bg': 'var(--bg-tab)',
    '--footer-hover-bg': 'var(--bg-hover)',
    '--footer-primary-fg': 'var(--text)',
    '--footer-font': 'var(--font)',
    '--footer-primary-left-display': 'none',
    '--footer-primary-right-display': 'inline-flex',
    '--footer-selected-icon-display': 'none',
    '--footer-nav-display': 'flex',
    '--kbd-font-size': '11px',
    '--kbd-padding': '2px 6px',
    '--kbd-radius': '4px',
    '--kbd-bg': 'var(--bg-hover)',
    '--kbd-border': 'var(--border)',
    '--kbd-fg': 'var(--text-dim)',
    '--kbd-shadow': 'inset 0 -1px 0 rgba(0,0,0,0.32)',

    // Action panel: wider, more breathable
    '--action-panel-width': '300px',
    '--action-panel-padding': '8px 8px',
    '--action-row-padding': '7px 10px',
    '--action-row-radius': '10px',
    '--action-row-selected-bg': 'var(--bg-select)',
    '--action-row-selected-fg': 'var(--text)',
    '--action-icon-size': '18px',
    '--action-font-size': '13px',
    '--action-kbd-bg': 'var(--bg-hover)',
    '--action-kbd-selected-bg': 'var(--bg-select-hover)',
    '--action-panel-radius': '12px',
    '--action-panel-shadow': '0 16px 48px rgba(0,0,0,0.48), 0 2px 8px rgba(0,0,0,0.32)',

    // Grid
    '--grid-padding': '12px 10px',
    '--grid-gap': '10px',
    '--grid-cell-radius': '10px',
    '--grid-cell-padding': '12px 6px',
    '--grid-icon-size': '36px',
    '--grid-icon-font-size': '24px',
    '--grid-label-font-size': '12px',
    '--grid-sublabel-font-size': '10px',
    '--grid-cell-bg': 'var(--bg-tab)',
    '--grid-cell-selected-bg': 'var(--bg-select)',
    '--grid-cell-border': 'var(--border)',
    '--grid-cell-selected-border': 'var(--accent)',
    '--grid-cell-selected-fg': 'var(--text)',

    // Detail pane
    '--detail-width': '35%',
    '--detail-padding': '14px 16px',
    '--detail-radius': '8px',

    // Form
    '--form-padding': '12px 14px',
    '--form-field-radius': '8px',

    // Breadcrumb
    '--breadcrumb-font-size': '12px',
    '--breadcrumb-padding': '6px 16px 4px',
    '--breadcrumb-border': '1px solid var(--divider)',

    // Results list container
    '--results-list-padding': '4px 0px',
  },
}

const BUILTIN_STYLES: UIStyle[] = [defaultStyle, onixStyle]

let appliedStyleKeys: string[] = []

export function applyStyle(name: string | undefined | null) {
  const style = BUILTIN_STYLES.find(s => s.name.toLowerCase() === (name ?? 'default').toLowerCase()) ?? defaultStyle
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
  const style = BUILTIN_STYLES.find(s => s.name.toLowerCase() === (name ?? 'default').toLowerCase()) ?? defaultStyle
  return style.name
}
