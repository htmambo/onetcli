# Terminal Theme Follows App Theme Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make terminal tabs follow the app theme by default, while terminal-side theme changes become per-tab overrides that do not affect other tabs and survive restored tabs, and align terminal theme options with app themes that were previously missing.

**Architecture:** Add an app-theme-to-terminal-theme adapter in `terminal_view`, track per-view theme override state inside `TerminalView`, stop treating terminal theme as a global default, and remove the global terminal color scheme control from the settings page. Reuse the existing tab restore `theme_name` field as the persisted per-tab override.

**Tech Stack:** Rust, gpui, gpui_component, one_ui, serde, existing terminal/tab restore pipeline.

---

### Task 1: Add app-theme to terminal-theme adapter

**Files:**
- Modify: `crates/terminal_view/src/theme.rs`
- Modify: `crates/terminal_view/src/view.rs`

**Steps:**
1. Add a helper that derives a `TerminalTheme` from the active app theme colors and highlight style.
2. Map app theme background/foreground/selection/base colors into terminal foreground, background, cursor, selection, and ANSI palette.
3. Keep terminal typography preservation behavior unchanged.

### Task 2: Convert terminal theme changes to per-view overrides

**Files:**
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `crates/terminal_view/src/sidebar/settings_panel.rs`
- Modify: `crates/terminal_view/src/sidebar/mod.rs`

**Steps:**
1. Add per-view override state to `TerminalView`.
2. When the terminal sidebar changes theme, apply it only to the current `TerminalView`.
3. Stop writing theme changes into global terminal settings or app settings.
4. Keep sidebar UI synced to the effective current theme of that view.

### Task 3: Make app theme changes refresh only non-overridden terminals

**Files:**
- Modify: `main/src/home/home_tabs.rs`
- Modify: `main/src/setting_tab.rs`
- Modify: `crates/terminal_view/src/view.rs`

**Steps:**
1. Replace the old global terminal-theme propagation path with an app-theme refresh path.
2. On app theme updates, regenerate and apply terminal themes only for views without an override.
3. Preserve existing cross-tab sync for font, line height, cursor blink, and other terminal-wide settings.

### Task 4: Remove global terminal theme setting from settings page

**Files:**
- Modify: `main/src/setting_tab.rs`
- Modify: `main/locales/main.yml`

**Steps:**
1. Remove the terminal color scheme dropdown from the global settings page.
2. Keep compatibility fields only where needed for deserialization or legacy migration.
3. Update locale strings if they become unused.

### Task 5: Preserve per-tab theme override through restore

**Files:**
- Modify: `crates/terminal_view/src/view.rs`
- Verify: `crates/core/src/connection_restore.rs`

**Steps:**
1. Reinterpret restored `theme_name` as a per-tab override flag/value.
2. Ensure restored local terminal tabs reapply the override after the default follow-app theme is established.
3. Leave tabs without `theme_name` following the current app theme.

### Task 6: Add or update targeted tests and run verification

**Files:**
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `main/src/setting_tab.rs`
- Modify: `main/src/home/home_tabs.rs`

**Steps:**
1. Add focused tests for theme derivation or state behavior where coverage is practical.
2. Run targeted Rust tests for touched modules.
3. If no precise automated test exists for one path, document the exact manual verification performed.

### Task 7: Align app-only themes in the terminal list

**Files:**
- Modify: `crates/terminal_view/src/theme.rs`
- Create: `crates/terminal_view/schemes/Adventure`
- Create: `crates/terminal_view/schemes/Catppuccin Latte`
- Create: `crates/terminal_view/schemes/Flexoki Light`
- Create: `crates/terminal_view/schemes/Matrix`

**Steps:**
1. Add terminal theme definitions for app themes that exist in the settings page but not in the terminal theme list.
2. Keep terminal-only themes available as per-tab manual overrides.
3. Adjust terminal theme de-duplication to allow distinct names that differ only by case when they intentionally represent different themes.
