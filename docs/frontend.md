# Frontend architecture

The frontend is a strict TypeScript React application. The key design is a
small command/step protocol rather than a separate bespoke screen for every
feature.

## Core types

`src/types.ts` defines the public frontend model:

- `Command` is a root-list entry. It can run an `action`, push a `Step`, be
  grouped under a virtual folder, or expose metadata/details for the action
  panel.
- `Step` is a navigation level. It can be a list, grid, slider, form, or raw
  input step. Selection returns `done`, `push`, `replace`, `pop`, or `stay`.
- `PaletteItem` is the rendered row model. `source`, `keywords`, `iconPath`,
  metadata, live output, and detail Markdown affect ranking and presentation.
- `CommandProvider` contributes static root commands through `getCommands`
  and/or query-specific commands through `search`.

Prefer these types over feature-specific state passed through the whole tree.
If a feature needs an unusual interaction, add a Step capability deliberately
and document its keyboard contract.

## Command assembly

`src/providers/index.ts` registers the newer provider families. Providers are
sorted by descending priority, loaded concurrently, and isolated so one failed
provider does not remove all root commands. `src/App.tsx` still assembles:

- settings commands;
- the permanently searchable Commandeer Guide and first-run welcome step;
- scanned script commands and script folders;
- the optional web-search command;
- the Tools virtual folder and its dynamic Quick Links, Notes, and Bookmarks
  children.

When adding a provider, register it in `src/providers/index.ts`, give it a stable
provider id and priority, and decide whether its children belong in a virtual
folder or should be directly searchable. Keep command ids stable: frecency,
overrides, pinned state, and per-command hotkeys use them as durable keys.

## Palette state and async safety

`Palette.tsx` and `paletteReducer.ts` manage the step stack, query, item cache,
loading/error state, and selected index. `paletteModes.ts` distinguishes normal
steps, `@search`, `@find`, and root search. `paletteNavigation.ts` and
`paletteActions.ts` keep keyboard transitions and Ctrl+K actions separate from
rendering.

Loading, empty, and error states share `PaletteStatePanel.tsx`, keeping feedback
visually consistent across root search and loaded steps. First-run onboarding is
opened only after the hidden Accessory window receives real focus; existing
installations are detected through earlier localStorage keys and are not interrupted.

When adding a load path:

1. Give it a request sequence or equivalent freshness guard.
2. Update results only if the request still belongs to the current mode/step.
3. Clear loading when the mode is abandoned.
4. Clamp selection after replacement and use absolute indices for pointer hover.
5. Make Enter read the same current item array that rendered the highlight.

Do not introduce a new effect that leaves a Tauri listener subscribed when
registration resolves after React cleanup. Chain the unlisten promise or use a
disposed flag.

## Common interaction implementations

| Interaction          | Implementation                                                                      |
| -------------------- | ----------------------------------------------------------------------------------- |
| Fuzzy result ranking | `src/lib/fuzzy.ts`, `paletteRanking.ts`, `frecency.ts`                              |
| Folder navigation    | `src/providers/tools.ts`, `paletteNavigation.ts`                                    |
| Forms                | `src/components/FormView.tsx` and `Step.isFormStep`                                 |
| Grids                | `src/components/ResultsGrid.tsx` and `Step.isGridStep`                              |
| Sliders              | `Step.isSliderStep`; apply effects per tick, serialize trailing config writes       |
| Confirmations        | `src/lib/confirm.ts`, `ConfirmOverlay.tsx`, and the Palette dismissal state machine |
| Detail Markdown      | `src/lib/markdown.ts`, `DetailPane.tsx`                                             |
| Icons                | `Icon.tsx`, lazy `iconPath` resolution, Rust `icons.rs`                             |
| Feedback             | `appEvents.ts`, `Toast.tsx`, `usePaletteFeedback.ts`                                |
| Window sizing        | `usePaletteWindowSize.ts`; Linux delegates size requests to Rust layer-shell setup  |

## IPC and error handling

`src/lib/tauri.ts` is the only place frontend code should call `invoke`. Keep
serialized argument and return types beside their wrappers. Convert native
errors into user-visible feedback at the command boundary; do not silently turn
a failed destructive action into success. The screenshot flow is especially
strict: a failed finish must release its finishing guard and leave the capture
retryable.

## Local UI state

Small preferences and caches use webview `localStorage`: game mode, widget
visibility, script/app caches, frecency, confirmation suppression, last command,
the onboarding version, and usage-panel polling caches. Durable user content and configuration belongs
in the Rust-owned files documented in [`storage.md`](storage.md).

## Keeping this document current

Update this page when the command/step protocol, provider registration, Palette
state machine, async invariants, component ownership, IPC rules, or localStorage
keys change. Check `src/types.ts`, `src/App.tsx`, `src/providers/index.ts`, and
`src/components/Palette.tsx` before editing this page; update
[`commands.md`](commands.md) for user-visible command changes.
