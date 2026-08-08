# Configuration and settings

Durable configuration is JSON owned by Rust. The frontend reads it at startup,
mutates a shared `AppConfig` while Settings is open, and writes it back through
`src/lib/tauri.ts`. `commands/config.rs` is lenient during startup: a missing or
invalid file produces defaults rather than preventing the palette from opening.

## `config.json`

The file is `<app-data>/config.json`; use the Settings command that opens the
data directory or the `data_dir` IPC command rather than hard-coding an OS path.

| Field                 | Default / range                                                       | Meaning                                                                    |
| --------------------- | --------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `scripts_dir`         | discovered `scripts` directory, otherwise `<home>/commandeer/scripts` | Directory scanned for executable commands                                  |
| `search_paths`        | platform defaults when absent                                         | Roots considered by global file search                                     |
| `theme`               | `Tokyo Night`                                                         | Built-in or user theme name; legacy `dark`/`light` names still resolve     |
| `transparency`        | `0.0` (opaque) to `1.0` (transparent)                                 | Window transparency; native Windows/macOS alpha is applied where supported |
| `global_hotkey`       | `Ctrl+Space`; macOS `Cmd+Shift+Space`                                 | Main palette toggle                                                        |
| `global_hotkey_game`  | `Alt+Space`                                                           | Toggle used while Game Mode is enabled                                     |
| `screenshot_hotkey`   | Windows `Insert`; macOS empty; Linux compositor-managed               | Region screenshot trigger; hidden from Linux Settings                      |
| `window_drag`         | `false`                                                               | Alt-drag move/resize on Windows/macOS                                      |
| `per_monitor_alt_tab` | `false`                                                               | Windows monitor-local Alt+Tab replacement                                  |
| `palette_scale`       | `1.0`; UI maps 0–100% to 0.5–1.5                                      | Whole-palette CSS/window scale                                             |
| `ui_style`            | `Default`                                                             | Presentation system: `Default` or Black Water `Onix`                       |
| `auto_update`         | `true` when absent                                                    | Release-only signed background updater                                     |

Optional fields use `serde(default)` and `skip_serializing_if` where appropriate,
so older config files continue to round-trip without gaining meaningless nulls.

## Settings behavior

`src/commands/settings.ts` owns the Settings step and live previews. Theme and
style changes preview while highlighted and restore the saved selection on exit.
The applied-style event updates Onix's frontend shell and native material during
the preview; do not key style-sensitive React behavior only from the persisted
`config.ui_style`, which may not rerender while a preview is highlighted.
Transparency and scale apply on every slider tick, but persistence is debounced
and serialized; the trailing value is flushed when leaving the step. Do not
replace this with independent fire-and-forget whole-config writes, because an
older write can finish after a newer one and roll the setting back.

Shortcut changes are validated and registered by Rust. On Linux, the managed
COSMIC/GNOME shortcut path is separate from the global-shortcut plugin path.
Game Mode updates both the effective registered shortcut and the Linux desktop
binding when applicable. Live shortcut updates are transactional: a proposed
binding is registered before it is persisted, and an OS collision leaves the
previous registration and stored value intact. Per-command bindings are
validated as a complete, duplicate-free set and rolled back together if any
registration fails.

The Scripts Directory setting accepts a full absolute path and reloads the root
command list immediately after it is saved. The adjacent Open Scripts Folder
action opens the current location in the system file manager.

File Search Roots accepts one absolute directory per line. Blank lines and
duplicate paths are removed before saving; relative paths and shell-dependent
`~` expansion are rejected. The index manager reads its roots once during app
startup, so saving custom roots or resetting to the platform defaults requires
a restart before the background scan and watcher use the new set. The Settings
UI keeps this restart requirement visible rather than implying a live re-index.

## User themes and styles

Built-in themes and structural styles live in `src/lib/themes.ts` and
`src/lib/styles.ts`. User themes are JSON files in `<app-data>/themes/` with a
`name` and CSS variable map, read by `commands/store.rs`. Default keeps the
traditional division in which the theme controls color and the style controls
layout, spacing, fonts, and component treatment.

Onix deliberately adds one narrow exception: its Black Water shell owns a
dark-neutral material and readable foreground so it remains a coherent dark
glass design even when a light theme is selected. The active theme still owns
the accent and environmental tint used by the optical rim, selection lens, and
semantic states. Do not substitute a theme's light background directly into the
Onix shell or override the theme accent with a fixed blue.

OS accessibility settings can further override presentation without changing
`config.json`: reduced motion freezes pointer-responsive optics and removes
shape/lens travel; reduced transparency selects an opaque dark CSS material;
forced-colors uses system colors. These are runtime policies, not durable app
preferences. The existing `transparency` field still controls whole-window
alpha, including content, after the material has been selected.

## Safe config changes

For a new setting:

1. Add the serialized field in `commands/config.rs` and the matching TypeScript
   field in `src/types.ts`.
2. Define a default in Rust and a matching display fallback in Settings.
3. Decide whether it is durable config or webview-local UI state.
4. Apply it at startup if native state must exist before the webview is ready.
5. Add platform gating and an unsupported explanation.
6. Add or update tests for serde round-tripping and the Settings interaction.
7. Update this table, [`storage.md`](storage.md), and the relevant platform page.

## Keeping this document current

Update this page whenever `AppConfig`, Settings labels, defaults, slider mapping,
theme/style loading, Onix material policy, shortcut behavior, or config
migration changes. Verify against `src/types.ts`, `src/commands/settings.ts`, and
`src-tauri/src/commands/config.rs`; check that `AGENTS.md` and
[`platforms.md`](platforms.md) still describe the same defaults.
