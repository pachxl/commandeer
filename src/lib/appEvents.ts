// Bridge between App-level state (game mode, usage panels, command
// list) and commands defined outside the component tree. App registers its
// toggles and getters here; commands and settings steps read/call them.
export const appEvents: {
  toggleGameMode?: () => void
  toggleClaudeUsage?: () => void
  toggleCodexUsage?: () => void
  toggleWebSearch?: () => void
  toggleSystemStats?: () => void
  isGameMode?: () => boolean
  isClaudeUsageVisible?: () => boolean
  isCodexUsageVisible?: () => boolean
  isWebSearchVisible?: () => boolean
  isSystemStatsVisible?: () => boolean
  // Palette scale (CSS zoom factor). getScale reads the current factor; setScale
  // applies it live (used by the Settings scale slider for real-time feedback).
  getScale?: () => number
  setScale?: (scale: number) => void
  // Rebuild the root command list (e.g. after a note or quick link changes)
  refreshCommands?: () => void
  // Show a transient toast above the results list (registered by Palette)
  toast?: (message: string, kind?: 'success' | 'error' | 'info') => void
  // Show a HUD confirmation pill and then dismiss the launcher (registered by
  // Palette). Use for actions that close the palette but need visible feedback
  // (copy/paste), where a toast would vanish with the window before it's seen.
  showHud?: (message: string, icon?: string) => void
  // Ask the user to confirm a destructive action; resolves true if confirmed.
  // A remembered "Don't ask again" (keyed) resolves true without prompting.
  // Registered by Palette; typed loosely here to avoid importing UI types.
  confirm?: (options: import('./confirm').ConfirmOptions) => Promise<boolean>
} = {}
