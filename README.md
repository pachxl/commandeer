# Commandeer

Commandeer is a cross-platform desktop command palette and utility suite built
with Tauri 2, React, TypeScript, and Rust. It provides a keyboard-first launcher
for applications, scripts, files, system actions, clipboard history, media
controls, notes, links, calculations, screenshots, and platform-specific window
management.

The project is organized around platform parity: features are implemented on
Windows, Linux/Wayland, and macOS where the operating system allows them, and
explicitly hidden or documented as unsupported where it does not. The codebase
is active; the [maintainer documentation](docs/README.md) is the best starting
point for understanding its moving parts.

## Contributing

Before changing a feature, read the relevant page in [`docs/`](docs/README.md),
then update that page when behavior, platform support, configuration, storage,
or verification steps change.

Use `bun install` after pulling dependency changes. The normal checks are
`npm run build`, `npm test`, `npm run lint`, `cargo test` from `src-tauri/`, and
`cargo clippy --all-targets -- -D warnings`. The complete release and restart
workflow is documented in [`RELEASING.md`](RELEASING.md) and the shared agent
instructions in [`AGENTS.md`](AGENTS.md).

## Highlights

- Launch installed applications and user scripts.
- Search files locally with a self-hosted SQLite/FTS5 index.
- Search browser bookmarks, notes, quick links, and clipboard history.
- Run calculations and time-zone conversions directly from the query field.
- Control volume, inspect or kill processes, and run system actions.
- Capture, crop, annotate, save, and copy screenshots.
- Customize themes, styles, scale, transparency, shortcuts, and optional status panels.
- Use list, grid, form, input, and slider flows without leaving the keyboard.

Clipboard history is encrypted at rest on every supported platform.

## Platform notes

| Platform | Palette shortcut                  | Notable behavior                                                                                                                                                          |
| -------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows  | Configurable                      | Full Alt-drag window moving/resizing and snapping; screenshot hotkey defaults to `Insert`.                                                                                |
| Linux    | `Ctrl+Space` by default on COSMIC | Uses a Wayland layer-shell palette. Re-launching the binary is a reliable palette toggle when global X11-style grabs are unavailable. Alt-drag is left to the compositor. |
| macOS    | `Cmd+Shift+Space` by default      | Runs as an Accessory app. Screenshot and paste features require Screen Recording and Accessibility permission respectively.                                               |

macOS system actions and Finder-aware search may also cause one-time Automation
permission prompts. Some platform-specific behavior can only be fully verified
on that operating system.

## Using Commandeer

Open the palette with the platform shortcut or tray icon, type to filter commands,
and press Enter to run the selected result. Common controls include:

- Arrow keys to move through results; Enter to select.
- Escape to cancel the current confirmation, close the action panel, move back one step, or dismiss the root palette.
- `Ctrl+K` to open actions for the current result.
- `@search` to search the active folder and `@find` for indexed global file search.

The scripts directory is configurable in Settings. Supported entries are
platform-specific:

- Windows: PowerShell scripts and shortcuts.
- Linux: shell scripts, desktop entries, AppImages, and executables.
- macOS: shell/command scripts and executables.

The default palette shortcut is `Ctrl+Space` except on macOS, where it is
`Cmd+Shift+Space`. Game Mode can switch to `Alt+Space`. Windows uses `Insert`
as the default screenshot shortcut; macOS has no default screenshot shortcut,
and Linux uses its compositor-managed binding. All configurable shortcuts are
available from Settings.

## Screenshot tool

The screenshot command freezes the current display, lets you drag a region, and
then opens an annotation stage. Draw freehand marker strokes, undo with `Ctrl+Z`
or Backspace, finish with Enter, or cancel with Escape. The resulting PNG is
saved under `~/Pictures/Screenshots` and copied to the clipboard.

Holding Alt (Option on macOS) shows the raw frame color beneath the pointer;
Alt-click copies its hex value while still saving the crop.

## Keeping this documentation current

Update this README when the product’s user-facing capabilities, default
shortcuts, supported platforms, setup commands, or top-level navigation change.
Keep implementation detail in [`docs/`](docs/README.md) and agent/build policy
in [`AGENTS.md`](AGENTS.md); do not let this overview become a second
architecture specification.
