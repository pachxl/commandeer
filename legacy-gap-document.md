# Commandeer: Features in Testing Branch Not Present in Main

This document catalogs all features, components, and infrastructure that exist in the `testing` branch but have **not** been ported to `main`.

---

## 📁 Missing Rust Commands (7)

| Command | File | Description |
|---------|------|-------------|
| **audio.rs** | `src-tauri/src/commands/audio.rs` | Volume/audio control via Core Audio (`IAudioEndpointVolume`) |
| **bookmarks.rs** | `src-tauri/src/commands/bookmarks.rs` | Browser bookmark integration and enumeration |
| **extensions.rs** | `src-tauri/src/commands/extensions.rs` | Extension manifest support |
| **favicon.rs** | `src-tauri/src/commands/favicon.rs` | Fetches and caches site favicons via Google S2 API |
| **fonts.rs** | `src-tauri/src/commands/fonts.rs` | System font enumeration via DirectWrite/GDI |
| **launcher.rs** | `src-tauri/src/commands/launcher.rs` | Start-Menu app enumeration with native icon extraction |
| **window_mgmt.rs** | `src-tauri/src/commands/window_mgmt.rs` | Window snap (corners/edges), quarters, restore with RECT tracking |

---

## 📡 Missing Frontend Providers (12)

| Provider | File | Description |
|----------|------|-------------|
| **appLauncher.ts** | `src/providers/appLauncher.ts` | App launching with native icons |
| **bookmarks.ts** | `src/providers/bookmarks.ts` | Browser bookmark search |
| **builtin.ts** | `src/providers/builtin.ts` | Built-in command registration |
| **fallbackSearch.ts** | `src/providers/fallbackSearch.ts` | Walkdir fallback when Everything unavailable |
| **fileSearch.ts** | `src/providers/fileSearch.ts` | File search provider (note: main has this in `commands/`) |
| **fonts.ts** | `src/providers/fonts.ts` | Font browser with grid preview |
| **quicklinks.ts** | `src/providers/quicklinks.ts` | URL shortcuts with `{query}` args and seeds |
| **scripts.ts** | `src/providers/scripts.ts` | Script command management |
| **settings.ts** | `src/providers/settings.ts` | Settings provider (main has this in `commands/`) |
| **snippets.ts** | `src/providers/snippets.ts` | Text snippet management (main has this in `commands/`) |
| **system.ts** | `src/providers/system.ts` | System actions: lock, sleep, hibernate, restart, shutdown, logout, empty trash |
| **volume.ts** | `src/providers/volume.ts` | Volume control UI |
| **windowManagement.ts** | `src/providers/windowManagement.ts` | Window snap/quarter/restore |
| **windowSwitcher.ts** | `src/providers/windowSwitcher.ts` | Alt-tab style window switching |

---

## 🧪 Missing Test Infrastructure

| File | Description |
|------|-------------|
| `src/lib/color.test.ts` | Color parsing, conversion, round-trip tests |
| `src/lib/frecency.test.ts` | Two-term frecency (frequency + recency) tests |
| `src/lib/fuzzy.test.ts` | Fuzzy matching, scoring, multi-field tests |
| `src/lib/math.test.ts` | Arithmetic, units, dates, percent semantics tests |
| `src/lib/overrides.test.ts` | Alias/pin caching and merge semantics tests |

**Total: 89 tests across 5 modules**

---

## 📄 Missing Documentation

| File | Description |
|------|-------------|
| `PLAN.md` | Original Raycast/Vicinae parity plan and roadmap |
| `VICINAE_PLAN.md` | Vicinae port analysis and feature tracking |

---

## 🛠️ Missing Scripts

| File | Description |
|------|-------------|
| `scripts/dev-launcher.bat` | Development launcher (Windows batch) |
| `scripts/dev-launcher.ps1` | Development launcher (PowerShell) |

---

## 📊 Summary Statistics

| Category | Testing | Main | Gap |
|----------|---------|------|-----|
| Rust Commands | 19 | 12 | **7** |
| Frontend Providers | 19 | 4 | **12** |
| Test Files | 5 | 0 | **5** |
| Documentation | 2 | 0 | **2** |
| Scripts | 2 | 0 | **2** |
| **Total Files** | **~65** | **~40** | **~25** |

---

## 🎯 Feature Gap Summary

### Core Systems (5)
- ✅ File indexing (SQLite+FTS5) — **PORTED**
- ✅ Clipboard history (encrypted SQLite) — **PORTED**
- ✅ Calculator (units/currency/colors) — **PORTED**
- ❌ Audio/volume control — **MISSING**
- ❌ Frecency test suite — **MISSING**

### App Ecosystem (6)
- ❌ App launcher (Start-Menu)
- ❌ Bookmarks
- ❌ Quicklinks
- ❌ Window management (snap/restore)
- ❌ Window switcher
- ❌ Fonts browser

### System Integration (4)
- ✅ Tray + autostart — **PORTED**
- ✅ Deep links — **PORTED**
- ✅ Global shortcuts — **PORTED**
- ❌ System actions (power management)

### UI/UX (3)
- ❌ Full detail pane with metadata
- ❌ Rich accessories/badges
- ❌ Fallback search UI

---

## 📝 Porting Status

| Phase | Features | Status |
|-------|----------|--------|
| Phase 1 | Provider architecture, UI framework, frecency, storage | ✅ PORTED |
| Phase 2 | Calculator, clipboard history, Tools/Snippets folders | ✅ PORTED |
| Phase 4 | Configurable hotkeys, tray, deep links, overrides UI | ✅ PORTED |
| **Phase 3** | App launcher, bookmarks, window mgmt, quicklinks, system actions, volume | ❌ **NOT PORTED** |
| **Phase 5+** | Tests, fonts, favicons, extensions, fallback search | ❌ **NOT PORTED** |

---

*Generated: $(date)*
*This file exists in both `commandeer-legacy/` and `main`/`testing` to document the migration state.*
