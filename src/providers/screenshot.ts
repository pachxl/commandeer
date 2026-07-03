// Region screenshot (Lightshot-style). The heavy lifting is on the Rust side;
// this just exposes the trigger in the palette. Rust hides the palette itself
// and waits for the compositor to unmap it before freezing the screen, so the
// palette never appears in the frame.
import type { Command, CommandProvider } from '../types'
import { startScreenshot } from '../lib/tauri'

const screenshotCommand: Command = {
  id: 'builtin:screenshot',
  label: 'Take Screenshot',
  description: 'Select a region — copied to clipboard and saved to Pictures/Screenshots',
  icon: 'camera',
  source: 'builtin',
  folderName: 'Tools',
  keywords: ['screenshot', 'snip', 'capture', 'screen', 'region', 'grab'],
  action: async () => {
    await startScreenshot()
  },
}

export const screenshotProvider: CommandProvider = {
  id: 'screenshot',
  name: 'Screenshot',
  priority: 50,
  getCommands: () => [screenshotCommand],
}
