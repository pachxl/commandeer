// Safe frontend target for an external commandeer://command/<id> URI.
// Root-step commands keep their existing navigation/confirmation UI. A leaf
// action is wrapped in a one-row step so the URI itself cannot execute it: the
// user must explicitly press Enter or click the row.

import type { AppConfig, Command, Step } from '../types'
import { commandToItem } from './paletteItems'

export function commandDeepLinkStep(
  command: Command | undefined,
  config: AppConfig,
  onActivated?: (commandId: string) => void,
): Step | undefined {
  if (!command) return undefined

  // Destructive/confirming commands expose confirmation as their root step.
  // Prefer it even if a future command accidentally supplies both forms.
  if (command.createRootStep) return command.createRootStep(config)
  if (!command.action) return undefined

  const item = commandToItem(command)
  return {
    id: `deep-link:${command.id}`,
    label: command.label,
    placeholder: `${command.label} — press Enter to continue`,
    load: async () => [item],
    onSelect: async (_item, currentConfig) => {
      await command.action!(currentConfig)
      onActivated?.(command.id)
      return command.noClose ? { type: 'stay' } : { type: 'done' }
    },
  }
}
