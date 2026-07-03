import type { Command, CommandProvider, Step } from '../types'
import { systemAction, type SystemActionId } from '../lib/tauri'

interface SystemCommand {
  id: SystemActionId
  label: string
  description: string
  icon: string
  keywords: string[]
  // Confirmation prompt: when set, Enter pushes a confirm step instead of
  // firing immediately — a fuzzy-matched "res" landing on Restart shouldn't
  // nuke the session. Confirm is preselected, so the fast path is Enter-Enter.
  confirm?: string
}

const SYSTEM_COMMANDS: SystemCommand[] = [
  { id: 'lock', label: 'Lock Screen', description: 'Lock your computer', icon: 'lock', keywords: ['lock', 'screen'] },
  { id: 'sleep', label: 'Sleep', description: 'Put your computer to sleep', icon: 'moon', keywords: ['sleep', 'suspend'] },
  { id: 'hibernate', label: 'Hibernate', description: 'Hibernate your computer', icon: 'snowflake', keywords: ['hibernate'] },
  { id: 'restart', label: 'Restart', description: 'Restart your computer', icon: 'refresh', keywords: ['restart', 'reboot'],
    confirm: 'Restart the computer now?' },
  { id: 'shutdown', label: 'Shut Down', description: 'Shut down your computer', icon: 'power', keywords: ['shutdown', 'power off', 'turn off'],
    confirm: 'Shut down the computer now?' },
  { id: 'logout', label: 'Log Out', description: 'Log out of your account', icon: 'logout', keywords: ['logout', 'sign out'],
    confirm: 'Log out of this session? Unsaved work will be lost.' },
  { id: 'empty-trash', label: 'Empty Trash', description: 'Empty the recycle bin', icon: 'trash', keywords: ['trash', 'recycle bin', 'empty'],
    confirm: 'Permanently delete everything in the Recycle Bin?' },
]

function confirmStep(sc: SystemCommand): Step {
  return {
    id: `system:${sc.id}:confirm`,
    label: sc.label,
    placeholder: sc.confirm ?? `${sc.label}?`,
    load: async () => [
      { id: 'confirm', label: sc.label, sublabel: sc.confirm, icon: sc.icon, actionLabel: 'Confirm' },
      { id: 'cancel', label: 'Cancel', icon: 'x', actionLabel: 'Cancel' },
    ],
    onSelect: async item => {
      if (item.id !== 'confirm') return { type: 'pop' }
      await systemAction(sc.id)
      return { type: 'done' }
    },
  }
}

function actionCommand(sc: SystemCommand): Command {
  const base = {
    id: `system:${sc.id}`,
    label: sc.label,
    description: sc.description,
    icon: sc.icon,
    source: 'system' as const,
    folderName: 'System',
    keywords: sc.keywords,
  }
  return sc.confirm
    ? { ...base, createRootStep: () => confirmStep(sc) }
    : { ...base, action: async () => { await systemAction(sc.id) } }
}

export const systemProvider: CommandProvider = {
  id: 'system',
  name: 'System',
  priority: 40,
  getCommands: () => SYSTEM_COMMANDS.map(actionCommand),
}
