// Bridge between App-level state (game mode, Claude usage panel, command
// list) and commands defined outside the component tree. App registers its
// toggles and getters here; commands and settings steps read/call them.
export const appEvents: {
  toggleGameMode?: () => void
  toggleClaudeUsage?: () => void
  toggleWebSearch?: () => void
  isGameMode?: () => boolean
  isClaudeUsageVisible?: () => boolean
  isWebSearchVisible?: () => boolean
  // Rebuild the root command list (e.g. after snippets change)
  refreshCommands?: () => void
} = {}
