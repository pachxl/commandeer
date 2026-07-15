// Transient user feedback for the command palette: toasts, the HUD pill, and
// the confirmation dialog.
//
// Extracted from Palette.tsx. These are also registered on appEvents so
// commands defined outside the component tree can raise them. The reset helper
// stays in Palette (it needs the reducer's dispatch directly).

import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { appEvents } from '../lib/appEvents'
import { isConfirmSuppressed, suppressConfirm, type ConfirmOptions } from '../lib/confirm'
import type { ToastKind, ToastMessage } from '../components/Toast'
import type { PaletteAction } from '../types'

interface ConfirmReq {
  options: ConfirmOptions
  resolve: (ok: boolean) => void
}

export interface PaletteFeedback {
  toast: (message: string, kind?: ToastKind) => void
  toasts: ToastMessage[]
  // A Raycast-style confirmation shown instead of the palette body after an
  // action (then the window hides). Null when nothing is showing.
  hud: { message: string; icon?: string } | null
  showHud: (message: string, icon?: string) => void
  // Ask for confirmation; resolves true if confirmed (or a remembered key).
  requestConfirm: (options: ConfirmOptions) => Promise<boolean>
  resolveConfirm: (ok: boolean) => void
  confirmReq: ConfirmReq | null
  confirmRemember: boolean
  setConfirmRemember: Dispatch<SetStateAction<boolean>>
  confirmFocus: 'confirm' | 'cancel'
  setConfirmFocus: Dispatch<SetStateAction<'confirm' | 'cancel'>>
  // Cancel all feedback that can outlive the current palette session.
  resetFeedback: () => void
}

export function usePaletteFeedback(dispatch: Dispatch<PaletteAction>): PaletteFeedback {
  const [toasts, setToasts] = useState<ToastMessage[]>([])
  // HUD: a Raycast-style confirmation shown *instead of* the palette body after
  // an action, kept up briefly while the window is still visible, then the
  // window hides. Fixes the old "toast fires then the window hides immediately,
  // so you never see it" gap for copy/paste-style actions.
  const [hud, setHud] = useState<{ message: string; icon?: string } | null>(null)
  // Pending confirmation prompt (see ConfirmOverlay + appEvents.confirm). The
  // resolve fn settles the promise the requesting action is awaiting.
  const [confirmReq, setConfirmReq] = useState<ConfirmReq | null>(null)
  const [confirmRemember, setConfirmRemember] = useState(false)
  const [confirmFocus, setConfirmFocus] = useState<'confirm' | 'cancel'>('confirm')
  // Refs mirror the confirm state so resolveConfirm can read the latest values
  // without nesting setState updaters.
  const confirmReqRef = useRef<ConfirmReq | null>(null)
  const confirmRememberRef = useRef(false)
  confirmRememberRef.current = confirmRemember
  const toastIdRef = useRef(0)
  const hudTimerRef = useRef<number | null>(null)

  const toast = useCallback((message: string, kind: ToastKind = 'info') => {
    const id = ++toastIdRef.current
    setToasts(prev => [...prev, { id, message, kind }])
    window.setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id))
    }, 2000)
  }, [])

  // Show the HUD, then dismiss the launcher once it's been seen. Replaces the
  // action body so it reads as a single floating confirmation pill.
  const showHud = useCallback(
    (message: string, icon?: string) => {
      if (hudTimerRef.current !== null) window.clearTimeout(hudTimerRef.current)
      setHud({ message, icon })
      hudTimerRef.current = window.setTimeout(async () => {
        hudTimerRef.current = null
        setHud(null)
        dispatch({ type: 'RESET' })
        await getCurrentWindow().hide()
      }, 1000)
    },
    [dispatch],
  )

  // Ask for confirmation. A remembered key resolves immediately; otherwise the
  // returned promise settles when the user answers via ConfirmOverlay.
  const requestConfirm = useCallback((options: ConfirmOptions): Promise<boolean> => {
    if (options.key && isConfirmSuppressed(options.key)) return Promise.resolve(true)
    setConfirmRemember(false)
    setConfirmFocus('confirm')
    return new Promise<boolean>(resolve => {
      // Only one confirmation can own the palette. Superseding a request must
      // settle the old action instead of leaving it suspended forever.
      confirmReqRef.current?.resolve(false)
      const req = { options, resolve }
      confirmReqRef.current = req
      setConfirmReq(req)
    })
  }, [])

  // Settle the pending confirm; persist "Don't ask again" only on a positive,
  // remembered answer. Reads latest values from refs to avoid nested setState.
  const resolveConfirm = useCallback((ok: boolean) => {
    const req = confirmReqRef.current
    if (!req) return
    if (ok && confirmRememberRef.current && req.options.key) suppressConfirm(req.options.key)
    req.resolve(ok)
    confirmReqRef.current = null
    setConfirmReq(null)
    setConfirmRemember(false)
  }, [])

  const resetFeedback = useCallback(() => {
    if (hudTimerRef.current !== null) {
      window.clearTimeout(hudTimerRef.current)
      hudTimerRef.current = null
    }
    setHud(null)
    const req = confirmReqRef.current
    confirmReqRef.current = null
    if (req) req.resolve(false)
    setConfirmReq(null)
    setConfirmRemember(false)
    setConfirmFocus('confirm')
  }, [])

  // Expose the toast/HUD/confirm helpers app-wide (the reset helper is
  // registered separately in Palette — it needs dispatch directly).
  useEffect(() => {
    appEvents.toast = toast
    appEvents.showHud = showHud
    appEvents.confirm = requestConfirm
    return () => {
      appEvents.toast = undefined
      appEvents.showHud = undefined
      appEvents.confirm = undefined
    }
  }, [toast, showHud, requestConfirm])

  useEffect(() => resetFeedback, [resetFeedback])

  return {
    toast,
    toasts,
    hud,
    showHud,
    requestConfirm,
    resolveConfirm,
    confirmReq,
    confirmRemember,
    setConfirmRemember,
    confirmFocus,
    setConfirmFocus,
    resetFeedback,
  }
}
