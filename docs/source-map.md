# Source ownership map

This is the documentation coverage map. Every application source module has a
row here, even when the module is small enough not to need a standalone page.
The linked subsystem page is the place to update when the module’s behavior
changes.

## Frontend entry points and types

| File            | Responsibility                                                         | Detailed page                                                        |
| --------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `src/main.tsx`  | Chooses palette vs screenshot bundle by Tauri window label             | [`features.md`](features.md)                                         |
| `src/App.tsx`   | Loads config, assembles providers/scripts/settings, bridges app events | [`architecture.md`](architecture.md), [`frontend.md`](frontend.md)   |
| `src/types.ts`  | Command, Step, PaletteItem, provider, and reducer contracts            | [`frontend.md`](frontend.md)                                         |
| `src/index.css` | Global palette/overlay styling and CSS variables                       | [`frontend.md`](frontend.md), [`configuration.md`](configuration.md) |

## Frontend commands and hooks

| File                                   | Responsibility                                                       | Detailed page                                              |
| -------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------- |
| `src/commands/index.ts`                | Script-to-command conversion, web search, confirmations, live output | [`scripts.md`](scripts.md), [`commands.md`](commands.md)   |
| `src/commands/settings.ts`             | Settings step, previews, persistence, feature toggles                | [`configuration.md`](configuration.md)                     |
| `src/commands/guide.ts`                | First-run welcome and searchable usage guide                         | [`frontend.md`](frontend.md), [`commands.md`](commands.md) |
| `src/commands/guide.test.ts`           | Guide content and navigation regression tests                        | [`testing.md`](testing.md)                                 |
| `src/commands/fileSearch.ts`           | Active-folder `@search` load/filter/open                             | [`features.md`](features.md)                               |
| `src/commands/globalFileSearch.ts`     | Global `@find` ranking and icon/detail mapping                       | [`features.md`](features.md)                               |
| `src/hooks/useInlineScripts.ts`        | Polls inline script output                                           | [`scripts.md`](scripts.md)                                 |
| `src/hooks/usePaletteFeedback.ts`      | Toast/feedback subscription and cleanup                              | [`frontend.md`](frontend.md)                               |
| `src/hooks/usePaletteFeedback.test.ts` | Feedback cleanup regression tests                                    | [`testing.md`](testing.md)                                 |
| `src/hooks/usePaletteWindowSize.ts`    | Measures and resizes the palette, especially Linux                   | [`platforms.md`](platforms.md)                             |

## Frontend providers

| File                           | Responsibility                                            | Detailed page                |
| ------------------------------ | --------------------------------------------------------- | ---------------------------- |
| `src/providers/index.ts`       | Provider registry, priority ordering, isolation           | [`frontend.md`](frontend.md) |
| `src/providers/appLauncher.ts` | Installed app cache, running badges, launch commands      | [`features.md`](features.md) |
| `src/providers/bookmarks.ts`   | Browser bookmark loading/search commands                  | [`commands.md`](commands.md) |
| `src/providers/calculator.ts`  | Calculator query results and currency warmup              | [`commands.md`](commands.md) |
| `src/providers/clipboard.ts`   | Clipboard History step and clear action                   | [`features.md`](features.md) |
| `src/providers/notes.ts`       | Note add/remove/copy commands                             | [`storage.md`](storage.md)   |
| `src/providers/processes.ts`   | Process grouping, kill step, inline kill search           | [`commands.md`](commands.md) |
| `src/providers/quicklinks.ts`  | Templated links and CRUD steps                            | [`storage.md`](storage.md)   |
| `src/providers/screenshot.ts`  | Screenshot palette trigger                                | [`features.md`](features.md) |
| `src/providers/system.ts`      | Power/session/appearance command definitions              | [`commands.md`](commands.md) |
| `src/providers/tools.ts`       | Tools folder, calculator/time-zone steps, virtual folders | [`frontend.md`](frontend.md) |
| `src/providers/volume.ts`      | Output-device sliders and Windows mixer entry             | [`features.md`](features.md) |

## Frontend components

| File                                   | Responsibility                                                          | Detailed page                                                      |
| -------------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `src/components/Palette.tsx`           | Main state machine, ranking, keyboard/pointer interaction, action panel | [`architecture.md`](architecture.md), [`frontend.md`](frontend.md) |
| `src/components/PaletteStatePanel.tsx` | Shared loading, empty, and error presentation                           | [`frontend.md`](frontend.md)                                       |
| `src/components/ScreenshotOverlay.tsx` | Capture selection, annotation, color-pick, finish/cancel UI             | [`features.md`](features.md)                                       |
| `src/components/VolumeMixer.tsx`       | Windows session mixer interaction                                       | [`features.md`](features.md), [`testing.md`](testing.md)           |
| `src/components/VolumeMixer.test.ts`   | Mixer rendering/navigation tests                                        | [`testing.md`](testing.md)                                         |
| `src/components/ActionPanel.tsx`       | Ctrl+K action/submenu UI                                                | [`commands.md`](commands.md)                                       |
| `src/components/ConfirmOverlay.tsx`    | Confirmation prompt UI                                                  | [`frontend.md`](frontend.md)                                       |
| `src/components/DetailPane.tsx`        | Metadata and Markdown details                                           | [`frontend.md`](frontend.md)                                       |
| `src/components/ClaudeUsage.tsx`       | Claude rate-limit panel/cache/polling                                   | [`features.md`](features.md), [`storage.md`](storage.md)           |
| `src/components/CodexUsage.tsx`        | Codex rate-limit panel/cache/polling                                    | [`features.md`](features.md), [`storage.md`](storage.md)           |
| `src/components/FormView.tsx`          | Multi-field Step form renderer                                          | [`frontend.md`](frontend.md)                                       |
| `src/components/Footer.tsx`            | Keyboard hints, settings/status controls                                | [`commands.md`](commands.md)                                       |
| `src/components/HudOverlay.tsx`        | Optional system/usage heads-up display                                  | [`storage.md`](storage.md)                                         |
| `src/components/Icon.tsx`              | Built-in, data URL, and lazy native icon rendering                      | [`features.md`](features.md)                                       |
| `src/components/ResultRow.tsx`         | Row rendering, badges, highlight and details affordances                | [`frontend.md`](frontend.md)                                       |
| `src/components/ResultsGrid.tsx`       | Grid rendering and fast highlight matching                              | [`frontend.md`](frontend.md)                                       |
| `src/components/ResultsList.tsx`       | List rendering and selection                                            | [`frontend.md`](frontend.md)                                       |
| `src/components/SearchInput.tsx`       | Query field and live preview                                            | [`frontend.md`](frontend.md)                                       |
| `src/components/StepBreadcrumb.tsx`    | Step-stack breadcrumb display                                           | [`frontend.md`](frontend.md)                                       |
| `src/components/SystemStats.tsx`       | Resource stats widget                                                   | [`features.md`](features.md)                                       |
| `src/components/Toast.tsx`             | Transient success/error feedback                                        | [`frontend.md`](frontend.md)                                       |

## Frontend libraries

| File                             | Responsibility                                  | Detailed page                                            |
| -------------------------------- | ----------------------------------------------- | -------------------------------------------------------- |
| `src/lib/tauri.ts`               | Single typed wrapper for Rust invokes/events    | [`backend.md`](backend.md)                               |
| `src/lib/paletteReducer.ts`      | Pure palette state transitions                  | [`frontend.md`](frontend.md)                             |
| `src/lib/paletteReducer.test.ts` | Reducer regression tests                        | [`testing.md`](testing.md)                               |
| `src/lib/paletteModes.ts`        | Root/step/`@search`/`@find` mode helpers        | [`frontend.md`](frontend.md)                             |
| `src/lib/paletteNavigation.ts`   | Step push/pop/replace and selection restoration | [`frontend.md`](frontend.md)                             |
| `src/lib/paletteActions.ts`      | Ctrl+K action construction                      | [`commands.md`](commands.md)                             |
| `src/lib/paletteItems.ts`        | Command-to-item and visible-item helpers        | [`frontend.md`](frontend.md)                             |
| `src/lib/paletteRanking.ts`      | Fuzzy/relevance/frecency ranking                | [`frontend.md`](frontend.md)                             |
| `src/lib/fuzzy.ts`               | Fzf matching and highlight helpers              | [`frontend.md`](frontend.md)                             |
| `src/lib/frecency.ts`            | Bounded localStorage usage ranking              | [`storage.md`](storage.md)                               |
| `src/lib/confirm.ts`             | Suppressed-confirm persistence and request API  | [`frontend.md`](frontend.md), [`storage.md`](storage.md) |
| `src/lib/appEvents.ts`           | Mutable bridge for App-owned callbacks          | [`architecture.md`](architecture.md)                     |
| `src/lib/scroll.ts`              | Keyboard/selection scroll helpers               | [`frontend.md`](frontend.md)                             |
| `src/lib/math.ts`                | Expression/unit evaluation                      | [`commands.md`](commands.md)                             |
| `src/lib/timezones.ts`           | Time-zone parsing/conversion                    | [`commands.md`](commands.md)                             |
| `src/lib/markdown.ts`            | Safe detail Markdown rendering                  | [`frontend.md`](frontend.md)                             |
| `src/lib/themes.ts`              | Built-in/user theme loading and CSS variables   | [`configuration.md`](configuration.md)                   |
| `src/lib/styles.ts`              | Structural UI style presets                     | [`configuration.md`](configuration.md)                   |
| `src/lib/overrides.ts`           | Per-command override/pin state                  | [`storage.md`](storage.md)                               |
| `src/lib/onboarding.ts`          | First-run eligibility and version marker        | [`frontend.md`](frontend.md), [`storage.md`](storage.md) |
| `src/lib/onboarding.test.ts`     | Onboarding eligibility regression tests         | [`testing.md`](testing.md)                               |

## Rust entry points and command modules

| File                                         | Responsibility                                      | Detailed page                                                                         |
| -------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `src-tauri/src/main.rs`                      | Native binary entry point                           | [`architecture.md`](architecture.md)                                                  |
| `src-tauri/src/lib.rs`                       | Tauri setup, windows, tray, events, registration    | [`architecture.md`](architecture.md), [`backend.md`](backend.md)                      |
| `src-tauri/src/commands/mod.rs`              | Command module declarations and cfg gates           | [`backend.md`](backend.md)                                                            |
| `src-tauri/src/commands/config.rs`           | AppConfig, paths, migrations, config IPC            | [`configuration.md`](configuration.md), [`storage.md`](storage.md)                    |
| `src-tauri/src/commands/shortcuts.rs`        | Shortcut parser, registration, command hotkeys      | [`configuration.md`](configuration.md), [`platforms.md`](platforms.md)                |
| `src-tauri/src/commands/linux_shortcuts.rs`  | COSMIC/GNOME managed shortcut files                 | [`platforms.md`](platforms.md)                                                        |
| `src-tauri/src/commands/deeplink.rs`         | `commandeer://` URL parsing/routing                 | [`commands.md`](commands.md)                                                          |
| `src-tauri/src/commands/fs.rs`               | Script discovery, metadata, execution, previews     | [`scripts.md`](scripts.md)                                                            |
| `src-tauri/src/commands/launcher.rs`         | Installed/running apps and launch paths             | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/search.rs`           | Global search fallback, file info, native icons     | [`features.md`](features.md)                                                          |
| `src-tauri/src/commands/file_index.rs`       | SQLite/FTS5 index and notify watcher                | [`features.md`](features.md), [`storage.md`](storage.md)                              |
| `src-tauri/src/commands/explorer.rs`         | Active Explorer/Finder folder and recursive listing | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/bookmarks.rs`        | Browser bookmark discovery                          | [`commands.md`](commands.md)                                                          |
| `src-tauri/src/commands/store.rs`            | Notes, Quick Links, themes, overrides files         | [`storage.md`](storage.md)                                                            |
| `src-tauri/src/commands/clipboard.rs`        | Clipboard monitor and text IPC                      | [`features.md`](features.md)                                                          |
| `src-tauri/src/commands/clipboard/db.rs`     | Clipboard SQLite schema, retention, migrations      | [`storage.md`](storage.md)                                                            |
| `src-tauri/src/commands/clipboard/crypto.rs` | DPAPI/ChaCha encryption and key sources             | [`features.md`](features.md), [`storage.md`](storage.md)                              |
| `src-tauri/src/commands/paste.rs`            | Foreground capture and paste-to-previous            | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/icons.rs`            | Native icon resolution, cache, warmup               | [`features.md`](features.md), [`storage.md`](storage.md)                              |
| `src-tauri/src/commands/process.rs`          | Process snapshots and kill operation                | [`commands.md`](commands.md)                                                          |
| `src-tauri/src/commands/stats.rs`            | CPU/memory/GPU stats                                | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/audio.rs`            | Device volume and Windows session mixer             | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/system.rs`           | Cross-platform power/session actions                | [`commands.md`](commands.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/appearance.rs`       | Native dark/light mode                              | [`commands.md`](commands.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/rates.rs`            | Cached currency rates                               | [`commands.md`](commands.md), [`storage.md`](storage.md)                              |
| `src-tauri/src/commands/screenshot.rs`       | Capture, overlay timing, annotation, copy           | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/window.rs`           | Native window transparency                          | [`configuration.md`](configuration.md), [`platforms.md`](platforms.md)                |
| `src-tauri/src/commands/window_drag.rs`      | Windows/macOS Alt-drag implementations              | [`features.md`](features.md), [`platforms.md`](platforms.md), [`TODO.md`](../TODO.md) |
| `src-tauri/src/commands/permissions.rs`      | macOS permission status and System Settings links   | [`platforms.md`](platforms.md), [`testing.md`](testing.md)                            |
| `src-tauri/src/commands/alt_tab.rs`          | Windows monitor-local Alt+Tab hooks/overlay         | [`features.md`](features.md), [`platforms.md`](platforms.md)                          |
| `src-tauri/src/commands/updater.rs`          | Release-only signed update loop                     | [`features.md`](features.md), [`RELEASING.md`](../RELEASING.md)                       |
| `src-tauri/src/commands/codex.rs`            | Codex OAuth usage lookup                            | [`features.md`](features.md)                                                          |
| `src-tauri/src/commands/claude.rs`           | Claude OAuth usage lookup                           | [`features.md`](features.md)                                                          |
| `src-tauri/src/commands/desktop.rs`          | Linux desktop-entry parsing/icons                   | [`scripts.md`](scripts.md), [`platforms.md`](platforms.md)                            |

## Project configuration and operational files

| File                                  | Responsibility                                           | Detailed page                                                        |
| ------------------------------------- | -------------------------------------------------------- | -------------------------------------------------------------------- |
| `package.json`                        | JS scripts, dependency versions, lint-staged rules       | [`testing.md`](testing.md)                                           |
| `bun.lock`                            | Dependency lockfile                                      | [`testing.md`](testing.md)                                           |
| `release.cjs`                         | Cross-platform release artifact copy/build orchestration | [`RELEASING.md`](../RELEASING.md)                                    |
| `vite.config.ts`                      | Frontend bundler configuration                           | [`architecture.md`](architecture.md)                                 |
| `tsconfig.json`                       | Strict TypeScript compiler policy                        | [`frontend.md`](frontend.md), [`testing.md`](testing.md)             |
| `eslint.config.js`                    | ESLint policy                                            | [`testing.md`](testing.md)                                           |
| `.prettierrc.json`                    | Prettier policy                                          | [`AGENTS.md`](../AGENTS.md)                                          |
| `src-tauri/Cargo.toml`                | Rust dependencies and platform gates                     | [`backend.md`](backend.md), [`platforms.md`](platforms.md)           |
| `src-tauri/tauri.conf.json`           | Shared Tauri windows, updater, deep link, capabilities   | [`architecture.md`](architecture.md), [`platforms.md`](platforms.md) |
| `src-tauri/tauri.linux.conf.json`     | Linux window override                                    | [`platforms.md`](platforms.md)                                       |
| `src-tauri/capabilities/default.json` | Webview permissions                                      | [`backend.md`](backend.md)                                           |
| `src-tauri/build.rs`                  | Tauri build integration                                  | [`architecture.md`](architecture.md)                                 |
| `.github/workflows/release.yml`       | CI release workflow                                      | [`RELEASING.md`](../RELEASING.md)                                    |
| `.husky/pre-commit`                   | Formatting and agent-sync pre-commit hook                | [`testing.md`](testing.md), [`AGENTS.md`](../AGENTS.md)              |
| `.agents/hooks/*`                     | Shared agent integrity/ship hooks                        | [`AGENTS.md`](../AGENTS.md)                                          |

## Keeping this document current

This map is complete only if every application source file appears in a row.
When adding, removing, renaming, or substantially repurposing a module, update
its row and detailed-page link in the same change. Periodically compare the
tables with `rg --files src src-tauri/src` and review docs links with the
repository’s formatter.
