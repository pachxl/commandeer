# Raycast Extension API — Scoping Report for Commandeer

**Purpose:** Define a *curated, verified* subset of the Raycast extension API that commandeer can realistically host, for human review. This is research/scoping only — no code.
**Source:** https://developers.raycast.com/ (API reference pages, fetched as `.md`), https://www.raycast.com/store (popular listings).
**Scope guarantee:** A *guaranteeable* subset for in-app use — **not** open-store compatibility. Extensions run inside commandeer only if they stick to the subset; everything else is detected and surfaced as "unsupported API".

---

## 1. Full Raycast API Surface Inventory

The Raycast platform is two packages: **`@raycast/api`** (bundled with the app, the native surface) and **`@raycast/utils`** (installable npm, React hooks + helpers built on top of the api). Extensions are React + Node/TypeScript; the manifest is a superset of `package.json`.

### 1.1 UI Components (`@raycast/api`, top-level exports)

Four "page" components host an extension's main view; every one accepts an `actions` (ActionPanel) prop and an `isLoading`/`navigationTitle`. Docs: https://developers.raycast.com/api-reference/user-interface.md

#### List — https://developers.raycast.com/api-reference/user-interface/list.md
The de-facto UI. Built-in fuzzy filtering over `title` + `keywords`.
- **`List`** props: `actions, children, filtering(bool|{keepSectionOrder}), isLoading, isShowingDetail, navigationTitle, onSearchTextChange, onSelectionChange, pagination{hasMore,onLoadMore,pageSize}, searchBarAccessory(List.Dropdown), searchBarPlaceholder, searchText, selectedItemId, throttle`.
- **`List.Item`** props: `title*, accessories[], actions, detail, icon, id, keywords, quickLook{path,name}, subtitle`. `title`/`subtitle`/`icon` may be `{tooltip, value}`.
- **`List.Item.Accessory`** — `{icon?, text, tooltip?}` array on the right of an item.
- **`List.Section`** — `children, title, subtitle`.
- **`List.Dropdown`** (search-bar accessory) — `tooltip*, children, defaultValue, filtering, id, isLoading, onChange, onSearchTextChange, placeholder, storeValue, throttle, value`.
- **`List.Dropdown.Item`** — `title*, value*, icon, keywords`.
- **`List.Dropdown.Section`** — `children, title`.
- **`List.EmptyView`** — `actions, description, icon, title`.
- **`List.Item.Detail`** — inline detail pane (`isLoading, markdown, metadata`) shown when `isShowingDetail`.
- **`List.Item.Detail.Metadata`** — `children`, with sub-elements: `Label{title,text,icon}`, `Link{title,target,text}`, `TagList{title}` + `TagList.Item{text,color,icon,onAction}`, `Separator`.

#### Grid — https://developers.raycast.com/api-reference/user-interface/grid.md
Image-first variant of List. `content` replaces `icon`.
- **`Grid`** props: `actions, aspectRatio("1"|"3/2"|...|"9/16"), children, columns(1-8), filtering, fit, fit(Grid.Fit), inset(Grid.Inset), isLoading, navigationTitle, onSearchTextChange, onSelectionChange, pagination, searchBarAccessory(Grid.Dropdown), searchBarPlaceholder, searchText, selectedItemId, throttle`.
- **`Grid.Item`** — `content* (ImageLike | {color} | {tooltip,value}), accessory, actions, id, keywords, quickLook, subtitle, title`.
- **`Grid.Section`** — `aspectRatio, children, columns, fit, inset, title, subtitle`.
- **`Grid.Dropdown` / `.Item` / `.Section`** — mirror List.Dropdown.
- **`Grid.EmptyView`** — `actions, description, icon, title`.
- `Grid.Fit` (`contain`/`fill`), `Grid.Inset` (`small`/`medium`/`large`), `Grid.Item.Accessory`.

#### Detail — https://developers.raycast.com/api-reference/user-interface/detail.md
Markdown (CommonMark) view with optional right-side metadata. Supports LaTeX and `raycast-width/height/tintColor` image query params.
- **`Detail`** props: `actions, isLoading, markdown, metadata, navigationTitle`.
- **`Detail.Metadata`** with `Label{title,text,icon}`, `Link{title,target,text}`, `TagList{title}` + `TagList.Item{text,color,icon,onAction}`, `Separator`.

#### Form — https://developers.raycast.com/api-reference/user-interface/form.md
Data entry. Controlled *and* uncontrolled items; `storeValue` persists across launches; `enableDrafts` preserves unsubmitted input.
- **`Form`** props: `actions, children, enableDrafts, isLoading, navigationTitle, searchBarAccessory(Form.LinkAccessory)`.
- Field types (confirmed from docs): **`Form.TextField`**, **`Form.PasswordField`**, **`Form.TextArea`** (with `enableMarkdown`), **`Form.Checkbox`**, **`Form.DatePicker`**, **`Form.Dropdown`** + **`Form.Dropdown.Item`** + **`Form.Dropdown.Section`**, **`Form.Separator`**, **`Form.LinkAccessory`**.
- Common field props: `id*, autoFocus, defaultValue, error, info, onBlur, onChange, onFocus, placeholder, storeValue, title, value` (Password/Text have no `title`-required but use it; Checkbox adds `label*`).
- Imperative methods on each field: `focus()`, `reset()`.
- `Action.SubmitForm` is the submit bridge; `onSubmit(values)` receives a `Form.Values` map.

#### ActionPanel — https://developers.raycast.com/api-reference/user-interface/action-panel.md
- **`ActionPanel`** — `children, title`. First/second Action become primary (`↵`) / secondary (`⌘↵`).
- **`ActionPanel.Section`** — `children, title`.
- **`ActionPanel.Submenu`** — replaces panel with children when selected. Props: `title*, autoFocus, children, filtering, icon, isLoading, onOpen, onSearchTextChange, shortcut, throttle`.

#### Actions — https://developers.raycast.com/api-reference/user-interface/actions.md
- **`Action`** — `title*, autoFocus, icon, onAction, shortcut, style(Alert.ActionStyle)`.
- Built-in actions: **`Action.CopyToClipboard`** (`content*, concealed, onCopy`), **`Action.Paste`** (`content*, onPaste`), **`Action.Open`** (`target*, title*, application, onOpen`), **`Action.OpenInBrowser`** (`url*, onOpen`), **`Action.OpenWith`** (`path*, onOpen`), **`Action.Push`** (`target*, title*, onPush, onPop`), **`Action.ShowInFinder`** (`path*, onShow`), **`Action.Trash`** (`paths*, onTrash`), **`Action.SubmitForm`** (`onSubmit, ...`), **`Action.CreateSnippet`** (`snippet*`), **`Action.CreateQuicklink`** (`quicklink*`), **`Action.ToggleQuickLook`**, **`Action.PickDate`** (`onChange*, title*, type, min, max` + static `isFullDay`).
- **`Action.Style`** — `Regular | Destructive`.

#### Cross-cutting primitives
- **`Icon`** — ~700-name enum of built-in SF-style glyphs (e.g. `Icon.Circle`, `Icon.Star`, `Icon.Trash`). https://developers.raycast.com/api-reference/user-interface/icons-and-images.md
- **`Color`** — `Blue, Green, Magenta, Orange, Purple, Red, Yellow, PrimaryText, SecondaryText`; plus `Color.ColorLike = Color | Color.Dynamic{light,dark,adjustContrast} | Color.Raw(hex/rgba/hsl/keyword)`. https://developers.raycast.com/api-reference/user-interface/colors.md
- **`Image.ImageLike`** — `string` (Icon name, asset path, or URL) | `{source, tintColor?}` | `Buffer` | `Asset`.
- **`Keyboard`** — `Shortcut{key, modifiers[]}`, `KeyEquivalent` (~70 keys incl. arrows/return/escape), `KeyModifier` (`cmd|ctrl|opt|shift|alt|windows`), `Keyboard.Shortcut.Common` table (Copy, Save, New, Open, Refresh, …). Platform-split shortcuts supported: `{macOS:{…}, Windows:{…}}`. https://developers.raycast.com/api-reference/keyboard.md

### 1.2 Feedback / Imperative UI
- **Toast** — `showToast(options)` returns a mutable `Toast`; `Toast.Style = Animated|Success|Failure`; supports `primaryAction`/`secondaryAction` with shortcuts; falls back to HUD when window closed. https://developers.raycast.com/api-reference/feedback/toast.md
- **HUD** — `showHUD(title, {clearRootSearch, popToRootType})`; closes the main window. https://developers.raycast.com/api-reference/feedback/hud.md
- **Alert** — `confirmAlert(options)` → `Promise<boolean>`; `Alert.Options{title*, message, icon, primaryAction, dismissAction, rememberUserChoice}`; `Alert.ActionStyle = Default|Destructive|Cancel`. https://developers.raycast.com/api-reference/feedback/alert.md

### 1.3 Services / Top-level API objects

| Export | Methods | Notes / docs |
|---|---|---|
| **Clipboard** | `copy(content, {concealed})`, `paste(content)`, `clear()`, `read({offset})`, `readText({offset})` | `Content = {text}|{file}|{html,text?}`; `offset` 0-5 reaches clipboard history. clipboard.md |
| **LocalStorage** | `getItem`, `setItem`, `removeItem`, `allItems`, `clear` | Encrypted at rest; values `string|number|boolean`; per-extension namespace. storage.md |
| **Cache** (class) | `get/has/set/remove/clear/subscribe`, `isEmpty`; `Cache.Options{capacity, namespace}` | On-disk LRU, default 10MB. cache.md |
| **Environment** | `environment` object (`raycastVersion, ownerOrAuthorName, extensionName, commandName, commandMode, assetsPath, supportPath, isDevelopment, appearance, textSize, launchType, canAccess(api)`) | Plus free functions `getSelectedFinderItems()`, `getSelectedText()`. environment.md |
| **System Utilities** | `getApplications(path?)`, `getDefaultApplication(path)`, `getFrontmostApplication()`, `showInFinder(path)`, `trash(path)`, `open(target, application?)`, `captureException(e)` | `Application{name,path,bundleId,localizedName,windowsAppId}`. utilities.md |
| **Command** | `launchCommand({name, type, arguments?, context?, fallbackText?})` (+ inter-extension variant w/ `extensionName, ownerOrAuthorName`), `updateCommandMetadata({subtitle})` | LaunchType.UserInitiated/Background. command.md |
| **Navigation / Window** | `popToRoot({clearSearchBar})`, `clearSearchBar({forceScrollToTop})`, `closeMainWindow({clearRootSearch, popToRootType})` | `PopToRootType = Default|Immediate|Suspended`. window-and-search-bar.md |
| **Preferences** | `getPreferenceValues()`, `openExtensionPreferences()`, `openCommandPreferences()` | Types auto-generated into `Preferences` global namespace. preferences.md |
| **AI** | `AI.ask(prompt, {creativity, model, signal})` → `Promise<string> & EventEmitter` (streams via `on('data')`) | **Pro-gated**; `environment.canAccess(AI)`; rate-limited 10/min, 100/hr; ~60 models. ai.md |
| **OAuth** | `OAuth.PKCEClient` w/ `authorizationRequest/authorize/setTokens/getTokens/removeTokens` | PKCE flow; Raycast hosts a PKCE proxy + redirect endpoints (`raycast.com/redirect`, `raycast://oauth`). oauth.md |
| **WindowManagement** | `getActiveWindow()`, `getWindowsOnActiveDesktop()`, `getDesktops()`, `setWindowBounds({id, bounds})` | **Pro-gated; macOS only**. window-management.md |
| **BrowserExtension** | `getContent({cssSelector, tabId, format})`, `getTabs()` | Requires the Raycast browser extension; **macOS only**. browser-extension.md |
| **MenuBarExtra** | `<MenuBarExtra>` + `.Item / .Submenu / .Section / .Separator` | `menu-bar` mode; macOS only. menu-bar-commands.md |

### 1.4 Hooks (`@raycast/utils`) — https://developers.raycast.com/utilities/getting-started.md

| Hook | Purpose | Env dependency |
|---|---|---|
| `usePromise(fn, deps)` | run an async fn, expose `{isLoading, data, error, revalidate, pagination}` | none |
| `useCachedPromise` | `usePromise` + Cache-backed persistence across launches | none |
| `useCachedState(key, initialValue)` | `useState` persisted to `LocalStorage`/Cache | none |
| `useFetch(url, options)` | fetch wrapper with pagination/`mapResult` | none (network) |
| `useForm(initialValues, {onSubmit, validation})` | form state + `FormValidation` helpers | none |
| `useLocalStorage(key, initial)` | reactive `LocalStorage` mirror | none |
| `useFrecencySorting(items)` | frecency-rank a list | none |
| `useExec(command, args, options)` | run a child process, stream output | **needs Node `child_process`** |
| `useSQL(database, query)` | query SQLite via `executeSQL` | **needs Node `better-sqlite3`** |
| `useAI(prompt, options)` | `AI.ask` as a React hook | **Pro-gated** |
| `useStreamJSON(response)` | stream-parse JSON from a `useFetch` response | none |

Utility functions (`@raycast/utils`): `createDeeplink`, `executeSQL`, `runAppleScript` (**macOS**), `runPowerShellScript` (**Windows**), `showFailureToast`, `withCache`, plus icon helpers `getAvatarIcon`, `getFavicon`, `getProgressIcon`, and OAuth helpers `OAuthService`, `withAccessToken`, `getAccessToken`.

### 1.5 Manifest schema — https://developers.raycast.com/information/manifest.md

`package.json` is a superset of npm's. Raycast-specific fields:

**Extension-level:** `name*, title*, description*, icon* (png ≥512px; `icon@dark.png` for dark), author*, platforms* (["macOS","Windows"]), categories*, commands*, tools, ai, owner, access("public"|"private"), contributors, pastContributors, keywords, preferences, external`.

**Command-level:** `name* (→ src/<name>.tsx), title*, subtitle, description*, icon, mode*, interval (e.g. "5m"/"12h"/"1d"; min 1m; for no-view/menu-bar background), keywords, arguments, preferences, disabledByDefault`.

**Preference properties:** `name*, title*, description*, type* ("textfield"|"password"|"checkbox"|"dropdown"|"appPicker"|"file"|"directory"), required*, placeholder, default` (platform-split via `{macOS, Windows}`); checkbox adds `label*`; dropdown adds `data*=[{title,value}]`.

**Argument properties:** `name*, type* ("text"|"password"|"dropdown"), placeholder*, required`. Dropdown adds `data*`.

### 1.6 Modes — https://developers.raycast.com/information/lifecycle.md + manifest

Three modes only (there is **no `LaunchAgent`** in Raycast — background work is `no-view`/`menu-bar` + `interval`):
- **`view`** — exports a React component; pushes onto the navigation stack.
- **`no-view`** — exports an `async function`; no UI; can call `showHUD`/`Clipboard`/`open` then exit. Background-refreshable via `interval`.
- **`menu-bar`** — returns `<MenuBarExtra>` or `null`; **macOS only**; refreshable via `interval`; lifecycle: root-search launch / interval / icon-click / Raycast restart / re-enable in prefs.

`LaunchProps` (passed to every command): `arguments, launchType (UserInitiated|Background), draftValues, fallbackText, launchContext`.

### 1.7 Navigation model
- **Push**: `<Action.Push target={<Component/>}>` pushes a React node onto the stack.
- **Pop**: implicit (back gesture / Esc) — no explicit `pop()` export.
- **popToRoot({clearSearchBar})**: collapse the whole stack to root search.
- **clearSearchBar({forceScrollToTop})**.
- **closeMainWindow({clearRootSearch, popToRootType})**: hide the window; `PopToRootType.Default|Immediate|Suspended`.
- **launchCommand**: programmatic, intra- or inter-extension (inter triggers a permission alert).

---

## 2. What is NOT Reproducible Without Raycast's Backend or macOS-only Deps

### 2.1 AI APIs (Pro, Raycast's backend)
`AI.ask` / `useAI` proxy ~60 models (OpenAI GPT-5.x/o-series, Anthropic Claude 4.x, Google Gemini, xAI Groq, Mistral, Perplexity, etc.) through **Raycast's server with Raycast account billing** — no API keys from the user. Not reproducible as-is. **Mitigation:** expose an `AI.ask` shim that routes to a user-configured provider key (OpenAI/Anthropic/local Ollama), returning a `Promise<string> & EventEmitter` with a `.on('data')` stream. `environment.canAccess(AI)` should return `true` once a key is configured so gating works.

### 2.2 macOS-only / Apple-service APIs
These have **no cross-platform general API** and should be excluded (or shimmed per-OS where commandeer already has equivalents):
- **`MenuBarExtra`** — macOS menu bar. Commandeer already has a tray icon; a shim could render the same tree into the tray menu (v2).
- **`getSelectedFinderItems()`** — Finder automation. Commandeer already does Finder automation on macOS; on Linux it falls back to home folder. Map to commandeer's existing channel.
- **`WindowManagement.*`** — macOS Accessibility-based window moving. Commandeer has its own Alt-drag window management; the *Raycast* API surface is macOS-only and Pro-gated, so exclude.
- **`BrowserExtension.*`** — requires the Raycast browser extension (macOS only). Exclude; a future commandeer browser bridge could shim `getTabs`/`getContent`.
- **`runAppleScript`** (utils) — macOS. Map to commandeer's existing AppleScript channel on macOS; no-op (or surface "macOS-only") elsewhere.
- **`getFrontmostApplication()` / `getApplications()` / `getDefaultApplication()`** — work cross-platform in Raycast but are Apple-event-backed on macOS. Commandeer already has launcher/process modules; shim these from commandeer's app-index.

### 2.3 Raycast server / account infrastructure
- **OAuth redirect endpoints** (`raycast.com/redirect`, `raycast://oauth`) and the **PKCE proxy** (`oauth.raycast.com`) are Raycast-hosted. Extensions using `OAuth.PKCEClient` with `RedirectMethod.Web` hardcode Raycast's redirect URI. **Mitigation:** host commandeer's own redirect (`commandeer://oauth`) and PKCE proxy, or rewrite the redirect URI at install time. This is the single biggest "not free" item — needed by ~30-40% of store extensions (anything with login).
- **`captureException`** → Raycast Developer Hub. Replace with a local error log / Sentry.
- **Store install/launch pipeline** (`raycast://` deep links, signed builds) — out of scope; commandeer curates its own install path.
- **`environment.canAccess(API)`** gating is Raycast's license check — commandeer should implement it as "is this shim available on this OS/config".

---

## 3. Recommended Initial Subset for Commandeer (v1)

Goal: cover the largest number of *simple* store extensions with the smallest implementation surface. The subset below is ordered by ROI.

### 3.1 UI components to implement first

| Component | Sub-features for v1 | Defer to v2 |
|---|---|---|
| **List** | `List`, `List.Item` (title/subtitle/icon/accessories/keywords/id/actions), `List.Section`, `List.EmptyView`, `searchText`/`onSearchTextChange`/`searchBarPlaceholder`, `filtering`, `isLoading`, `navigationTitle`, `selectedItemId`, `onSelectionChange`, `throttle` | `pagination` (non-trivial virtualization), `isShowingDetail` + `List.Item.Detail` + Metadata, `List.Dropdown` (search-bar accessory) |
| **ActionPanel** | `ActionPanel`, `ActionPanel.Section`, `ActionPanel.Submenu` (static children) | `onOpen` lazy submenus, filtering in submenus |
| **Actions** | `Action`, `Action.CopyToClipboard`, `Action.Paste`, `Action.OpenInBrowser`, `Action.Push`, `Action.SubmitForm`, `Action.Style`, `Action.Open`, `Action.ShowInFinder` | `Action.Trash`, `Action.OpenWith`, `Action.CreateSnippet`, `Action.CreateQuicklink`, `Action.ToggleQuickLook`, `Action.PickDate` |
| **Form** | `Form`, `Form.TextField`, `Form.TextArea`, `Form.Checkbox`, `Form.PasswordField`, `Form.Dropdown`+Item+Section, `Form.Separator`, `Action.SubmitForm`, `error`/`onChange`/`value`/`defaultValue` | `Form.DatePicker`, `Form.LinkAccessory`, `enableDrafts`, `storeValue`, imperative `focus()`/`reset()` |
| **Detail** | `Detail` with `markdown` (CommonMark subset: headings, bold/italic, code, links, images, lists) + `actions` | `Detail.Metadata` (Label/Link/TagList/Separator), LaTeX, `raycast-width/height/tint` image params |
| **Grid** | — (defer entirely) | all of Grid — most "simple" extensions use List, not Grid |
| **MenuBarExtra** | — (defer; commandeer tray is the analog) | all |
| **Icons/Colors/Keyboard** | `Icon` enum (map to a bundled icon set, e.g. Lucide), `Color` standards + raw hex, `Image.ImageLike` for asset paths + URLs, `Keyboard.Shortcut` with `KeyModifier`/`KeyEquivalent` and `Keyboard.Shortcut.Common` | `Color.Dynamic` per-theme, platform-split shortcuts (just pick one mapping) |

**Rationale:** List + ActionPanel + the core Actions + a minimal Form cover the overwhelming majority of "search a thing, copy/open the result" extensions. Detail without metadata still serves markdown-render use cases. Grid is almost exclusively emoji/icon pickers — defer until the icon-pipeline is solid.

### 3.2 Services to implement first

| Service | v1 surface | How to back it |
|---|---|---|
| **Clipboard** | `copy`, `paste`, `clear`, `readText`, `read` (text+html) | commandeer already has an encrypted clipboard module (ChaCha20/DPAPI/key-file) — wire `invoke` calls to it. `offset` history (0-5) maps to commandeer's clipboard history. |
| **Environment** | `environment` (raycastVersion→commandeer version, commandMode, commandName, extensionName, assetsPath, supportPath, isDevelopment, appearance, textSize, launchType, canAccess) + `getSelectedText` | `canAccess` returns true for everything in the v1 set, false for AI/WindowManagement/BrowserExtension/MenuBarExtra. `getSelectedText` → commandeer's existing selected-text path. |
| **LocalStorage** | `getItem/setItem/removeItem/allItems/clear` | SQLite per-extension namespace (commandeer already uses SQLite for file index). |
| **Cache** | `Cache` class with `get/set/has/remove/clear/subscribe`, `isEmpty` | same SQLite store, LRU by timestamp column. |
| **System Utilities** | `open(target, app?)`, `getFrontmostApplication()`, `getApplications()`, `showInFinder(path)` (→ reveal in file manager), `trash(path)` | commandeer's launcher + process modules already do this cross-platform. |
| **Navigation** | `popToRoot({clearSearchBar})`, `clearSearchBar`, `closeMainWindow`, `PopToRootType` | map to commandeer's step-stack (`pop`/`popToRoot`) and palette hide. |
| **Preferences** | `getPreferenceValues()`, `openExtensionPreferences()`/`openCommandPreferences()` | read from extension's parsed `package.json` preferences block + a settings JSON commandeer writes; "open preferences" → commandeer Settings step for that extension. |
| **Feedback** | `showToast` (mutable, styles, actions), `showHUD`, `confirmAlert` | render toasts as a palette overlay; HUD as a brief palette flash; confirmAlert as a modal step (commandeer already has action panels). |
| **Command** | `launchCommand` (intra-extension only for v1) | spawn the target command's entry in a fresh extension context. |

**Defer / shim-later:** AI (shim to user key), OAuth (host own redirect + proxy), WindowManagement (exclude), BrowserExtension (exclude), MenuBarExtra (route to tray), inter-extension `launchCommand`.

### 3.3 Hooks — essential vs deferrable

| Hook | v1? | Reason |
|---|---|---|
| `usePromise`, `useCachedPromise`, `useCachedState`, `useFetch`, `useLocalStorage`, `useFrecencySorting`, `useForm` | **Yes — bundle `@raycast/utils`** | pure React, no Node deps; cover ~80% of extensions |
| `useStreamJSON` | **Yes** | pure React + fetch; cheap |
| `useExec` | **No** | needs `child_process` → Node runtime path only |
| `useSQL` / `executeSQL` | **No** | needs `better-sqlite3` native → Node runtime only |
| `useAI` | **No (shim later)** | Pro-gated; route to user key |

Since `@raycast/utils` is an npm package that peer-depends on `@raycast/api`, commandeer should publish a *local* `@raycast/api` shim package and let `@raycast/utils` resolve against it; the pure-React hooks then "just work" in the webview.

### 3.4 Modes to support first
- **`view`** — yes, primary.
- **`no-view`** — yes (cheap: run the async fn, route `showHUD`/`Clipboard`/`open` to commandeer).
- **`menu-bar`** — **later** (route to commandeer tray icon).
- **Background `interval`** — later (needs a scheduler; commandeer has single-instance + tray, so a timer loop is feasible but defer).

### 3.5 Explicit v1 EXCLUSIONS & how to communicate them

**Exclude from v1:** AI, OAuth (PKCEClient), WindowManagement, BrowserExtension, MenuBarExtra, `useExec`, `useSQL`/`executeSQL`, `runAppleScript`/`runPowerShellScript`, List pagination, List/Grid Detail+Metadata, Form.DatePicker/drafts/storeValue, Grid entirely, `Action.PickDate`/`Trash`/`OpenWith`/`CreateSnippet`/`CreateQuicklink`/`ToggleQuickLook`.

**Detection + UX:** At install/load, statically analyze the extension's bundled JS for references to excluded exports (a denylist of `@raycast/api` member paths + `@raycast/utils` hook names). Three outcomes:
1. **Clean** → install and run.
2. **Uses a shimmed-later API** (e.g. AI, OAuth) → install but show a **"Limited support"** badge in the extension's row + a one-line reason ("Uses AI — configure a provider in Settings"); the API call throws a typed `UnsupportedAPIError` at runtime that the extension is expected to catch (most do via `environment.canAccess`).
3. **Uses a hard-excluded API** (WindowManagement, BrowserExtension, `useExec`) → **block install** with an inline message: *"This extension uses `<API>`, which commandeer doesn't support."* Offer to open the source on GitHub.

`environment.canAccess(X)` must reflect this exact matrix so well-written extensions degrade gracefully instead of crashing.

---

## 4. Test Harness Design

### 4.1 Snapshot-testing React extension rendering against a JSON tree
Raycast renders React to *native* UI; commandeer renders React to *DOM*. So commandeer can host the extension's React **directly** (see §6) and snapshot the DOM tree, but to keep snapshots host-stable, normalize to a **JSON component tree**:
- Wrap each `@raycast/api` component in a thin renderer that, in test mode, serializes its props + children to a JSON node (`{type:'List.Item', props:{title:'…'}, children:[…]}`) instead of painting DOM. This is exactly the "vicinae" reconciler approach, but used only as a *test fixture* — production runs real React in the webview.
- Use `react-test-renderer` (no DOM) against the shim components to produce the tree, then `expect(tree).toMatchInlineSnapshot()` (Jest/Vitest). Fuzzy-match props where non-deterministic (icons, IDs) via a custom serializer.
- Cover one snapshot per extension × a few interaction states (initial render, after search, after action).

### 4.2 Corpus runner for real extensions
- Maintain a **fixtures repo** of real extensions (cloned from the raycast/extensions GitHub monorepo) pinned at a manifest SHA.
- A runner script: for each extension in the allowlist, `npm i` it against commandeer's `@raycast/api` shim, then drive it headlessly in the webview (Playwright against the dev build) with a scripted interaction sequence (type query → enter → assert toast/clipboard).
- **Breakage detection:** intercept `throw`/`console.error`/React error boundaries + assert no `UnsupportedAPIError` escaped. A run is "green" if the extension rendered its primary view and at least one action completed without an uncaught error.
- Run on every change to the shim; CI-gate the allowlist.

### 4.3 Verified-extension allowlist curation process
1. **Triage by static scan**: run the denylist analyzer over the corpus; keep only "clean" or "shimmed-later" extensions.
2. **Manual API audit**: read the extension's `package.json` + entry file; record every `@raycast/api`/`@raycast/utils` import in a per-extension manifest (`apis.json`).
3. **Interaction author**: write a 3-5 step Playwright script per extension (stored alongside the allowlist entry).
4. **Two-eyes review**: a second person signs off the API list + script.
5. **Pin + version-bump policy**: pin the extension git SHA; on upstream change, re-run the suite before bumping.
6. **Tier labels**: `Tier-1 (fully supported)`, `Tier-2 (shimmed — works if provider configured)`, `Tier-3 (best-effort, may regress)`.

---

## 5. Initial Verified-Extension Candidate List

Candidates chosen for: small API surface, no OAuth/AI/WindowManagement/BrowserExtension/MenuBar, no `useExec`/`useSQL`. Store install counts from raycast.com/store/popular (Jul 2026). "Likely APIs" is a *guess* from the extension's described behavior; the real audit (§4.3 step 2) confirms it.

| # | Extension | Author | What it does | Likely APIs | Confidence |
|---|---|---|---|---|---|
| 1 | **Kaomoji Search** | Alexander Ignatov (`yalishanda`) | Search & copy ASCII/unicode kaomoji | List, List.Item, Action.CopyToClipboard, static data | **High** — featured, pure data |
| 2 | **Can I Use** | Thomas Lombart (`thomaslombart`) | Browser-support lookup for web tech | List, useFetch, Action.OpenInBrowser | **High** — read-only fetch |
| 3 | **Google Translate** | Slavik Nychkalo (`gebeto`) | Translate via Google Translate (420k) | Form, useFetch, Clipboard.paste, Action.CopyToClipboard | **Medium** — fetch + clipboard; *may* need a free key pref |
| 4 | **Color Picker** | Thomas Paul Mann (`thomas`) | Pick & organize colors (480k) | List, Action.CopyToClipboard, Clipboard, Color | **Medium** — "pick from screen" needs OS capture; the *organize/list* half fits |
| 5 | **UUID Generator** *(common)* | various | Generate UUIDs | List/Form, Action.CopyToClipboard, `crypto` | **High** — pure JS |
| 6 | **Lorem Ipsum** *(common)* | various | Insert placeholder text | Form, Clipboard.paste | **High** |
| 7 | **Timestamp / Unix Time** *(common)* | various | Convert/insert timestamps | Form, List, Action.CopyToClipboard, `Date` | **High** |
| 8 | **Base64 Encode/Decode** *(common)* | various | Encode/decode strings | Form, Action.CopyToClipboard | **High** |
| 9 | **JSON Formatter** *(common)* | various | Pretty/minify JSON | Form.TextArea, Action.CopyToClipboard | **High** |
| 10 | **URL Encode/Decode** *(common)* | various | Percent-encode strings | Form, Action.CopyToClipboard | **High** |
| 11 | **Hash Generator** *(common)* | various | md5/sha1/sha256 of text | Form, Action.CopyToClipboard, `crypto` | **High** |
| 12 | **Color Converter** *(common)* | various | hex↔rgb↔hsl | Form, List, Color, Action.CopyToClipboard | **High** |
| 13 | **HTTP Status Codes** *(common)* | various | Reference list of status codes | List, List.Section, Action.CopyToClipboard | **High** — static |
| 14 | **Regex Tester** *(common)* | various | Test regex against input | Form, List, `RegExp` | **High** |
| 15 | **QR Code Generator** *(common)* | various | Make a QR from text/URL | Form, Action.CopyToClipboard, image gen lib | **Medium** — image payload |
| 16 | **Wikipedia Search** *(common)* | various | Search & open Wikipedia | List, useFetch, Action.OpenInBrowser | **High** |
| 17 | **DNS Lookup** *(common)* | various | Resolve DNS records | Form, List, `dns` node lib | **Medium** — needs Node `dns` |
| 18 | **Currency Converter** *(common)* | various | FX rates via public API | Form, useFetch, Action.CopyToClipboard | **Medium** — fetch; rate source varies |
| 19 | **Emoji Search** *(common)* | various | Find & copy emoji | List, List.Section, Action.CopyToClipboard, static data | **High** |
| 20 | **Gitignore Generator** *(common)* | various | Build .gitignore from templates | List, Form, Action.CopyToClipboard | **High** — static templates |
| 21 | **Cron Expression Builder** *(common)* | various | Compose/explain cron | Form, List, Action.CopyToClipboard | **High** |
| 22 | **Lorem Picsum** *(common)* | various | Insert random placeholder image URL | List, Action.CopyToClipboard, Action.OpenInBrowser | **High** |
| 23 | **Word/Synonym Lookup** *(common)* | various | Datamuse/free dictionary API | Form, List, useFetch, Action.CopyToClipboard | **High** |
| 24 | **GitHub Gist** *(common)* | various | Create/fetch gists | List, Form, useFetch, Action.CopyToClipboard | **Medium** — *may* use OAuth for create; fetch-only variants fit |
| 25 | **Bitly/URL Shortener** *(common)* | various | Shorten a URL via API | Form, useFetch, Action.CopyToClipboard | **Medium** — usually needs a key pref |

**Selection bias:** clipboard tools, converters, static-reference, and simple public-API fetchers dominate. Anything that logged in (Slack/Notion/Linear/Spotify/1Password) was deliberately excluded — those need OAuth and are a v2 milestone. Note ~17-19 are "common" ecosystem extensions not pinned to one store author; the curator should pick the highest-quality maintained instance of each from the raycast/extensions monorepo.

---

## 6. Commandeer's Architectural Advantage

Commandeer's UI is already **React in a platform webview** (WebView2/WKWebView/WebKitGTK). Raycast extensions are **React + TypeScript**. Unlike *vicinae* (which had to build a custom React reconciler that serializes to JSON because its host is Qt/QML), commandeer can host extension React **directly** — the same reconciler already runs. This collapses a huge amount of work: no JSON protocol, no host-side re-render, no prop-buffering latency.

Only extensions needing Node built-ins (`fs`, `child_process`, `better-sqlite3`, `dns`, native crypto-streams) require a bundled Node runtime. The v1 subset (§3) is deliberately chosen so that *none* of the candidate extensions (§5) need Node built-ins — they can all run in the webview.

### Hosting options — pros/cons

**(a) Web Worker in the same webview**
- *Pros:* shared origin with the palette → zero IPC for `@raycast/api` calls (shims are just `postMessage` to the main thread or direct module calls); fastest startup; no extra binary; natural sandbox (Worker is same-origin, no DOM access → can't touch the palette DOM); CORS-friendly if the shim proxies `fetch` through the main thread to Rust.
- *Cons:* no Node built-ins (a Worker is a browser context); `DOMException` on sync `fs`; bundle must be browser-compatible (the extension's deps must not `require('fs')` at top level — esbuild can mark them external). Memory ceiling per Worker.
- *Verdict:* **default for v1** — fits every §5 candidate.

**(b) Sandboxed `<iframe srcdoc>` in the same webview**
- *Pros:* hard process-like isolation (separate JS realm; a crash doesn't kill the palette); can render the extension's List/Form as *real DOM* inside the iframe → exact visual fidelity, no JSON tree needed in production; CSP per-extension; easy to tear down (`iframe.remove()`).
- *Cons:* async messaging only (postMessage) → every `@raycast/api` call is a round-trip; layout coupling (iframe size must track content height — commandeer already does this for the palette on Linux); same no-Node-built-ins constraint as Worker; `sandbox` attribute must allow `allow-scripts` but not `allow-same-origin` for true isolation, which breaks `localStorage` → need a shimmed storage via postMessage.
- *Verdict:* **best for visual fidelity & crash isolation**; slightly more plumbing than Worker. Good v2 default once the shim API stabilizes.

**(c) Separate Node child process** (bundled Node or `node`-in-Rust via `deno`/`bun` runtime)
- *Pros:* full Node API → `useExec`/`useSQL`/`runAppleScript`/`fs`/`dns`/native modules all work; enables the *excluded* Tier to become supported; matches Raycast's own model (each extension is a Node process).
- *Cons:* heaviest — must ship a Node runtime (or bundle via `pkg`/`bun`); IPC is now OS-level (stdio/IPC socket) → latency + serialization cost; per-process memory; startup time (~50-150ms cold); security surface (extension code runs with full Node privileges → need a permission prompt per extension).
- *Verdict:* **only for Node-dependent extensions** (the ones in §3.5's "hard-excluded" bucket that are still worth supporting). Trigger transparently: the loader detects `require('fs'|'child_process'|...)` or `useExec`/`useSQL` imports and spawns a Node worker for *that* extension only, while UI still renders in the webview via a JSON bridge (the vicinae model, used as a *fallback* here, not the primary path).

### Recommended split
- **Default path (a) Web Worker** for any extension whose static scan shows no Node-built-in usage — this is ~90% of the §5 candidates and likely a majority of the store.
- **Promote to (b) iframe** when an extension is crash-prone in testing or needs pixel-perfect DOM rendering (e.g. complex Form layouts).
- **Fall back to (c) Node child** only when the scan finds Node built-ins; render its UI through a minimal JSON bridge into the webview (the one place commandeer reuses the vicinae reconciler idea, narrowly scoped).

This tiering keeps the fast path fast, isolates risk, and only pays the Node cost where it's unavoidable — a strict improvement over both Raycast (always-Node) and vicinae (always-JSON-bridge).

---

## Appendix A — Doc URLs consulted (all `.md` form)

- Index: https://developers.raycast.com/llms.txt
- UI: /api-reference/user-interface{,list,grid,detail,form,action-panel,actions,colors,icons-and-images,navigation}.md
- Services: /api-reference/{clipboard,environment,storage,cache,command,preferences,window-and-search-bar,utilities,ai,oauth,window-management,browser-extension,menu-bar-commands,keyboard,feedback,feedback/toast,feedback/hud,feedback/alert}.md
- Lifecycle/manifest: /information/{manifest,lifecycle,lifecycle/arguments,lifecycle/background-refresh,lifecycle/deeplinks}.md
- Utils: /utilities/{getting-started,react-hooks/*,functions/*,icons/*,oauth/*}.md
- Store: https://www.raycast.com/store , /store/popular

## Appendix B — Mode/preference quick reference

- Modes: `view` | `no-view` | `menu-bar` (no `LaunchAgent`).
- Preference types: `textfield` (string), `password` (string), `checkbox` (boolean), `dropdown` (string), `appPicker` (Application), `file` (string path), `directory` (string path).
- Argument types: `text` | `password` | `dropdown`.
- PopToRootType: `Default` | `Immediate` | `Suspended`.
- LaunchType: `UserInitiated` | `Background`.
