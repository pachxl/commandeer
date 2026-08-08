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

## Onix Black Water presentation

Onix is a distinct Black Water interaction system, not a larger variation of
Default. A fresh root session opens as a compact search capsule containing only
the query field and active global-hotkey hint. Typing, clicking the search
surface, pressing Up/Down/Enter/Tab, opening a step, or showing feedback that
needs panel space blooms it into the full results panel. Expansion is sticky for
that visible session: clearing the query, closing Actions, or popping back to
root does not shrink the native window. Only the same whole-session reset used
for dismissal returns Onix to its compact state for the next invocation. The
pure transition rules and regression tests live in `lib/onixPresentation.ts`.

`OnixOpticalShell.tsx` is the one palette-wide material layer. Its WebGL2 path
uses a transparent premultiplied rounded-rectangle SDF to model dark edge
absorption, a Fresnel rim, pointer-directed specular light, restrained color
dispersion, caustics, and dither. Its depth-dependent smoked field is clearer at
the rim and darker through the interior, leaving enough environmental detail for
the adaptive native glass to remain visible. Rendering is event-driven, stops
after values settle, and uses a 2× backing store even on a 1× display so the
subpixel optical rim does not become jagged. WebGL cannot
sample another application's pixels; the native Acrylic/glass/vibrancy surface
provides the real environmental backdrop where available, while the shader adds
the shared optical edge treatment. Shader failure or context loss switches to a
marked CSS-gradient fallback without affecting input.

Selection uses `SelectionLens.tsx`, one geometry layer per selectable list,
grid, or Actions surface rather than a background painted by every row. Only the
active surface exposes its lens; opening Actions deactivates the result lens.
The lens reads the selected element's local offsets, so async item replacement,
scrolling, and palette scale do not create a second selection target. Keep the
existing selection clamping and movement-guarded pointer rules when changing it.

`usePaletteWindowSize.ts` serializes native resizes and coalesces pending
`ResizeObserver` phases to the newest geometry. On macOS, an Onix expansion uses
`resize_palette_window` to animate the borderless AppKit frame downward from a
fixed top edge over 150 ms. Each normal resize event interpolates the native
glass radius from capsule to panel during that first bloom while the CSS curve
and WebGL SDF follow the same short transition. Later result-height changes keep
the settled panel curve.
The macOS glass sits inside a matching rounded native clip, so WebGL can repaint
the growing surface without a transient full-panel dark overlay or a rectangular
glass host appearing at the corners. Reduced Motion takes the direct resize
path. Wayland sends final geometry through `resize_palette`; Windows and X11 use Tauri window
size, recentering only for width changes. In parallel, `set_palette_surface`
sends the applied style, compact/expanded state, and scale; the backend remembers
it and refreshes native clipping after every host resize. Preserve both sequences
so a slower earlier resize cannot overwrite the final panel size.

Accessibility preferences are material policy. Reduced motion freezes the
optical light and removes shell/lens travel. Reduced transparency disables
WebGL glass and uses an opaque high-contrast dark surface; forced-colors uses
system colors and removes decorative caustics. These paths must remain fully
keyboard-functional and cannot change palette navigation state.

When adding a load path:

1. Give it a request sequence or equivalent freshness guard.
2. Update results only if the request still belongs to the current mode/step.
3. Clear loading when the mode is abandoned.
4. Clamp selection after replacement and use absolute indices for pointer hover.
5. Make Enter read the same current item array that rendered the highlight.

Do not introduce a new effect that leaves a Tauri listener subscribed when
registration resolves after React cleanup. Chain the unlisten promise or use a
disposed flag. Components that poll or refresh only while the palette is visible
must use `useWindowFocused`, which defaults to hidden until the native window
reports its initial state and owns the late-registration cleanup.

## Common interaction implementations

| Interaction          | Implementation                                                                      |
| -------------------- | ----------------------------------------------------------------------------------- |
| Fuzzy result ranking | `src/lib/fuzzy.ts`, `paletteRanking.ts`, `frecency.ts`                              |
| Folder navigation    | `src/providers/tools.ts`, `paletteNavigation.ts`                                    |
| Forms                | `src/components/FormView.tsx` and `Step.isFormStep`; textareas own Enter/arrows     |
| Grids                | `src/components/ResultsGrid.tsx` and `Step.isGridStep`                              |
| Sliders              | `Step.isSliderStep`; apply effects per tick, serialize trailing config writes       |
| Confirmations        | `src/lib/confirm.ts`, `ConfirmOverlay.tsx`, and the Palette dismissal state machine |
| Detail Markdown      | `src/lib/markdown.ts`, `DetailPane.tsx`                                             |
| Icons                | `Icon.tsx`, lazy `iconPath` resolution, Rust `icons.rs`                             |
| Feedback             | `appEvents.ts`, `Toast.tsx`, `usePaletteFeedback.ts`                                |
| Onix presentation    | `onixPresentation.ts`, `OnixOpticalShell.tsx`, `SelectionLens.tsx`                  |
| Window sizing        | `usePaletteWindowSize.ts`; serialized newest-only native geometry                   |

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
state machine, Onix presentation/material policy, async invariants, component
ownership, IPC rules, or localStorage keys change. Check `src/types.ts`,
`src/App.tsx`, `src/providers/index.ts`, and `src/components/Palette.tsx` before
editing this page; update
[`commands.md`](commands.md) for user-visible command changes.
