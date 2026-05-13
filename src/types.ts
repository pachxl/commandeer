export interface AppConfig {
  scripts_dir: string
}

export interface PaletteItem {
  id: string
  label: string
  sublabel?: string
  icon: string
  searchText?: string
  isFolder?: boolean
  data?: unknown
}

export type StepResult =
  | { type: 'done' }
  | { type: 'push'; step: Step }

export interface Step {
  id: string
  label: string
  placeholder: string
  load?: (config: AppConfig) => Promise<PaletteItem[]>
  onSelect: (item: PaletteItem, config: AppConfig) => Promise<StepResult>
  // If true, pressing Enter with no selection confirms the raw query text
  isInputStep?: boolean
  onCommitQuery?: (query: string, config: AppConfig) => Promise<StepResult>
}

export interface Command {
  id: string
  label: string
  description?: string
  icon: string
  folderName?: string
  isFolder?: boolean
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
  | { type: 'MOVE_SELECTION'; delta: number }
  | { type: 'SET_LOADING'; loading: boolean }
  | { type: 'SET_ERROR'; error: string | null }
  | { type: 'RESET' }
