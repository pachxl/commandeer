import { useEffect, useState } from 'react'
import { fileInfo, type FileInfo } from '../lib/tauri'

interface DetailPaneProps {
  path: string
  name: string
}

// Extensions the backend can thumbnail (raw bytes as a data URL)
const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|ico)$/i

export function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path)
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(2)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

function row(label: string, value: string) {
  return (
    <div key={label} style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
      <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
        {label}
      </span>
      <span style={{ fontSize: 12, color: 'var(--text)', wordBreak: 'break-all' }}>
        {value}
      </span>
    </div>
  )
}

// Raycast-style preview pane for image results: thumbnail plus basic metadata.
// Only rendered when the highlighted file is an image (see isImagePath).
export default function DetailPane({ path, name }: DetailPaneProps) {
  const [info, setInfo] = useState<FileInfo | null>(null)

  useEffect(() => {
    let cancelled = false
    setInfo(null)
    fileInfo(path)
      .then(i => { if (!cancelled) setInfo(i) })
      .catch(() => {})
    return () => { cancelled = true }
  }, [path])

  return (
    <div
      style={{
        width: '40%',
        flexShrink: 0,
        borderLeft: '1px solid var(--border)',
        padding: '10px 12px',
        overflowY: 'auto',
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
        fontFamily: 'var(--font-ui)',
        userSelect: 'none',
      }}
    >
      {info?.thumbnail && (
        <img
          src={info.thumbnail}
          style={{
            maxWidth: '100%',
            maxHeight: 140,
            objectFit: 'contain',
            borderRadius: 4,
            alignSelf: 'center',
          }}
        />
      )}
      <div title={name} style={{
        fontSize: 13,
        fontFamily: 'var(--font)',
        color: 'var(--text)',
        fontWeight: 600,
        whiteSpace: 'nowrap',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
      }}>
        {name}
      </div>
      {info && (
        <>
          {!info.is_dir && row('Size', formatBytes(info.size))}
          {info.modified && row('Modified', new Date(info.modified).toLocaleString())}
          {row('Path', path)}
        </>
      )}
    </div>
  )
}
