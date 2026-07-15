import type { Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import { getVolume, setVolume, toggleMute, listAudioDevices, type AudioDevice } from '../lib/tauri'

// A slider step (0–100%) that reads and drives one device's volume live.
function deviceSliderStep(device: AudioDevice): Step {
  return {
    id: `volume:slider:${device.id}`,
    label: device.name,
    placeholder: `Volume — ${device.name}`,
    icon: 'volume',
    isSliderStep: true,
    minValue: 0,
    maxValue: 100,
    stepValue: 1,
    loadSliderValue: async () => {
      const level = await getVolume(device.id) // 0.0..1.0
      return Math.round(level * 100)
    },
    onSliderChange: async (value: number): Promise<void> => {
      await setVolume(Math.min(100, Math.max(0, value)) / 100, device.id)
    },
    load: async () => [],
    onSelect: async () => ({ type: 'pop' }),
  }
}

// Output devices (default first), each opening its own slider.
function devicesStep(): Step {
  return {
    id: 'volume:devices',
    label: 'Set Volume',
    placeholder: 'Select an output device...',
    load: async (): Promise<PaletteItem[]> => {
      const devices = await listAudioDevices()
      return devices.map(d => ({
        id: `volume:device:${d.id}`,
        label: d.name,
        sublabel: d.is_default ? 'Default output' : undefined,
        icon: 'volume',
        isFolder: true,
        actionLabel: 'Adjust',
        data: d,
      }))
    },
    onSelect: async (item): Promise<StepResult> => ({ type: 'push', step: deviceSliderStep(item.data as AudioDevice) }),
  }
}

export const volumeProvider: CommandProvider = {
  id: 'volume',
  name: 'Volume',
  priority: 40,
  getCommands: (): Command[] => [
    {
      id: 'volume:set',
      label: 'Set Volume',
      description: 'Adjust the volume of an output device',
      icon: 'volume',
      source: 'system',
      folderName: 'System',
      keywords: ['volume', 'audio', 'sound', 'loudness', 'output device'],
      createRootStep: () => devicesStep(),
    },
    {
      id: 'volume:mute-toggle',
      label: 'Toggle Mute',
      description: 'Mute or unmute the system output',
      icon: 'volume',
      source: 'system',
      folderName: 'System',
      keywords: ['mute', 'unmute', 'silence', 'audio', 'sound'],
      noClose: true,
      action: async () => {
        await toggleMute()
      },
    },
  ],
}
