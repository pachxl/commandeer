// "Don't ask again" persistence for confirmation prompts. A confirm request
// carries a stable `key`; once the user ticks "Don't ask again" for that key it
// is remembered here (webview localStorage) and future requests with the same
// key resolve immediately without showing the dialog. See Palette's confirm
// overlay and appEvents.confirm.

const STORAGE_KEY = 'commandeer:confirm-suppressed'
const SAVE_DEBOUNCE_MS = 50

let pendingSet: Set<string> | null = null
let saveTimeout: ReturnType<typeof setTimeout> | null = null

function load(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return new Set(raw ? (JSON.parse(raw) as string[]) : [])
  } catch {
    return new Set()
  }
}

function doSave(set: Set<string>): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify([...set]))
}

function save(set: Set<string>): void {
  pendingSet = set
  if (saveTimeout) clearTimeout(saveTimeout)
  saveTimeout = setTimeout(() => {
    if (pendingSet !== null) {
      doSave(pendingSet)
      pendingSet = null
    }
    saveTimeout = null
  }, SAVE_DEBOUNCE_MS)
}

export function isConfirmSuppressed(key: string): boolean {
  return load().has(key)
}

export function suppressConfirm(key: string): void {
  const set = load()
  set.add(key)
  // Merge with pending set if there's a debounced save in flight
  if (pendingSet !== null) {
    pendingSet.add(key)
  }
  save(set)
}

export interface ConfirmOptions {
  // Stable id for "Don't ask again"; omit to make the prompt non-rememberable
  key?: string
  message: string
  detail?: string
  confirmLabel?: string
  cancelLabel?: string
  // Style the confirm button as destructive (red)
  danger?: boolean
}
