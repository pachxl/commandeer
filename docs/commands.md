# Command catalog

This page describes where user-facing commands come from and how they become
searchable. It is intentionally a catalog, not a second implementation: stable
ids, labels, and platform filters must be verified against the source files.

## Root command sources

Providers are loaded concurrently and sorted by priority. The priority affects
assembly order and tie behavior; fuzzy ranking still decides what the user sees
for a query.

| Provider          |               Priority | Main commands or results                                                  | Source                                                            |
| ----------------- | ---------------------: | ------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Applications      |                     60 | Installed apps, running-state badges                                      | [`appLauncher.ts`](../src/providers/appLauncher.ts)               |
| Screenshot        |                     50 | Take Screenshot                                                           | [`screenshot.ts`](../src/providers/screenshot.ts)                 |
| System            |                     40 | Lock, sleep, hibernate, restart, shutdown, logout, trash, dark/light mode | [`system.ts`](../src/providers/system.ts)                         |
| Volume            |                     40 | Set Volume, Toggle Mute, Windows Volume Mixer                             | [`volume.ts`](../src/providers/volume.ts)                         |
| Clipboard         |                     35 | Clipboard History, hidden Clear Clipboard History                         | [`clipboard.ts`](../src/providers/clipboard.ts)                   |
| Processes         |                     20 | Kill Process and `kill <name>` inline results                             | [`processes.ts`](../src/providers/processes.ts)                   |
| Tools             |                     15 | Calculator, Time Zone Converter                                           | [`tools.ts`](../src/providers/tools.ts)                           |
| Quick Links       |                     12 | Add/remove links and templated URL commands                               | [`quicklinks.ts`](../src/providers/quicklinks.ts)                 |
| Notes             |                     11 | Add/remove notes and copy note contents                                   | [`notes.ts`](../src/providers/notes.ts)                           |
| Bookmarks         |                     10 | Browser bookmark commands and query results                               | [`bookmarks.ts`](../src/providers/bookmarks.ts)                   |
| Calculator search |                     10 | Expression result and Google fallback                                     | [`calculator.ts`](../src/providers/calculator.ts)                 |
| Scripts           | assembled in `App.tsx` | User script commands and folders                                          | [`commands/index.ts`](../src/commands/index.ts), `commands/fs.rs` |
| Settings          | assembled in `App.tsx` | Appearance, shortcuts, paths, feature toggles                             | [`commands/settings.ts`](../src/commands/settings.ts)             |
| Commandeer Guide  | assembled in `App.tsx` | First-run welcome, shortcuts, search modes, and workflow help             | [`commands/guide.ts`](../src/commands/guide.ts)                   |

Quick Links, Notes, and Bookmarks are represented as subfolders inside Tools in
the assembled root. Their children can also appear in flat search through their
`folderName` and source metadata.

## Built-in interactions

- `@find <query>` uses the indexed global file-search path, with the Rust index
  first and platform fallbacks after it.
- `@search <query>` loads the focused Explorer/Finder folder on Windows/macOS;
  Linux falls back to the home folder.
- Calculator expressions can be entered through the Tools step or recognized by
  the calculator provider. Currency needs cached or freshly fetched rates;
  an initial offline failure is retried by a later query instead of disabling
  conversion for the rest of the process.
- Time-zone conversion is handled by the Tools input step and copies its result.
- `Ctrl+K` opens actions for the selected row. Action-panel actions may be leaf
  handlers or nested submenus.
- `Commandeer Guide` remains searchable after the first-run welcome is complete.
- `Ctrl+M` opens the Windows Volume Mixer when that feature is available.
- `commandeer://command/<id>` is navigation-only: it shows/focuses the palette,
  opens the command's root step (including confirmation steps), or selects a
  leaf command for an explicit Enter/click. URI handling never invokes
  `Command.action`; only that later user activation does. The legacy
  `commandeer://run/<id>` spelling has the same safe behavior.
  `commandeer://screenshot` starts capture for the managed Linux desktop
  binding; `commandeer://open` only shows the palette.

## Safety and visibility

Destructive system actions and scripts marked `needsConfirmation` push a
confirmation Step. Search-only commands are intentionally absent from the root
browse list but remain discoverable by typing. Clipboard contents do not surface
in global root search; they are available only inside Clipboard History.

When adding a command, decide explicitly:

1. Is it browsable, search-only, or a child of a folder?
2. Does it close the palette, stay open, or push a step?
3. Does it need confirmation or a permission explanation?
4. Which `CommandSource`, stable id, keywords, aliases, icon, and action label
   should it use?
5. On which platforms should it be listed, and what does the backend return on
   unsupported platforms?

## Native IPC command groups

The registered backend surface is grouped here for navigation. The exact
arguments and return shapes are in `src/lib/tauri.ts` and `lib.rs`.

| Group             | Registered commands                                                                                                                                                                                                                 |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Config/scripts    | `read_config`, `write_config`, `list_scripts`, `run_script`, `run_script_capture`, `read_text_preview`, `reveal_path`                                                                                                               |
| User data         | `data_dir`, `read_quicklinks`, `write_quicklinks`, `read_notes`, `write_notes`, `read_overrides`, `write_overrides`, `read_themes`                                                                                                  |
| Launch/search     | `list_apps`, `running_app_paths`, `run_app`, `search_files`, `file_info`, `path_icon`, `explorer_location`, `list_files_recursive`, `list_bookmarks`                                                                                |
| Clipboard/paste   | `read_clipboard_history`, `clear_clipboard_history`, `write_clipboard_text`, `paste_to_previous`                                                                                                                                    |
| System/media      | `system_action`, `set_dark_mode`, `list_processes`, `kill_process`, `system_stats`, `list_audio_devices`, `get_volume`, `set_volume`, `toggle_mute`, `list_audio_sessions`, `set_audio_session_volume`, `toggle_audio_session_mute` |
| Window/shortcuts  | `set_window_transparency`, `set_global_hotkey`, `set_screenshot_hotkey`, `set_command_hotkey`, `set_window_drag`, `set_per_monitor_alt_tab`, `set_alt_tab_theme`                                                                    |
| Screenshot        | `start_screenshot`, `show_screenshot_overlay`, `reveal_screenshot_overlay`, `hide_screenshot_overlay`, `pick_frame_color`, `finish_screenshot`, `cancel_screenshot`                                                                 |
| Permissions       | `permission_status`, `open_permission_settings`                                                                                                                                                                                     |
| External services | `get_rates`, `codex_usage`, `claude_usage`                                                                                                                                                                                          |

## Keeping this document current

Update the provider table when a provider, priority, command family, search
prefix, shortcut, or platform filter changes. Update the IPC table from the
`generate_handler!` list in `src-tauri/src/lib.rs` and the wrappers in
`src/lib/tauri.ts`; do not rely on memory. Review stable ids and confirmation
behavior whenever a command is renamed or moved between folders.
