export interface AppConfig {
  scripts_dir: string
  // Used by the testing-branch build (file search); round-tripped so builds can coexist
  search_paths?: string[]
  // Theme name ('Tokyo Night', 'Light', …); legacy values 'dark'/'light' still resolve
  theme?: string
  // Window transparency: 0.0 (fully opaque) to 1.0 (fully transparent)
  transparency?: number
}

export interface PaletteItem {
  id: string
  label: string
  sublabel?: string
  icon: string
  searchText?: string
  isFolder?: boolean
  data?: unknown
  // Label of the primary (Enter) action, shown in the footer
  actionLabel?: string
}

export type StepResult =
  | { type: 'done' }
  | { type: 'push'; step: Step }
  | { type: 'replace'; step: Step }
  | { type: 'pop' }

export interface Step {
  id: string
  label: string
  placeholder: string
  load?: (config: AppConfig) => Promise<PaletteItem[]>
  onSelect: (item: PaletteItem, config: AppConfig) => Promise<StepResult>
  // If true, pressing Enter with no selection confirms the raw query text
  isInputStep?: boolean
  onCommitQuery?: (query: string, config: AppConfig) => Promise<StepResult>
  // For slider steps
  isSliderStep?: boolean
  minValue?: number
  maxValue?: number
  stepValue?: number
  onSliderChange?: (value: number, config: AppConfig) => Promise<void>
}

export interface Command {
  id: string
  label: string
  description?: string
  icon: string
  folderName?: string
  isFolder?: boolean
  // Extra terms matched by the fuzzy search
  keywords?: string[]
  // Label of the primary (Enter) action, shown in the footer
  actionLabel?: string
  // Either run directly (scripts/shortcuts) or push a step (multi-step commands)
  action?: (config: AppConfig) => Promise<void>
  createRootStep?: (config: AppConfig) => Step
  // If true, selecting this command won't close the palette
  noClose?: boolean
}

export interface PaletteState {
  query: string
  stepStack: Step[]
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
  | { type: 'REPLACE_STEP'; step: Step }
  | { type: 'MOVE_SELECTION'; delta: number }
  | { type: 'SET_LOADING'; loading: boolean }
  | { type: 'SET_ERROR'; error: string | null }
  | { type: 'RESET' }
