import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

// Tauri windows mount while hidden, so assume the conservative state until
// the native window confirms otherwise. Register the listener before reading
// the initial state so a focus transition during setup cannot be missed.
export function useWindowFocused(): boolean {
  const [focused, setFocused] = useState(false)

  useEffect(() => {
    let disposed = false
    let eventVersion = 0
    let unlisten: (() => void) | undefined
    const window = getCurrentWindow()

    void (async () => {
      try {
        const registered = await window.onFocusChanged(({ payload }) => {
          eventVersion += 1
          if (!disposed) setFocused(payload)
        })
        if (disposed) {
          registered()
          return
        }
        unlisten = registered
      } catch (error) {
        console.error('window focus listener:', error)
      }

      const versionBeforeRead = eventVersion
      try {
        const initiallyFocused = await window.isFocused()
        if (!disposed && eventVersion === versionBeforeRead) setFocused(initiallyFocused)
      } catch (error) {
        console.error('window focus state:', error)
      }
    })()

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])

  return focused
}
