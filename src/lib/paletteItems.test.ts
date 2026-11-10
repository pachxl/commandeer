import { describe, expect, it } from 'vitest'
import type { Command } from '../types'
import { canToggleShowAtRoot, isShownAtRoot } from './paletteItems'

const cmd = (patch: Partial<Command>): Command => ({ id: 'x', label: 'X', icon: 'script', ...patch })

describe('Show in Root', () => {
  it('surfaces search-only commands once the user pins them to root', () => {
    // Regression: searchOnly commands (e.g. "Switch XG27AQDMG to HDMI 1")
    // ignored the override and never appeared on the main page.
    const monitorSwitch = cmd({ folderName: 'System', searchOnly: true })
    expect(isShownAtRoot(monitorSwitch)).toBe(false)
    expect(isShownAtRoot(monitorSwitch, { showAtRoot: true })).toBe(true)
    expect(isShownAtRoot(cmd({ searchOnly: true }), { showAtRoot: true })).toBe(true)
  })

  it('surfaces folder-tagged commands only when shown at root', () => {
    const app = cmd({ folderName: 'Apps' })
    expect(isShownAtRoot(app)).toBe(false)
    expect(isShownAtRoot(app, { showAtRoot: true })).toBe(true)
  })

  it('only offers the toggle where it changes something', () => {
    expect(canToggleShowAtRoot(cmd({ folderName: 'Tools' }))).toBe(true)
    expect(canToggleShowAtRoot(cmd({ searchOnly: true }))).toBe(true)
    // Always on the main page already: root scripts and folder rows.
    expect(canToggleShowAtRoot(cmd({ source: 'script' }))).toBe(false)
    expect(canToggleShowAtRoot(cmd({ isFolder: true }))).toBe(false)
    expect(isShownAtRoot(cmd({ source: 'script' }))).toBe(true)
  })
})
