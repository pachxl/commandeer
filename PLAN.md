# Commandeer Plan

Commandeer is a two-person force-multiplier: the goal is speed-of-computer-use,
not a product with a userbase. Every item below is ranked by
(frequency of use × keystrokes saved) ÷ effort.

## Context

The current `testing` branch was rebuilt on top of `main`'s history. Features
from the old feature-rich branch — which lives as a full checkout at
`C:\Users\lunat\commandeer-legacy` — have been re-ported in phases:

- **Ported already:** provider architecture, weighted fuzzy + frecency ranking,
  calculator (units/currency/colors/timezones), clipboard history, file search,
  snippets, Tools folder, kill process, configurable + per-command hotkeys,
  tray, autostart, deep links, themes, overrides UI.
- **Not yet ported:** app launcher, window switcher/management, system power
  actions, volume control, quicklinks, bookmarks, favicons, fonts browser,
  the test suite.

Everything in the "not yet ported" list exists **fully written and previously
verified working** in `commandeer-legacy`. This plan is mostly port-and-adapt
work, not greenfield. When porting, expect drift: the current tree's provider
registry (`src/providers/index.ts`), `types.ts`, and `mod.rs`/`lib.rs`
invoke-handler wiring have evolved since legacy was written.

---

## Priority order

### 1. App launcher — ✅ DONE (2026-07-03)

**Built:** `src-tauri/src/commands/launcher.rs`, `src/providers/appLauncher.ts`

Implemented as an improvement over the legacy port rather than a straight
copy:

- **Shell AppsFolder enumeration** (COM `IEnumShellItems`) instead of walking
  Start-Menu `.lnk` files — picks up UWP/Store apps too, with localized
  display names and shell-side dedup. Start-Menu walkdir kept as a fallback.
- **`ShellExecuteW` launching** (handles `shell:AppsFolder\<id>` and `.lnk`
  alike) instead of legacy's PowerShell `Start-Process` spawn (~200 ms/launch).
- **Lazy icons**: entries carry their parsing path as `iconPath`; the existing
  per-visible-row `path_icon` mechanism resolves them (new
  `icon_for_shell_item` via `IShellItemImageFactory` in `icons.rs`), so
  `list_apps` does no icon work at all.
- **TTL + localStorage cache** in the provider (refresh() fires on every
  palette show); background refetch calls `appEvents.refreshCommands()` only
  when the list changed.
- Apps are static commands under an **Apps virtual folder** (hidden from root
  browse, flat-searchable) so they rank through the same fuzzy + frecency
  pipeline as everything else — no separate ranking path like legacy had.

### 2. System actions + volume — ✅ DONE (2026-07-03)

**Built:** `src-tauri/src/commands/system.rs`, `src-tauri/src/commands/audio.rs`,
`src/providers/system.ts`, `src/providers/volume.ts`

Implemented as an improvement over the legacy port (same pattern as the app
launcher):

- **Direct Win32 instead of PowerShell spawns** — legacy shelled out to
  `powershell.exe` per action; now `LockWorkStation`, `SetSuspendState`,
  `ExitWindowsEx` (with the `SeShutdownPrivilege` enable dance), and
  `SHEmptyRecycleBinW`. Legacy's `powercfg -hibernate` toggle needed admin
  and silently failed; the direct API doesn't need it.
- **In-palette confirm step** for destructive actions (restart / shutdown /
  logout / empty trash) — Confirm is preselected so the fast path stays
  Enter-Enter, and Esc cancels.
- **System virtual folder** — all nine commands live under one `System`
  folder row at root (same mechanism as Apps/Tools: hidden from browse,
  still flat-searchable), instead of cluttering the main display.
- **Per-device volume sliders** — `list_audio_devices` enumerates active
  render endpoints (friendly names via `PKEY_Device_FriendlyName`, default
  first); Set Volume lists devices, each opening a live slider. Volume
  get/set/mute all take an optional endpoint id. Atomic `toggle_mute`
  (one IPC, no get/set race) instead of legacy's two round-trips.
- **Slider-step UX fixes** (benefits transparency too): the palette now
  actually calls `loadSliderValue` to seed position (was hardcoded to the
  transparency special case; other sliders started at min), Left/Right and
  Up/Down arrows nudge the value, Enter confirms and pops back.
- **Esc = back, not close** (palette-wide): Esc pops one step level at a
  time and only hides the launcher from the root screen.
- Typed `SystemAction` serde enum on the Rust side instead of string matching.

### 3. Window switcher + window management — *deferred for now (2026-07-03)*

**Port:** `src-tauri/src/commands/window_mgmt.rs`,
`src/providers/windowSwitcher.ts`, `src/providers/windowManagement.ts`

An alt-tab replacement with fuzzy search over window titles is arguably a
bigger daily multiplier than app launching — you switch windows far more often
than you launch apps. Snap / quarters / restore comes along in the same
backend (`EnumWindows`, `SetForegroundWindow`, `SetWindowPos`). Was #2, but
deliberately parked — pick up when ready for a meatier port; quicklinks (#4)
can go first.

### 4. Quicklinks + bookmarks (+ favicons)

**Port:** `src/providers/quicklinks.ts`, `src/providers/bookmarks.ts`,
`src-tauri/src/commands/bookmarks.rs`, `src-tauri/src/commands/favicon.rs`

Quicklinks with `{query}` args ("jira PROJ-123", "gh search foo") are a
genuine speed feature, not polish. Bookmarks ride along since the
fuzzy/frecency plumbing is shared. Favicons are small and make both usable at
a glance. Note: the legacy favicon fallback had a MIME-sniffing fix
(ICO/GIF/JPEG magic bytes vs. a hardcoded `data:image/png`) — make sure the
ported version includes it.

### 5. Test suite

**Port:** `src/lib/color.test.ts`, `frecency.test.ts`, `fuzzy.test.ts`,
`math.test.ts`, `overrides.test.ts` + vitest config (`bun add -d vitest`,
`test` script, `test` block in `vite.config.ts` with `environment: 'node'`)

~96 tests covering exactly the pure modules that already exist in the current
tree. Since code keeps moving between branches, this is cheap insurance and
mostly a copy job. Expect some assertions to need updating where `math.ts` /
`fuzzy.ts` have drifted.

### 6. Ranking polish (from Vicinae)

**Write:** small changes to result assembly in the palette

- **Alias-prefix hoisting** — an exact alias prefix match ranks above all
  scored results; shorter alias wins ties.
- **Stable sort** — avoid row flicker when scores tie across keystrokes.

Not a feature, but the kind of change that makes the whole launcher *feel*
faster. (Vicinae reference: `root-item-manager.cpp:133-204` in
`C:\Users\lunat\vicinae-ref`.)

---

## Opportunistic / mood-dependent

- **Fonts browser** (`fonts.rs` + `fonts.ts`) — fun, and a free port, but not
  a speed feature. Grab it when in the mood.
- **dmenu mode** — pipe stdin → palette → stdout selection. Neat for
  scripting, but GUI-subsystem + single-instance + console-attach on Windows
  make it fiddly to build right.
- **Emoji picker** — fully portable idea from Vicinae (static table + fuzzy +
  grid view) if the itch strikes.

## Explicitly skipped (and why)

- **Extension runtime / store / Raycast compat** — only pays off with a
  userbase. Out of scope for a two-person tool.
- **Replacing the calculator with Qalculate** — libqalculate on Windows is a
  build headache, and the hand-rolled `math.ts` already handles
  units/currency/dates/colors/timezones and is tested. Not worth it.
- **FZF v2 scoring port, file-search skeleton/spellfix pipeline** — real
  quality improvements but effort-heavy relative to perceived gain; the
  current matcher is already decent.
- **Browser tab switcher** — requires shipping a native-messaging browser
  extension per browser. Poor effort/payoff for two people.

---

*Supersedes `legacy-gap-document.md` and `vicinae-gap-close.md` (deleted
2026-07-03). Legacy source of truth for ports: `C:\Users\lunat\commandeer-legacy`.
Vicinae reference checkout: `C:\Users\lunat\vicinae-ref`.*
