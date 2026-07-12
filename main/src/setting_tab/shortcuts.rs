//! 设置页 - 快捷键 区块（轮 9b 重构抽取）。
//!
//! 仅供父模块 `SettingsPanel::render` 使用，标 `pub(super)`。

use gpui::{App, FontWeight, IntoElement, Keystroke, ParentElement, Styled, div};
use gpui_component::kbd::Kbd;
use gpui_component::{ActiveTheme, h_flex, v_flex};
use rust_i18n::t;

use super::app_settings::AppSettings;
use super::hotkey::{DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};

/// 快捷键条目
struct ShortcutEntry {
    /// macOS 快捷键字符串（Keystroke::parse 格式）
    key_macos: &'static str,
    /// Windows/Linux 快捷键字符串（Keystroke::parse 格式）
    key_other: &'static str,
    /// 国际化翻译 key
    label_key: &'static str,
}

/// 快捷键分组
struct ShortcutGroup {
    title_key: &'static str,
    entries: &'static [ShortcutEntry],
}

const WINDOW_SHORTCUTS: &[ShortcutEntry] = &[
    ShortcutEntry {
        key_macos: "cmd-q",
        key_other: "alt-f4",
        label_key: "Settings.Shortcuts.quit_app",
    },
    ShortcutEntry {
        key_macos: DEFAULT_SYSTEM_HOTKEY_MACOS,
        key_other: DEFAULT_SYSTEM_HOTKEY_OTHER,
        label_key: "Settings.Shortcuts.minimize_window",
    },
    ShortcutEntry {
        key_macos: "ctrl-cmd-f",
        key_other: "alt-enter",
        label_key: "Settings.Shortcuts.toggle_fullscreen",
    },
    // 窗口置顶仅 Windows 实现；设置页仍展示快捷键说明
    ShortcutEntry {
        key_macos: "ctrl-alt-t",
        key_other: "ctrl-alt-t",
        label_key: "Settings.Shortcuts.toggle_always_on_top",
    },
    ShortcutEntry {
        key_macos: "shift-escape",
        key_other: "shift-escape",
        label_key: "Settings.Shortcuts.toggle_zoom",
    },
    ShortcutEntry {
        key_macos: "ctrl-w",
        key_other: "ctrl-w",
        label_key: "Settings.Shortcuts.close_panel",
    },
];

const TAB_SHORTCUTS: &[ShortcutEntry] = &[
    ShortcutEntry {
        key_macos: "cmd-1",
        key_other: "alt-1",
        label_key: "Settings.Shortcuts.switch_tab_n",
    },
    ShortcutEntry {
        key_macos: "shift-cmd-t",
        key_other: "alt-shift-t",
        label_key: "Settings.Shortcuts.duplicate_tab",
    },
    ShortcutEntry {
        key_macos: "cmd-o",
        key_other: "alt-o",
        label_key: "Settings.Shortcuts.quick_open",
    },
    ShortcutEntry {
        key_macos: "cmd-n",
        key_other: "alt-n",
        label_key: "Settings.Shortcuts.new_connection",
    },
];

const TERMINAL_SHORTCUTS: &[ShortcutEntry] = &[
    ShortcutEntry {
        key_macos: "cmd-c",
        key_other: "ctrl-shift-c",
        label_key: "Settings.Shortcuts.terminal_copy",
    },
    ShortcutEntry {
        key_macos: "cmd-v",
        key_other: "ctrl-shift-v",
        label_key: "Settings.Shortcuts.terminal_paste",
    },
    ShortcutEntry {
        key_macos: "cmd-f",
        key_other: "ctrl-shift-f",
        label_key: "Settings.Shortcuts.terminal_search",
    },
    ShortcutEntry {
        key_macos: "cmd-a",
        key_other: "ctrl-shift-a",
        label_key: "Settings.Shortcuts.terminal_select_all",
    },
    ShortcutEntry {
        key_macos: "cmd-+",
        key_other: "ctrl-+",
        label_key: "Settings.Shortcuts.terminal_zoom_in",
    },
    ShortcutEntry {
        key_macos: "cmd--",
        key_other: "ctrl--",
        label_key: "Settings.Shortcuts.terminal_zoom_out",
    },
    ShortcutEntry {
        key_macos: "cmd-0",
        key_other: "ctrl-0",
        label_key: "Settings.Shortcuts.terminal_zoom_reset",
    },
    ShortcutEntry {
        key_macos: "f7",
        key_other: "f7",
        label_key: "Settings.Shortcuts.terminal_toggle_vi",
    },
];

const DATABASE_SHORTCUTS: &[ShortcutEntry] = &[
    ShortcutEntry {
        key_macos: "cmd-f",
        key_other: "ctrl-f",
        label_key: "Settings.Shortcuts.database_focus_search",
    },
    ShortcutEntry {
        key_macos: "cmd-shift-enter",
        key_other: "ctrl-shift-enter",
        label_key: "Settings.Shortcuts.database_open_table_query",
    },
    ShortcutEntry {
        key_macos: "cmd-enter",
        key_other: "ctrl-enter",
        label_key: "Settings.Shortcuts.sql_run_query",
    },
];

const SHORTCUT_GROUPS: &[ShortcutGroup] = &[
    ShortcutGroup {
        title_key: "Settings.Shortcuts.window",
        entries: WINDOW_SHORTCUTS,
    },
    ShortcutGroup {
        title_key: "Settings.Shortcuts.tabs",
        entries: TAB_SHORTCUTS,
    },
    ShortcutGroup {
        title_key: "Settings.Shortcuts.terminal",
        entries: TERMINAL_SHORTCUTS,
    },
    ShortcutGroup {
        title_key: "Settings.Shortcuts.database",
        entries: DATABASE_SHORTCUTS,
    },
];

fn shortcut_spec_for_entry(entry: &ShortcutEntry, cx: &App) -> String {
    if entry.label_key == "Settings.Shortcuts.minimize_window" {
        return AppSettings::global(cx).current_system_hotkey().to_string();
    }

    if cfg!(target_os = "macos") {
        entry.key_macos.to_string()
    } else {
        entry.key_other.to_string()
    }
}

fn render_shortcut_value(key_str: &str, cx: &App) -> gpui::AnyElement {
    match Keystroke::parse(key_str) {
        Ok(keystroke) => Kbd::new(keystroke).into_any_element(),
        Err(_) => div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(key_str.to_string())
            .into_any_element(),
    }
}

/// 渲染快捷键说明页面
pub(super) fn render_shortcuts_section(cx: &App) -> gpui::AnyElement {
    let mut container = v_flex().gap_4().p_4();

    for group in SHORTCUT_GROUPS {
        let mut group_container = v_flex().gap_2();

        // 分组标题
        group_container = group_container.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(t!(group.title_key).to_string()),
        );

        // 快捷键列表
        let mut list = v_flex().gap_1().pl_2();

        for entry in group.entries {
            let key_str = shortcut_spec_for_entry(entry, cx);

            list = list.child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .py_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!(entry.label_key).to_string()),
                    )
                    .child(render_shortcut_value(&key_str, cx)),
            );
        }

        group_container = group_container.child(list);
        container = container.child(group_container);
    }

    container.into_any_element()
}
