//! 设置项相关的迁移与同步辅助函数。
//!
//! 抽取自 `setting_tab.rs`（轮 8c 重构）。涵盖：
//! - 旧版热键自动迁移（`HotkeyMigration` 与 `migrate_legacy_system_hotkey`）
//! - 旧版主题状态迁移（`migrate_legacy_theme_state`）
//! - 终端设置同步（`sync_terminal_settings_to_all` / `sync_follow_app_terminal_themes` / `legacy_terminal_settings`）
//! - 同步服务器 URL 归一化（`editable_sync_server_url` / `normalize_sync_server_url`）
//!
//! `HotkeyMigration` 通过父模块以 `pub(crate) use` 重导出，维持
//! `crate::setting_tab::HotkeyMigration` 外部引用路径不变。

use gpui::{App, AppContext};
use one_core::cloud_sync::sync_server::SyncServerClient;
use terminal_view::settings::{GlobalTerminalSettings, TerminalSettingsStore};
use terminal_view::{TerminalSettings, set_recovery_scrollback_lines};

use super::app_settings::AppSettings;
use super::hotkey::{DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};
use super::theme_utils;
use crate::onetcli_app::GlobalHomePage;

/// 旧版系统级激活热键 `ctrl-space` 与系统输入法切换冲突，启动时按平台迁移为新默认值。
#[derive(Debug, Clone, Copy, Default)]
pub struct HotkeyMigration {
    pub macos_changed: bool,
    pub other_changed: bool,
}

impl HotkeyMigration {
    pub fn any_changed(self) -> bool {
        self.macos_changed || self.other_changed
    }
}

pub(super) fn migrate_legacy_system_hotkey(settings: &mut AppSettings) -> HotkeyMigration {
    let mut migration = HotkeyMigration::default();
    if is_legacy_ctrl_space(&settings.system_hotkey_macos) {
        settings.system_hotkey_macos = DEFAULT_SYSTEM_HOTKEY_MACOS.to_string();
        migration.macos_changed = true;
    }
    if is_legacy_ctrl_space(&settings.system_hotkey_other) {
        settings.system_hotkey_other = DEFAULT_SYSTEM_HOTKEY_OTHER.to_string();
        migration.other_changed = true;
    }
    migration
}

pub(super) fn is_legacy_ctrl_space(spec: &str) -> bool {
    spec.trim().eq_ignore_ascii_case("ctrl-space")
}

pub(super) fn migrate_legacy_theme_state(settings: &mut AppSettings) {
    const LEGACY_STATE_FILE: &str = "target/state.json";
    if settings.theme_name != theme_utils::default_theme_name()
        && settings.scrollbar_show != theme_utils::default_scrollbar_show()
    {
        return;
    }
    let Ok(content) = std::fs::read_to_string(LEGACY_STATE_FILE) else {
        return;
    };
    #[derive(Debug, Clone, serde::Deserialize)]
    struct LegacyState {
        theme: Option<String>,
        scrollbar_show: Option<String>,
    }
    if let Ok(legacy) = serde_json::from_str::<LegacyState>(&content) {
        if settings.theme_name == theme_utils::default_theme_name() {
            if let Some(theme) = legacy.theme.filter(|t| !t.is_empty()) {
                settings.theme_name = theme;
            }
        }
        if settings.scrollbar_show == theme_utils::default_scrollbar_show() {
            if let Some(sb) = legacy.scrollbar_show.filter(|t| !t.is_empty()) {
                settings.scrollbar_show = sb;
            }
        }
    }
}

pub(super) fn sync_terminal_settings_to_all(settings: AppSettings, cx: &mut App) {
    set_recovery_scrollback_lines(cx, settings.normalized_terminal_recovery_scrollback_lines());

    // 更新 GlobalTerminalSettings
    if let Some(global) = cx.try_global::<GlobalTerminalSettings>() {
        let store = global.0.clone();
        store.update(cx, |store: &mut TerminalSettingsStore, cx| {
            let mut next = store.snapshot();
            next.check_running_processes_on_exit =
                settings.terminal_check_running_processes_on_exit;
            store.replace(next, cx);
        });
    }

    let Some(home) = cx.try_global::<GlobalHomePage>() else {
        return;
    };
    let Some(window_id) = cx.active_window() else {
        return;
    };
    let home_page = home.home_page.clone();
    let _ = cx.update_window(window_id, move |_, window, cx| {
        home_page.update(cx, |hp, cx| {
            hp.apply_terminal_settings_to_all(&settings, window, cx);
        });
    });
}

pub(super) fn sync_follow_app_terminal_themes(cx: &mut App) {
    let settings = AppSettings::global(cx).clone();
    cx.defer(move |cx| {
        let Some(home) = cx.try_global::<GlobalHomePage>() else {
            return;
        };
        let Some(window_id) = cx.active_window() else {
            return;
        };

        // 避免在 HomePage 自己的 update 调用栈里再次触发 home_page.update，
        // 否则启动阶段会命中 gpui 的重入保护并直接 panic。
        let home_page = home.home_page.clone();
        let _ = cx.update_window(window_id, move |_, window, cx| {
            home_page.update(cx, |hp, cx| {
                hp.apply_app_settings(&settings, window, cx);
            });
        });
    });
}

pub(super) fn legacy_terminal_settings(settings: &AppSettings) -> TerminalSettings {
    TerminalSettings {
        font_size: settings.terminal_font_size as f32,
        auto_copy: settings.terminal_auto_copy,
        enable_autocomplete: settings.terminal_enable_autocomplete,
        middle_click_paste: settings.terminal_middle_click_paste,
        sync_path_with_terminal: settings.terminal_sync_path_with_terminal,
        theme: settings.terminal_theme.clone(),
        cursor_blink: settings.terminal_cursor_blink,
        confirm_multiline_paste: settings.terminal_confirm_multiline_paste,
        confirm_high_risk_command: settings.terminal_confirm_high_risk_command,
        vim_scroll_to_arrow_keys: true,
        builtin_highlights_initialized: false,
        custom_highlights: Vec::new(),
        check_running_processes_on_exit: settings.terminal_check_running_processes_on_exit,
    }
}

pub(super) fn editable_sync_server_url(value: &str) -> String {
    value.trim().to_string()
}

pub(super) fn normalize_sync_server_url(value: &str) -> String {
    SyncServerClient::normalize_base_url(value)
}
