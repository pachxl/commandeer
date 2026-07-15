# Commandeer

this was made with like 10 different models (gpt 5.6 sol, fable 5, opus 4.7 and 4.8, kimi 2.6 and 2.7, deepseek-v4-pro *maybe*). we have not looked at a single line of code nor do we understand how it works at all which is always good.

basically its a command palette originally made for windows, with some support for linux, and even less support for mac (just use spotlight). it has some cool window manager stuff for windows that lets you hold alt and right/left click to navigate. it has a weird alt+tab replacement for windows which i think sucks but i still use it

press ctrl+space to open it, or if "game mode" is enabled press alt+space (this was because i couldnt crouch and jump on cs)


oh it also has a quick access "commands" folder where you can put shortcuts and scripts to run directly from here, and you can add icons to them by just putting in a png with the same name as the script or shortcut.

and it has a screenshot tool but i forgot what the default key is (most of the keybinds are configurable through the settings page)

the rest of this readme is written by sol so whatever

The project is under active development. Platform integrations differ where the operating systems require it, especially for global shortcuts, window management, screenshots, and application discovery.

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

| Platform | Palette shortcut | Notable behavior |
| --- | --- | --- |
| Windows | Configurable | Full Alt-drag window moving/resizing and snapping; screenshot hotkey defaults to `Insert`. |
| Linux | `Ctrl+Space` by default on COSMIC | Uses a Wayland layer-shell palette. Re-launching the binary is a reliable palette toggle when global X11-style grabs are unavailable. Alt-drag is left to the compositor. |
| macOS | `Cmd+Shift+Space` by default | Runs as an Accessory app. Screenshot and paste features require Screen Recording and Accessibility permission respectively. |

macOS system actions and Finder-aware search may also cause one-time Automation permission prompts. Some platform-specific behavior can only be fully verified on that operating system.

## Using Commandeer

Open the palette with the platform shortcut or tray icon, type to filter commands, and press Enter to run the selected result. Common controls include:

- Arrow keys to move through results; Enter to select.
- Escape to cancel the current confirmation, close the action panel, move back one step, or dismiss the root palette.
- `Ctrl+K` to open actions for the current result.
- `@search` to search the active folder and `@find` for indexed global file search.

The scripts directory is configurable in Settings. Supported entries are platform-specific:

- Windows: PowerShell scripts and shortcuts.
- Linux: shell scripts, desktop entries, AppImages, and executables.
- macOS: shell/command scripts and executables.

## Screenshot tool

The screenshot command freezes the current display, lets you drag a region, and then opens an annotation stage. Draw freehand marker strokes, undo with `Ctrl+Z` or Backspace, finish with Enter, or cancel with Escape. The resulting PNG is saved under `~/Pictures/Screenshots` and copied to the clipboard.

Holding Alt (Option on macOS) shows the raw frame color beneath the pointer; Alt-click copies its hex value while still saving the crop.
