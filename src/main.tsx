import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import './index.css'
import App from './App'
import ScreenshotOverlay from './components/ScreenshotOverlay'

// The screenshot window shares this bundle: same HTML entry, different root
// component picked by window label.
const isScreenshotWindow = getCurrentWindow().label === 'screenshot'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {isScreenshotWindow ? <ScreenshotOverlay /> : <App />}
  </React.StrictMode>,
)
