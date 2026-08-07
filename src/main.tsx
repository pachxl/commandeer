import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import './index.css'

// Both windows share the HTML entry, but each imports only its own application
// chunk. This keeps the screenshot webview from parsing the palette/providers
// and keeps the palette webview from loading the annotation overlay.
const isScreenshotWindow = getCurrentWindow().label === 'screenshot'

async function renderWindow() {
  const Component = isScreenshotWindow
    ? (await import('./components/ScreenshotOverlay')).default
    : (await import('./App')).default

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Component />
    </React.StrictMode>,
  )
}

void renderWindow()
