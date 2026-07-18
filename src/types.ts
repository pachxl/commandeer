export type CommandSource =
  | 'builtin'
  | 'script'
  | 'app'
  | 'window'
  | 'file'
  | 'calculator'
  | 'system'
  | 'quicklink'
  | 'clipboard'
  | 'bookmark'
  | 'note'

export interface PaletteAccessory {
  text?: string
  icon?: string
  color?: string
}

export interface PaletteMetadata {
  label: string
  value: string
}

export interface AppConfig {
  scripts_dir: string
  // Roots for the global find: file search (defaults to Desktop/Documents/Downloads)
  search_paths?: string[]
  // Theme name ('Tokyo Night', 'Light', …); legacy values 'dark'/'light' still resolve
  theme?: string
  // Window transparency: 0.0 (fully opaque) to 1.0 (fully transparent)
  transparency?: number
  // Global hotkey that toggles the palette (e.g. 'Ctrl+Space')
  global_hotkey?: string
  // Alternate global hotkey used in game mode (e.g. 'Alt+Space')
  global_hotkey_game?: string
  // Global hotkey that starts the region screenshot (default 'PrintScreen';
  // Windows only — Linux uses a managed COSMIC binding instead)
  screenshot_hotkey?: string
  // Alt-drag window management: hold Alt to move any window, Alt + right-drag to
  // resize (Windows/macOS only). Default off.
  window_drag?: boolean
  // Replace Windows Alt+Tab with a switcher containing only windows on the
  // monitor under the cursor. Windows only; default off.
  per_monitor_alt_tab?: boolean
  // Palette scale factor applied as a CSS zoom to the whole palette (and used to
  // scale the window width/height). 1.0 = default size; the Settings slider maps
  // 0–100% onto 0.5×–1.5× with 50% = 1.0×.
  palette_scale?: number
  // UI style preset ('Default' or 'Onix'). Controls layout, spacing, fonts, and
  // component treatment; the separate theme setting owns all colors.
  ui_style?: string
  // Background auto-update from GitHub Releases. Undefined/true = on.
  auto_update?: boolean
}

export interface PaletteItem {
  id: string
  label: string
  sublabel?: string
  icon: string
  // Filesystem path whose shell icon should replace `icon` once resolved
  // (fetched lazily per visible row, cached per extension)
  iconPath?: string
  searchText?: string
  isFolder?: boolean
  data?: unknown
  // Label of the primary (Enter) action, shown in the footer
  actionLabel?: string
  // Which provider produced this item (drives action-panel actions)
  source?: CommandSource
  // Keywords used for weighted multi-field fuzzy ranking
  keywords?: string[]
  // CSS color for a swatch (used by the color formatter); also shows a
  // swatch preview in the detail pane
  color?: string
  // Tint the row icon without implying the item *is* a color (no swatch preview)
  iconColor?: string
  // Render the label/sublabel in this font family (used by the font browser)
  fontFamily?: string
  // Right-aligned badges/tags
  accessories?: PaletteAccessory[]
  // Key/value rows rendered in the detail pane
  metadata?: PaletteMetadata[]
  // For inline scripts (@vicinae.mode inline + refreshTime): the script path
  // the frontend polls, whose captured stdout replaces this item's sublabel
  // live. Presence marks the item as a live-refreshing inline script.
  liveOutputKey?: string
  // App is currently running — renders a small status dot before the label
  running?: boolean
  // Markdown rendered as a formatted "Details" section in the detail pane
  detailMarkdown?: string
}

export type StepResult =
  | { type: 'done' }
  | { type: 'push'; step: Step }
  | { type: 'replace'; step: Step }
  | { type: 'pop' }
  // Keep the palette open on the current step, untouched (e.g. calculator
  // commit: copy the result and stay put)
  | { type: 'stay' }

export interface FormFieldOption {
  label: string
  value: string
}

export interface LivePreview {
  label: string
  sublabel?: string
  copy: string
}

export interface FormField {
  id: string
  label: string
  type: 'text' | 'dropdown' | 'checkbox'
  placeholder?: string
  defaultValue?: unknown
  options?: FormFieldOption[]
}

export interface Step {
  id: string
  label: string
  placeholder: string
  load?: (config: AppConfig) => Promise<PaletteItem[]>
  onSelect: (item: PaletteItem, config: AppConfig) => Promise<StepResult>
  // Called when the highlighted item changes (arrow keys or hover) — used
  // for live previews (e.g. themes). Not called for the initial selection.
  onHighlight?: (item: PaletteItem) => void
  // Called when the step leaves the top of the stack (pop, replace, reset).
  // Pair with onHighlight to undo an uncommitted preview.
  onExit?: () => void
  // If true, pressing Enter with no selection confirms the raw query text
  isInputStep?: boolean
  onCommitQuery?: (query: string, config: AppConfig) => Promise<StepResult>
  // Live preview shown on the right side of the search input (e.g. calculator result)
  livePreview?: (query: string) => LivePreview | null
  // For slider steps
  isSliderStep?: boolean
  minValue?: number
  maxValue?: number
  stepValue?: number
  // Optional async seed for the slider's starting position (e.g. current volume)
  loadSliderValue?: () => Promise<number>
  // Icon shown beside the slider (defaults to an eye/opacity icon)
  icon?: string
  onSliderChange?: (value: number, config: AppConfig) => Promise<void>
  // For grid steps: render load items as a tiled grid instead of a list
  isGridStep?: boolean
  gridColumns?: number
  // For form steps: multi-field input resolved by a single submit
  isFormStep?: boolean
  fields?: FormField[]
  onSubmit?: (values: Record<string, unknown>, config: AppConfig) => Promise<StepResult>
}

export interface Command {
  id: string
  label: string
  description?: string
  icon: string
  // Which provider produced this command (drives action-panel actions)
  source?: CommandSource
  // Filesystem path whose shell icon should replace `icon` once resolved
  iconPath?: string
  folderName?: string
  isFolder?: boolean
  // Extra terms matched by the fuzzy search
  keywords?: string[]
  aliases?: string[]
  // Label of the primary (Enter) action, shown in the footer
  actionLabel?: string
  // If true, hidden from the browse list — only findable by typing a query
  searchOnly?: boolean
  // Optional payload passed through to the UI as PaletteItem.data
  data?: unknown
  // Either run directly (scripts/shortcuts) or push a step (multi-step commands)
  action?: (config: AppConfig) => Promise<void>
  createRootStep?: (config: AppConfig) => Step
  // If true, selecting this command won't close the palette
  noClose?: boolean
  // CSS color for a swatch (used by the color formatter)
  color?: string
  // Right-aligned badges/tags
  accessories?: PaletteAccessory[]
  // Key/value rows rendered in the detail pane
  metadata?: PaletteMetadata[]
  // For inline scripts (@vicinae.mode inline + refreshTime): the script path
  // the frontend polls, whose captured stdout becomes a live sublabel. When
  // set, Enter force-refreshes instead of fire-and-forget running.
  liveOutputKey?: string
  // App is currently running — renders a small status dot before the label
  running?: boolean
  // Markdown rendered as a formatted "Details" section in the detail pane
  detailMarkdown?: string
}

// A source of commands: static entries for the root list (getCommands) and/or
// dynamic per-query results surfaced inline at root (search)
export interface CommandProvider {
  id: string
  name: string
  priority: number
  getCommands?: (config: AppConfig) => Promise<Command[]> | Command[]
  search?: (query: string, config: AppConfig) => Promise<Command[]> | Command[]
}

// One entry in the Ctrl+K action panel
export interface ActionItem {
  id: string
  label: string
  shortcut?: string
  icon?: string
  // Leaf action: runs when selected. Omit when `submenu` is set.
  handler?: () => Promise<void>
  // Nested actions: selecting this row opens them as a sub-menu instead of
  // running a handler (e.g. a "Copy…" group). Esc / ← returns to the parent.
  submenu?: ActionItem[]
}

export interface PaletteState {
  query: string
  stepStack: Step[]
  // Query + highlighted row at each ancestor level, restored when popping
  // back so Esc/Left/Backspace return to where you were, not the top
  selectionStack: { query: string; selectedIndex: number }[]
  itemCache: Record<string, PaletteItem[]>
  selectedIndex: number
  loading: boolean
  error: string | null
}

export type PaletteAction =
  | { type: 'SET_QUERY'; query: string }
  | { type: 'SET_ITEMS'; stepId: string; items: PaletteItem[]; preserveSelection?: boolean }
  | { type: 'PUSH_STEP'; step: Step }
  | { type: 'POP_STEP' }
  | { type: 'REPLACE_STEP'; step: Step; preserveSelection?: boolean }
  | { type: 'SET_SELECTION'; index: number }
  | { type: 'SET_LOADING'; loading: boolean }
  | { type: 'SET_ERROR'; error: string | null }
  | { type: 'RESET' }
