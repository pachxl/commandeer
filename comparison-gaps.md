# Commandeer vs. Vicinae, Spotlight, Raycast — Gap Analysis

This document compares Commandeer with three reference launchers and identifies what makes sense to add next. Everything is ranked by Commandeer's own rule: **(frequency of use × keystrokes saved) ÷ effort**.

---

## 1. Commandeer's current state

### Architecture
- Tauri 2 + React 18 + TypeScript. Single reusable palette window, acrylic transparency, tray, autostart, deep links, single-instance toggle.
- Provider model: `CommandProvider` contributes static root commands (`getCommands`) and/or inline per-query results (`search`).
- Step-based navigation (push/pop/replace), breadcrumbs, slider steps, form steps, grid steps, live previews.
- Fuzzy ranking with weighted fields + frecency boost + alias/pin bonuses + stable sort.
- Overrides: alias, pin, global hotkey, "show at root".
- Action panel (Ctrl+K) with source-aware secondary actions.
- File index: SQLite FTS5 → Everything SDK → walkdir fallback.

### Already ported and working
Per [`PLAN.md`](PLAN.md): provider architecture, weighted fuzzy + frecency, calculator (units/currency/colors/dates/timezones), clipboard history, file search, snippets, Tools folder, kill process, configurable hotkeys, tray, autostart, deep links, themes, overrides UI, app launcher, system actions + volume.

### Backend exists but no frontend provider yet
- `read_quicklinks` / `write_quicklinks` are wired in Rust and `src/lib/tauri.ts`, and default quicklinks are seeded. There is **no** `quicklinks.ts` provider yet.

### In legacy but not yet ported
- Window switcher + window management (`window_mgmt.rs`, `windowSwitcher.ts`, `windowManagement.ts`).
- Bookmarks provider + `bookmarks.rs` + `favicon.rs`.
- Fonts browser (`fonts.rs`, `fonts.ts`).
- Test suite (`vitest` not in `package.json`, no `*.test.ts` files).
- Extension runtime (explicitly out of scope per [`PLAN.md`](PLAN.md)).

### Immediate rough edges
- `src-tauri/src/commands/window.rs` only handles transparency; the Win32 window-management code from legacy is gone.
- `commands/` directory is empty (legacy leftover).
- `scripts/dev-launcher.log` is committed noise.
- No automated tests; `npm run build` passes but coverage is zero.

**Verdict:** The core is production-grade. The biggest gaps are planned ports from legacy plus a small set of UX ideas from the comparison launchers.

---

## 2. Vicinae

Vicinae is a Qt/QML launcher with app search, clipboard history, snippets, file search, browser tab switcher, emoji picker, calculator, window switcher, font browser, volume control, React/TypeScript extensions, and Raycast-compatible script commands.

### Strengths worth borrowing

| Feature | Why it matters | Fit for Commandeer |
|---|---|---|
| **FZF v2 scoring** | Better fuzzy ranking than the current `fzf` npm package. | High effort, medium gain. [`PLAN.md`](PLAN.md) already skips this; current ranking is decent. |
| **Alias-prefix hoisting + stable sort** | Exact alias prefix matches above all; shorter aliases win ties; stable sort prevents flicker. | **Already implemented** in `Palette.tsx`. |
| **Favorites / pins** | Persistent top-of-list items. | Implemented as `pinned` override + score boost. |
| **"Fallback" items** | Items shown when query is empty, user-reorderable. | Not implemented; overlaps with last-command float-up and pins. |
| **Browser tab switcher** | Search open browser tabs. | Skipped in [`PLAN.md`](PLAN.md) — needs native-messaging extension per browser. |
| **Emoji picker** | Static table + fuzzy + grid view with recency/pins. | Listed as opportunistic in [`PLAN.md`](PLAN.md). Reasonable add. |
| **Font browser** | Grid view of installed fonts, click to copy family name. | **Free port** from legacy. Low effort, nice polish. |
| **Provider preferences / per-item preferences** | Rich extension configuration UI. | Out of scope because extension runtime is out of scope. |
| **dmenu mode** | Pipe stdin → palette → stdout. | Listed as opportunistic but fiddly on Windows. Defer. |

**Bottom line:** The ranking ideas are already in. Emoji picker and font browser are the clean, low-effort additions. Browser tabs and extension store are correctly out of scope.

---

## 3. Apple Spotlight

Spotlight is the OS-level search bar (Cmd+Space).

### Strengths
- Single search surface, no modes: apps, files, calculations, conversions, web suggestions, clipboard history from one field.
- Live results as you type with previews / Quick Look.
- Natural language calculations and conversions: `100 USD in EUR`, `5 km in miles`.
- Top hits + category headers (Applications, Documents, Folders, etc.).
- Actions: Open, Show in Finder, Copy, Look Up.
- Clipboard history search.
- Web suggestions at the bottom.
- Drag-and-drop out of results.

### What to copy
- **Unified "one box" search.** Commandeer currently splits flows: root search for commands vs `@find`/`@search`/`@web`/`@calc`/`@time` modes. Typing `budget` could surface the budget spreadsheet, budget app, and budget quicklink in one list.
- **Live file preview / Quick Look.** The DetailPane exists but only handles text files/colors/fonts. Add image thumbnails and generic file metadata.
- **"Reveal in folder" action.** Currently files only have Open and Copy path.

### What to skip
- Deep OS metadata-store integration. Commandeer's FTS5 + Everything fallback is sufficient.
- Web suggestions cluttering the bottom; `@web` is cleaner.

---

## 4. Raycast

Raycast is the modern keyboard-driven launcher benchmark.

### Strengths
- Extension ecosystem: Jira, Linear, Spotify, 1Password, Notion, Slack, etc.
- Quicklinks + fallback searches: `gh tauri` → GitHub search; `jira PROJ-123` → ticket.
- Snippets with keyword expansion.
- Clipboard history with rich previews.
- Window management (snap left/right, maximize, next monitor).
- AI features (Quick AI, AI Chat, custom commands) — Pro/paid.
- Notes, Focus, Calendar, Reminders, Flight tracker, Translator, Emoji picker, File search.
- Deep aliases + hotkeys.
- Menu bar commands.

### What to copy
- **Quicklinks with `{query}` arguments.** Already planned as #4 in [`PLAN.md`](PLAN.md). Highest Raycast value for lowest effort.
- **Window switcher + window management.** Already planned as #3 (deferred). Biggest missing daily workflow.
- **Emoji picker.** Fits existing grid step.
- **Fallback / default search engine.** Commandeer already shows "Search the web" on empty-state; could be more prominent.
- **Richer file actions:** Open With, Reveal, Copy path, Share.

### What to skip
- **Extension store / React-Node runtime.** [`PLAN.md`](PLAN.md) correctly out-of-scopes this.
- **Native third-party integrations** (Spotify, Notion, Jira). Quicklinks cover 80 % of the value for 5 % of the effort.
- **AI features.** Cost, complexity, privacy, scope creep.
- **Calendar / reminders / notes / flight tracker.** Becomes a PIM, not a launcher.

---

## 5. Recommended additions (prioritized)

### Tier 1 — do next

1. **Quicklinks provider + favicons**
   - Backend already exists (`read_quicklinks`, `write_quicklinks`, seeded defaults).
   - High daily value: `gh foo`, `jira PROJ-123`, `yt cats`.
   - Low effort: mostly adapt `commandeer-legacy/src/providers/quicklinks.ts` and port `favicon.rs`.

2. **Window switcher + window management**
   - Biggest alt-tab replacement value.
   - Legacy code is complete: `window_mgmt.rs`, `windowSwitcher.ts`, `windowManagement.ts`.
   - Medium effort. Exclude palette window, MRU ordering, "close window" action.

3. **Bookmarks provider + favicon fetch**
   - Rides same fuzzy/frecency plumbing as quicklinks.
   - Reads Chrome/Edge bookmarks (`bookmarks.rs`); favicon MIME-sniffing fix in legacy.
   - Medium effort (Rust backend port required).

### Tier 2 — strong value, moderate effort

4. **Unified "one box" search**
   - Make `@find` / `@web` / `@calc` / `@time` optional; surface files/web/calc results in the main root list.
   - Requires merging provider + file search results and careful ranking to avoid noise.

5. **Test suite**
   - Per [`PLAN.md`](PLAN.md), ~96 tests for pure modules.
   - Low effort, cheap insurance.

6. **Font browser**
   - Free legacy port, uses existing grid step.

### Tier 3 — polish / nice to have

7. **Emoji picker**
   - Static emoji table + grid step + fuzzy + recent/pinned.
   - Low-to-medium effort. Note Windows already has Win+.; value is moderate.

8. **Live file preview improvements**
   - Image thumbnails in DetailPane, generic file metadata, "Reveal in Explorer" action.

9. **Default action when no matches**
   - Configurable default search engine or quicklink for arbitrary text.

### Tier 4 — explicitly skip

- Extension store / Raycast-compatible runtime.
- Browser tab switcher (native-messaging extension per browser).
- Native third-party app integrations.
- AI features.
- Calendar / reminders / notes / flight tracker.
- FZF v2 scoring port (already skipped in [`PLAN.md`](PLAN.md)).
- dmenu mode (fiddly on Windows).

---

## 6. Suggested next steps

1. Port quicklinks + favicons.
2. Port window switcher + window management.
3. Port bookmarks.
4. Port the test suite.
5. Then evaluate unified "one box" search.

This keeps Commandeer a fast, keyboard-first, two-person force multiplier without drifting into Raycast's extension-store scope or Spotlight's OS-integration scope.
