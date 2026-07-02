// Bridge between App-level state (game mode, Claude usage panel) and
// commands defined outside the component tree. App registers its toggles
// and getters here; the settings step reads/calls them.
export const appEvents: {
  toggleGameMode?: () => void
  toggleClaudeUsage?: () => void
  isGameMode?: () => boolean
  isClaudeUsageVisible?: () => boolean
} = {}
