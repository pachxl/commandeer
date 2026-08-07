# Architecture and runtime flow

Commandeer is a Tauri 2 desktop process with a React/TypeScript webview and a
Rust host. It keeps two windows alive for the lifetime of the process and hides
or shows them as needed:

```text
OS shortcut / tray / second launch / deep link
                  |
                  v
        Rust host (`src-tauri/src/lib.rs`)
          |                    |
          v                    v
   palette window          screenshot window
   React app               React overlay
          |
          v
   commands/providers -> `src/lib/tauri.ts` -> Tauri invoke -> Rust commands
```

## Process startup

`src-tauri/src/lib.rs` is the composition root. Setup performs the following
work in order-sensitive groups:

1. Register the single-instance handler, so a second launch toggles the running
   palette and can forward a `commandeer://` URL.
2. Create the tray, global shortcut handlers, deep-link support, and the two
   webview windows.
3. Migrate data from the old bundle identifier and clean up only pristine legacy
   starter scripts.
4. Create managed state for screenshot capture, the file index, and clipboard
   history; start their background services.
5. Configure icon caching and the gentle application-icon warmup.
6. Start the signed updater in release builds and apply persisted window-drag
   and Alt+Tab settings.

The exact order matters: foreground/folder snapshots must happen before the
palette is shown, and the screenshot flow must hide the palette before freezing
the display. The registration list at the bottom of `lib.rs` is the complete
frontend-to-Rust command boundary.

## Palette lifecycle

`src/App.tsx` assembles root commands from registered providers, settings,
scripts, and dynamic folders. `components/Palette.tsx` owns query input, fuzzy
ranking, selection, keyboard handling, step navigation, action panels, and
dismissal. A `Command` either runs an action or creates a `Step`; a `Step` loads
`PaletteItem` rows and returns a `StepResult` when the user selects something.

The normal interaction is:

1. Show the hidden palette and capture the previously focused window/folder.
2. Refresh cached and dynamic command sources.
3. Rank the root list or enter a step with `Right`/`Enter`.
4. Load step items, preserving only the newest request for the current step.
5. Run the selected action or push/replace/pop a step.
6. Resolve pending confirmation state, clear feedback, hide the window, and
   leave no action alive for the next session.

Escape is a state machine, not a generic close button: it cancels a pending
confirmation, closes the action panel, pops one step, or dismisses the root in
that order. External dismissal paths must follow the same cleanup rules.

## Boundaries and ownership

| Boundary              | Owner                                                               | Contract                                                                     |
| --------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Root command assembly | `src/App.tsx`, `src/providers/index.ts`                             | Providers return `Command[]`; scripts/settings remain legacy assembly paths. |
| Palette state         | `src/components/Palette.tsx`, `src/lib/paletteReducer.ts`           | Every selection index is valid for current items; async loads are sequenced. |
| Frontend IPC          | `src/lib/tauri.ts`                                                  | All Rust `invoke` calls and shared serialized types live here.               |
| Native behavior       | `src-tauri/src/commands/*.rs`                                       | Public `#[tauri::command]` functions are registered in `lib.rs`.             |
| Persistent app data   | `commands/config.rs`, `commands/store.rs`, clipboard DB, icon cache | Rust owns durable files; webview `localStorage` owns lightweight UI caches.  |
| Platform behavior     | Rust `cfg` modules and frontend `IS_*` flags                        | Gate each OS explicitly and document unsupported behavior.                   |

## Adding a feature

For a new user-facing command, start with the data flow: identify whether it is
static or query-driven, choose a provider or the legacy assembly path, add the
typed Tauri wrapper if native work is needed, register the Rust command, and add
the feature to the command and source maps. Define platform behavior before
writing a shared UI so unsupported platforms do not get a misleading command.
Add tests for pure logic and a manual checklist for OS APIs, permissions,
shortcuts, windows, or clipboard behavior.

## Keeping this document current

Update this page when startup ordering, window ownership, the frontend/backend
boundary, navigation invariants, or extension workflow changes. Verify claims
against `src/App.tsx`, `src/components/Palette.tsx`, `src/lib/tauri.ts`, and
`src-tauri/src/lib.rs`; if the command registration list changes, update
[`backend.md`](backend.md), [`commands.md`](commands.md), and
[`source-map.md`](source-map.md) together.
