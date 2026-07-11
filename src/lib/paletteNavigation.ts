// Navigation glue for the command palette.
//
// Applying a StepResult (returned by a step's onSelect / onCommitQuery /
// onSubmit) to the reducer + window is the same dance in three places; this
// keeps them identical.

import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Dispatch } from 'react'
import type { PaletteAction, StepResult } from '../types'

// Drive the navigation stack from a StepResult:
//   done    → reset to root and hide the window
//   push    → push the returned step
//   replace → replace the top step (preserveSelectionOnReplace keeps the query
//             and highlighted row, used when a step replaces itself by the same id)
//   pop     → pop back one level
//   stay    → no-op (the caller already handled it, e.g. copy-and-stay)
// May throw from the window hide; callers wrap this in their SET_ERROR try/catch
// exactly as before.
export async function applyStepResult(
  dispatch: Dispatch<PaletteAction>,
  result: StepResult,
  opts: { preserveSelectionOnReplace?: boolean } = {},
): Promise<void> {
  switch (result.type) {
    case 'done':
      dispatch({ type: 'RESET' })
      await getCurrentWindow().hide()
      break
    case 'push':
      dispatch({ type: 'PUSH_STEP', step: result.step })
      break
    case 'replace':
      dispatch({ type: 'REPLACE_STEP', step: result.step, preserveSelection: opts.preserveSelectionOnReplace })
      break
    case 'pop':
      dispatch({ type: 'POP_STEP' })
      break
    case 'stay':
      break
  }
}
