// UI style presets applied as CSS variables on :root. Default is deliberately
// theme-owned. Onix is the one intentional exception: its "Black Water"
// material owns dark neutral surfaces/foregrounds while the selected theme
// continues to supply the accent colour.

export interface UIStyle {
  name: string
  variables: Record<string, string>
}

const DEFAULT_VARIABLES: Record<string, string> = {
  // Typography
  '--font': "'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif",
  '--font-ui': "'JetBrains Mono', 'Fira Code', Consolas, monospace",
  '--font-mono': "'JetBrains Mono', 'Fira Code', Consolas, monospace",

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

// Black Water: a compact optical search capsule that blooms into a dark,
// liquid-glass command panel. The `--onix-*` tokens are consumed only by the
// scoped Onix CSS/optical renderer; the ordinary component tokens below keep
// every existing view (list, grid, form, slider, mixer) functional.
const onixStyle: UIStyle = {
  name: 'Onix',
  variables: {
    ...DEFAULT_VARIABLES,

    // Optical material. The procedural rim and native material share one
    // coincident edge; an inset shell reads as a nested capsule and makes
    // compositor shadows tear at the lower corners.
    '--onix-window-gutter': '0px',
    '--onix-capsule-height': '66px',
    '--onix-capsule-radius': '33px',
    '--onix-panel-radius': '25px',
    '--onix-material': 'rgba(7, 9, 12, 0.82)',
    '--onix-material-deep': 'rgba(2, 3, 5, 0.94)',
    '--onix-material-raised': 'rgba(18, 21, 27, 0.86)',
    '--onix-material-soft': 'rgba(255, 255, 255, 0.045)',
    '--onix-material-hover': 'rgba(255, 255, 255, 0.055)',
    '--onix-material-selected': 'rgba(255, 255, 255, 0.082)',
    '--onix-foreground': 'rgba(247, 249, 252, 0.96)',
    '--onix-foreground-muted': 'rgba(189, 197, 209, 0.64)',
    '--onix-foreground-faint': 'rgba(174, 184, 198, 0.40)',
    '--onix-rim': 'rgba(225, 238, 255, 0.28)',
    '--onix-rim-hot': 'rgba(255, 255, 255, 0.60)',
    '--onix-rim-cool': 'rgba(112, 174, 255, 0.18)',
    '--onix-absorption': 'rgba(0, 0, 0, 0.72)',
    '--onix-shadow': 'inset 0 1px 0 rgba(255,255,255,0.055), inset 0 -1px 0 rgba(0,0,0,0.52)',
    '--onix-lens-shadow':
      'inset 0 1px 0 rgba(255,255,255,0.10), inset 0 -1px 0 rgba(0,0,0,0.48), 0 7px 22px rgba(0,0,0,0.22)',
    '--palette-border': 'transparent',
    '--palette-shadow': 'none',
    '--divider': 'rgba(255,255,255,0.075)',
    '--surface-muted': 'rgba(255,255,255,0.025)',
    '--detail-bg': 'rgba(1,3,6,0.20)',
    '--form-field-bg': 'rgba(255,255,255,0.045)',

    // Typography
    '--font': "'SF Pro Text', Inter, 'Segoe UI Variable', system-ui, sans-serif",
    '--font-ui': "'SF Pro Text', Inter, 'Segoe UI Variable', system-ui, sans-serif",
    '--font-mono': "'SF Mono', 'JetBrains Mono', Consolas, monospace",

    // Calm rows, one shared moving lens, restrained icon wells.
    '--row-padding': '7px 13px',
    '--row-gap': '11px',
    '--row-radius': '13px',
    '--row-inset': '5px',
    '--row-selected-bg': 'transparent',
    '--row-selected-fg': 'var(--onix-foreground)',
    '--row-selected-sublabel-fg': 'var(--onix-foreground-muted)',
    '--row-hover-bg': 'var(--onix-material-hover)',
    '--row-active-indicator-bg': '#55d68b',
    '--row-selected-shadow': 'none',
    '--row-transition':
      'background-color 105ms ease, border-color 105ms ease, color 105ms ease, box-shadow 120ms ease, transform 95ms ease',
    '--sublabel-margin-left': '0px',
    '--sublabel-font': 'var(--font)',
    '--sublabel-flex-shrink': '1',
    '--icon-size': '30px',
    '--icon-font-size': '16px',
    '--row-svg-size': '18px',
    '--label-font-size': '13.5px',
    '--sublabel-font-size': '11.5px',
    '--accessory-font-size': '10.5px',
    '--accessory-radius': '999px',
    '--accessory-padding': '2px 7px',
    '--accessory-bg': 'rgba(255,255,255,0.045)',
    '--accessory-border': 'rgba(255,255,255,0.075)',
    '--accessory-border-width': '1px',
    '--accessory-font': 'var(--font)',

    '--search-padding': '0 20px',
    '--search-height': '66px',
    '--search-font-size': '17px',
    '--search-icon-size': '19px',
    '--search-gap': '13px',
    '--search-back-bg': 'rgba(255,255,255,0.045)',
    '--preview-label-font-size': '16px',
    '--preview-sublabel-font-size': '11px',

    '--footer-padding': '7px 13px 9px',
    '--footer-height': '42px',
    '--footer-font-size': '11px',
    '--footer-button-radius': '9px',
    '--footer-button-padding': '4px 8px',
    '--footer-bg': 'rgba(0,0,0,0.14)',
    '--footer-hover-bg': 'rgba(255,255,255,0.055)',
    '--footer-primary-fg': 'var(--onix-foreground)',
    '--footer-font': 'var(--font)',
    '--footer-primary-left-display': 'none',
    '--footer-primary-right-display': 'inline-flex',
    '--footer-selected-icon-display': 'none',
    '--footer-nav-display': 'flex',
    '--kbd-font-size': '11px',
    '--kbd-padding': '2px 6px',
    '--kbd-radius': '4px',
    '--kbd-bg': 'rgba(255,255,255,0.055)',
    '--kbd-border': 'rgba(255,255,255,0.095)',
    '--kbd-fg': 'var(--onix-foreground-muted)',
    '--kbd-shadow': 'inset 0 1px 0 rgba(255,255,255,0.045), inset 0 -1px 0 rgba(0,0,0,0.42)',

    // Dense B-style action treatment.
    '--action-panel-width': '282px',
    '--action-panel-padding': '8px 7px',
    '--action-row-padding': '7px 9px',
    '--action-row-radius': '10px',
    '--action-row-selected-bg': 'transparent',
    '--action-row-selected-fg': 'var(--onix-foreground)',
    '--action-icon-size': '16px',
    '--action-font-size': '12px',
    '--action-kbd-bg': 'rgba(255,255,255,0.045)',
    '--action-kbd-selected-bg': 'rgba(255,255,255,0.09)',
    '--action-panel-radius': '17px',
    '--action-panel-shadow': '0 22px 64px rgba(0,0,0,0.62), inset 0 1px 0 rgba(255,255,255,0.06)',

    // Grid
    '--grid-padding': '10px 9px',
    '--grid-gap': '8px',
    '--grid-cell-radius': '14px',
    '--grid-cell-padding': '11px 6px',
    '--grid-icon-size': '34px',
    '--grid-icon-font-size': '24px',
    '--grid-label-font-size': '12px',
    '--grid-sublabel-font-size': '10px',
    '--grid-cell-bg': 'rgba(255,255,255,0.018)',
    '--grid-cell-selected-bg': 'transparent',
    '--grid-cell-border': 'rgba(255,255,255,0.045)',
    '--grid-cell-selected-border': 'transparent',
    '--grid-cell-selected-fg': 'var(--onix-foreground)',

    // Detail pane
    '--detail-width': '36%',
    '--detail-padding': '14px 15px',
    '--detail-radius': '11px',

    // Form
    '--form-padding': '14px 16px',
    '--form-field-radius': '11px',

    // Breadcrumb
    '--breadcrumb-font-size': '12px',
    '--breadcrumb-padding': '6px 16px 4px',
    '--breadcrumb-border': '1px solid var(--divider)',

    // Results list container
    '--results-list-padding': '5px 7px 7px',
  },
}

const BUILTIN_STYLES: UIStyle[] = [defaultStyle, onixStyle]

let appliedStyleKeys: string[] = []

export const UI_STYLE_CHANGE_EVENT = 'commandeer:ui-style-change'

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
  window.dispatchEvent(new CustomEvent(UI_STYLE_CHANGE_EVENT, { detail: style.name }))
}

export function getAppliedStyleName(): string {
  return getStyleName(document.documentElement.getAttribute('data-style'))
}

export function getAllStyles(): UIStyle[] {
  return BUILTIN_STYLES
}

export function getStyleName(name: string | undefined | null): string {
  const style = BUILTIN_STYLES.find(s => s.name.toLowerCase() === (name ?? 'default').toLowerCase()) ?? defaultStyle
  return style.name
}
