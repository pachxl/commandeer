import { useCallback, useEffect, useRef, useState } from 'react'
import {
  listAudioSessions,
  pathIcon,
  setAudioSessionVolume,
  toggleAudioSessionMute,
  type AudioSession,
} from '../lib/tauri'
import { getIconSvg } from './Icon'

interface VolumeMixerProps {
  onError: (error: string | null) => void
}

const iconPromises = new Map<string, Promise<string | null>>()
const resolvedIcons = new Map<string, string | null>()

function loadAppIcon(path: string): Promise<string | null> {
  const key = path.toLowerCase()
  let cached = iconPromises.get(key)
  if (!cached) {
    cached = pathIcon(path).catch(() => null)
    iconPromises.set(key, cached)
    cached.then(icon => resolvedIcons.set(key, icon))
  }
  return cached
}

function AppIcon({ path, muted, selected }: { path: string | null; muted: boolean; selected: boolean }) {
  const [icon, setIcon] = useState<string | null>(() => (path ? (resolvedIcons.get(path.toLowerCase()) ?? null) : null))

  useEffect(() => {
    if (!path) {
      setIcon(null)
      return
    }
    const known = resolvedIcons.get(path.toLowerCase())
    setIcon(known ?? null)
    if (known !== undefined) return
    let disposed = false
    loadAppIcon(path).then(value => {
      if (!disposed) setIcon(value)
    })
    return () => {
      disposed = true
    }
  }, [path])

  return (
    <div
      style={{
        width: 30,
        height: 30,
        borderRadius: 7,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        flexShrink: 0,
        background: selected ? 'transparent' : 'var(--bg-tab)',
        color: selected ? 'var(--row-selected-fg)' : 'var(--text-dim)',
        opacity: muted ? 0.55 : 1,
        overflow: 'hidden',
      }}
    >
      {icon ? (
        <img src={icon} width={24} height={24} style={{ objectFit: 'contain' }} />
      ) : (
        <div
          dangerouslySetInnerHTML={{
            __html: getIconSvg(muted ? 'volume-off' : 'volume', 'currentColor', 17) ?? '',
          }}
          style={{ display: 'flex' }}
        />
      )}
    </div>
  )
}

export function clampMixerVolume(value: number): number {
  return Math.min(1, Math.max(0, value))
}

export function nextMixerIndex(current: number, count: number, direction: -1 | 1): number {
  if (count <= 0) return 0
  return Math.min(count - 1, Math.max(0, current + direction))
}

export default function VolumeMixer({ onError }: VolumeMixerProps) {
  const [sessions, setSessions] = useState<AudioSession[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const rootRef = useRef<HTMLDivElement>(null)
  const sessionsRef = useRef(sessions)
  const selectedIdRef = useRef(selectedId)
  const rowRefs = useRef(new Map<string, HTMLDivElement>())
  const writeChains = useRef(new Map<string, Promise<void>>())
  const pendingWrites = useRef(new Set<string>())
  sessionsRef.current = sessions
  selectedIdRef.current = selectedId

  const updateSession = useCallback((id: string, patch: Partial<AudioSession>) => {
    const next = sessionsRef.current.map(session => (session.id === id ? { ...session, ...patch } : session))
    sessionsRef.current = next
    setSessions(next)
  }, [])

  const refresh = useCallback(
    async (initial = false) => {
      try {
        const incoming = await listAudioSessions()
        const previousById = new Map(sessionsRef.current.map(session => [session.id, session]))
        const next = incoming.map(session => {
          const previous = previousById.get(session.id)
          return previous && pendingWrites.current.has(session.id)
            ? { ...session, volume: previous.volume, muted: previous.muted }
            : session
        })
        sessionsRef.current = next
        setSessions(next)
        const currentId = selectedIdRef.current
        if (!currentId || !next.some(session => session.id === currentId)) {
          const firstId = next[0]?.id ?? null
          selectedIdRef.current = firstId
          setSelectedId(firstId)
        }
        onError(null)
      } catch (error) {
        onError(String(error))
      } finally {
        if (initial) setLoading(false)
      }
    },
    [onError],
  )

  useEffect(() => {
    rootRef.current?.focus()
    void refresh(true)
    const timer = window.setInterval(() => void refresh(), 2000)
    return () => window.clearInterval(timer)
  }, [refresh])

  useEffect(() => {
    if (selectedId) rowRefs.current.get(selectedId)?.scrollIntoView({ block: 'nearest' })
  }, [selectedId])

  const queueWrite = useCallback(
    (id: string, write: () => Promise<void>) => {
      pendingWrites.current.add(id)
      const previous = writeChains.current.get(id) ?? Promise.resolve()
      const current = previous
        .catch(() => undefined)
        .then(write)
        .catch(error => {
          onError(String(error))
          void refresh()
        })
        .finally(() => {
          if (writeChains.current.get(id) === current) {
            writeChains.current.delete(id)
            pendingWrites.current.delete(id)
          }
        })
      writeChains.current.set(id, current)
    },
    [onError, refresh],
  )

  const setSessionVolume = useCallback(
    (session: AudioSession, value: number) => {
      const volume = clampMixerVolume(value)
      if (volume === session.volume) return
      updateSession(session.id, { volume })
      queueWrite(session.id, () => setAudioSessionVolume(session.id, volume))
    },
    [queueWrite, updateSession],
  )

  const toggleSessionMute = useCallback(
    (session: AudioSession) => {
      updateSession(session.id, { muted: !session.muted })
      queueWrite(session.id, async () => {
        const muted = await toggleAudioSessionMute(session.id)
        updateSession(session.id, { muted })
      })
    },
    [queueWrite, updateSession],
  )

  const handleKeyDown = (event: React.KeyboardEvent) => {
    const currentSessions = sessionsRef.current
    const currentIndex = Math.max(
      0,
      currentSessions.findIndex(session => session.id === selectedIdRef.current),
    )
    const selected = currentSessions[currentIndex]

    if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      event.preventDefault()
      event.stopPropagation()
      const index = nextMixerIndex(currentIndex, currentSessions.length, event.key === 'ArrowUp' ? -1 : 1)
      const id = currentSessions[index]?.id ?? null
      selectedIdRef.current = id
      setSelectedId(id)
      return
    }
    if ((event.key === 'ArrowLeft' || event.key === 'ArrowRight') && selected) {
      event.preventDefault()
      event.stopPropagation()
      const delta = (event.shiftKey ? 0.1 : 0.02) * (event.key === 'ArrowRight' ? 1 : -1)
      setSessionVolume(selected, selected.volume + delta)
      return
    }
    if ((event.key === ' ' || event.key === 'Enter') && selected) {
      event.preventDefault()
      event.stopPropagation()
      toggleSessionMute(selected)
    }
  }

  return (
    <div ref={rootRef} tabIndex={0} onKeyDown={handleKeyDown} style={{ outline: 'none', minHeight: 0 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '12px 14px 10px',
          borderBottom: '1px solid var(--divider)',
          background: 'linear-gradient(180deg, var(--bg-tab), transparent)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <div
            style={{
              width: 32,
              height: 32,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              borderRadius: 9,
              color: 'var(--accent)',
              background: 'var(--bg-select)',
            }}
            dangerouslySetInnerHTML={{ __html: getIconSvg('volume', 'currentColor', 18) ?? '' }}
          />
          <div>
            <div style={{ fontSize: 15, fontWeight: 600, color: 'var(--text)', lineHeight: 1.25 }}>Volume Mixer</div>
            <div style={{ fontSize: 10, color: 'var(--text-dim)', marginTop: 2 }}>
              ↑↓ select · ←→ adjust · Shift for 10% · Space mute
            </div>
          </div>
        </div>
        <span
          style={{
            fontSize: 10,
            color: 'var(--text-dim)',
            padding: '2px 7px',
            borderRadius: 999,
            border: '1px solid var(--border)',
            background: 'var(--bg-tab)',
          }}
        >
          {loading ? 'Loading…' : `${sessions.length} ${sessions.length === 1 ? 'app' : 'apps'}`}
        </span>
      </div>

      <div
        style={{
          maxHeight: 350,
          overflowY: 'auto',
          padding: '7px 8px',
          scrollbarWidth: 'thin',
          scrollbarColor: 'var(--border-strong) transparent',
        }}
      >
        {!loading && sessions.length === 0 && (
          <div style={{ padding: '28px 18px 32px', textAlign: 'center' }}>
            <div style={{ color: 'var(--text)', fontSize: 13 }}>No application audio sessions</div>
            <div style={{ color: 'var(--text-dim)', fontSize: 11, marginTop: 5 }}>
              Start playback in an app and it will appear here automatically.
            </div>
          </div>
        )}

        {sessions.map((session, index) => {
          const selected = session.id === selectedId
          const percent = Math.round(session.volume * 100)
          const fill = session.muted ? 0 : percent
          return (
            <div
              key={session.id}
              ref={element => {
                if (element) rowRefs.current.set(session.id, element)
                else rowRefs.current.delete(session.id)
              }}
              data-mixer-session={session.id}
              data-list-index={index}
              data-selected={selected || undefined}
              onClick={() => {
                selectedIdRef.current = session.id
                setSelectedId(session.id)
                rootRef.current?.focus()
              }}
              style={{
                display: 'grid',
                gridTemplateColumns: '30px minmax(105px, 0.8fr) minmax(150px, 1.4fr) 42px 28px',
                alignItems: 'center',
                gap: 10,
                minHeight: 52,
                padding: '7px 9px',
                borderRadius: 'var(--row-radius)',
                background: selected ? 'var(--row-selected-bg)' : 'transparent',
                boxShadow: selected ? 'var(--row-selected-shadow)' : 'none',
                transition: 'var(--row-transition)',
                cursor: 'default',
                userSelect: 'none',
              }}
            >
              <AppIcon path={session.exe_path} muted={session.muted} selected={selected} />
              <div style={{ minWidth: 0 }}>
                <div
                  style={{
                    color: selected ? 'var(--row-selected-fg)' : 'var(--text)',
                    fontSize: 12,
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                >
                  {session.name}
                </div>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 5,
                    color: selected
                      ? 'var(--row-selected-sublabel-fg)'
                      : session.active
                        ? 'var(--row-active-indicator-bg)'
                        : 'var(--text-dim)',
                    fontSize: 9,
                    marginTop: 3,
                  }}
                >
                  <span
                    style={{
                      width: 5,
                      height: 5,
                      borderRadius: '50%',
                      background: 'currentColor',
                      boxShadow: session.active && !selected ? '0 0 4px currentColor' : 'none',
                    }}
                  />
                  {session.active ? 'Playing' : 'Ready'}
                </div>
              </div>
              <div
                role="slider"
                aria-label={`${session.name} volume`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={percent}
                onPointerDown={event => {
                  const rect = event.currentTarget.getBoundingClientRect()
                  selectedIdRef.current = session.id
                  setSelectedId(session.id)
                  setSessionVolume(session, (event.clientX - rect.left) / rect.width)
                  rootRef.current?.focus()
                }}
                style={{
                  position: 'relative',
                  height: 20,
                  display: 'flex',
                  alignItems: 'center',
                  cursor: 'pointer',
                }}
              >
                <div
                  style={{
                    width: '100%',
                    height: 4,
                    borderRadius: 999,
                    background: selected ? 'var(--row-selected-sublabel-fg)' : 'var(--border-strong)',
                    opacity: selected ? 0.55 : 1,
                  }}
                >
                  <div
                    style={{
                      width: `${fill}%`,
                      height: '100%',
                      borderRadius: 999,
                      background: session.muted
                        ? selected
                          ? 'var(--row-selected-sublabel-fg)'
                          : 'var(--text-dim)'
                        : selected
                          ? 'var(--row-selected-fg)'
                          : 'var(--accent)',
                      transition: 'width 80ms ease-out',
                    }}
                  />
                </div>
                <div
                  style={{
                    position: 'absolute',
                    left: `calc(${fill}% - 5px)`,
                    width: 10,
                    height: 10,
                    borderRadius: '50%',
                    background: session.muted
                      ? selected
                        ? 'var(--row-selected-sublabel-fg)'
                        : 'var(--text-dim)'
                      : selected
                        ? 'var(--row-selected-fg)'
                        : 'var(--accent)',
                    border: `2px solid ${selected ? 'var(--row-selected-bg)' : 'var(--bg)'}`,
                    boxShadow: '0 1px 4px rgba(0,0,0,0.3)',
                    transition: 'left 80ms ease-out',
                  }}
                />
              </div>
              <span
                style={{
                  textAlign: 'right',
                  color: selected
                    ? session.muted
                      ? 'var(--row-selected-sublabel-fg)'
                      : 'var(--row-selected-fg)'
                    : session.muted
                      ? 'var(--text-dim)'
                      : 'var(--text)',
                  fontFamily: 'var(--font-ui)',
                  fontSize: 10,
                }}
              >
                {percent}%
              </span>
              <button
                type="button"
                aria-label={session.muted ? `Unmute ${session.name}` : `Mute ${session.name}`}
                title={session.muted ? 'Unmute (Space)' : 'Mute (Space)'}
                onClick={event => {
                  event.stopPropagation()
                  selectedIdRef.current = session.id
                  setSelectedId(session.id)
                  toggleSessionMute(session)
                  rootRef.current?.focus()
                }}
                style={{
                  width: 26,
                  height: 26,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  border: 'none',
                  borderRadius: 6,
                  background: session.muted
                    ? selected
                      ? 'rgba(255,255,255,0.12)'
                      : 'var(--bg-select)'
                    : 'transparent',
                  color: session.muted
                    ? selected
                      ? 'var(--row-selected-fg)'
                      : '#f7768e'
                    : selected
                      ? 'var(--row-selected-sublabel-fg)'
                      : 'var(--text-dim)',
                  cursor: 'pointer',
                  outline: 'none',
                }}
                dangerouslySetInnerHTML={{
                  __html: getIconSvg(session.muted ? 'volume-off' : 'volume', 'currentColor', 14) ?? '',
                }}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}
