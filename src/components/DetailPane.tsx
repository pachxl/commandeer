import { useEffect, useState } from 'react'
import { fileInfo, readTextPreview, type FileInfo } from '../lib/tauri'
import { renderMarkdown } from '../lib/markdown'
import type { PaletteItem, PaletteMetadata } from '../types'

// Extensions the backend can thumbnail (raw bytes as a data URL)
const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|ico)$/i
// Extensions we can preview as plain text
const TEXT_EXT =
  /\.(txt|md|markdown|json|jsonc|js|jsx|ts|tsx|mjs|cjs|html|htm|css|scss|sass|less|xml|yaml|yml|toml|ini|cfg|conf|sh|bash|zsh|fish|ps1|py|rb|go|rs|c|cpp|h|hpp|cs|java|kt|swift|php|sql|log)$/i
const MARKDOWN_EXT = /\.(md|markdown)$/i

export function isImagePath(path: string): boolean {
  return IMAGE_EXT.test(path)
}

function isTextPath(path: string): boolean {
  return TEXT_EXT.test(path)
}

function isMarkdownPath(path: string): boolean {
  return MARKDOWN_EXT.test(path)
}

// Shared renderer for a block of markdown (generic detail + .md previews).
function MarkdownBlock({ label, source }: { label: string; source: string }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
        {label}
      </span>
      <div className="md-body" dangerouslySetInnerHTML={{ __html: renderMarkdown(source) }} />
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(2)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${bytes} B`
}

function metadataRow(label: string, value: string) {
  return (
    <div key={label} style={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
      <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
        {label}
      </span>
      <span style={{ fontSize: 12, color: 'var(--text)', wordBreak: 'break-all' }}>{value}</span>
    </div>
  )
}

function MetadataRows({ metadata }: { metadata: PaletteMetadata[] }) {
  return <>{metadata.map(m => metadataRow(m.label, m.value))}</>
}

interface DetailPaneProps {
  item: PaletteItem
}

// Raycast-style preview/detail pane for the highlighted item. Shows:
//   - image thumbnails for image files
//   - text previews for text files
//   - color swatches for items with a color payload
//   - font previews for items with a fontFamily payload
//   - generic metadata rows for every item that carries them
export default function DetailPane({ item }: DetailPaneProps) {
  const [info, setInfo] = useState<FileInfo | null>(null)
  const [textPreview, setTextPreview] = useState<string | null>(null)
  const [textError, setTextError] = useState<string | null>(null)

  const path = typeof item.data === 'string' ? item.data : null
  const isImage = path ? isImagePath(path) : false
  const isText = path ? isTextPath(path) : false
  const isFile = item.source === 'file' && path

  useEffect(() => {
    let cancelled = false
    setInfo(null)
    setTextPreview(null)
    setTextError(null)

    if (isFile && path) {
      fileInfo(path)
        .then(i => {
          if (!cancelled) setInfo(i)
        })
        .catch(() => {})
    }

    if (path && isText) {
      readTextPreview(path)
        .then(t => {
          if (!cancelled) setTextPreview(t)
        })
        .catch(err => {
          if (!cancelled) setTextError(String(err))
        })
    }

    return () => {
      cancelled = true
    }
  }, [item.id, path, isText, isFile])

  const isMarkdown = path ? isMarkdownPath(path) : false
  const title = item.label
  const hasContent =
    isImage ||
    isText ||
    item.color ||
    item.fontFamily ||
    item.detailMarkdown ||
    (item.metadata && item.metadata.length > 0)
  if (!hasContent) return null

  return (
    <div
      data-detail-pane
      style={{
        width: 'var(--detail-width)',
        flexShrink: 0,
        borderLeft: '1px solid var(--divider)',
        background: 'var(--detail-bg)',
        padding: 'var(--detail-padding)',
        overflowY: 'auto',
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
        fontFamily: 'var(--font-ui)',
        userSelect: 'none',
      }}
    >
      {isImage && info?.thumbnail && (
        <img
          src={info.thumbnail}
          style={{
            maxWidth: '100%',
            maxHeight: 140,
            objectFit: 'contain',
            borderRadius: 'var(--detail-radius)',
            alignSelf: 'center',
          }}
        />
      )}

      {item.color && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
            Color
          </span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <div
              style={{
                width: 40,
                height: 40,
                borderRadius: 'var(--detail-radius)',
                background: item.color,
                border: '1px solid var(--border)',
              }}
            />
            <span style={{ fontSize: 12, color: 'var(--text)', fontFamily: 'var(--font-mono, var(--font))' }}>
              {item.color}
            </span>
          </div>
        </div>
      )}

      {item.fontFamily && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
            Font
          </span>
          <div
            style={{
              fontFamily: item.fontFamily,
              fontSize: 24,
              color: 'var(--text)',
              lineHeight: 1.3,
            }}
          >
            Aa Bb Cc 123
          </div>
          <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>{item.fontFamily}</span>
        </div>
      )}

      {item.detailMarkdown && <MarkdownBlock label="Details" source={item.detailMarkdown} />}

      {isText && textPreview && isMarkdown && (
        // .md/.markdown files render their preview as formatted markdown
        <div style={{ maxHeight: 240, overflowY: 'auto' }}>
          <MarkdownBlock label="Preview" source={textPreview} />
        </div>
      )}

      {isText && textPreview && !isMarkdown && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <span style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.6, color: 'var(--text-dim)' }}>
            Preview
          </span>
          <pre
            style={{
              margin: 0,
              padding: 8,
              borderRadius: 'var(--detail-radius)',
              background: 'rgba(255,255,255,0.04)',
              border: '1px solid var(--border)',
              color: 'var(--text)',
              fontSize: 11,
              fontFamily: 'var(--font-mono, var(--font))',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              maxHeight: 200,
              overflowY: 'auto',
            }}
          >
            {textPreview}
          </pre>
        </div>
      )}

      {isText && textError && (
        <div style={{ fontSize: 11, color: 'var(--text-dim)', fontStyle: 'italic' }}>Preview unavailable</div>
      )}

      <div
        title={title}
        style={{
          fontSize: 13,
          fontFamily: 'var(--font)',
          color: 'var(--text)',
          fontWeight: 600,
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
        }}
      >
        {title}
      </div>

      {item.metadata && item.metadata.length > 0 && <MetadataRows metadata={item.metadata} />}

      {info && (
        <>
          {!info.is_dir && metadataRow('Size', formatBytes(info.size))}
          {info.modified && metadataRow('Modified', new Date(info.modified).toLocaleString())}
          {path && metadataRow('Path', path)}
        </>
      )}
    </div>
  )
}
