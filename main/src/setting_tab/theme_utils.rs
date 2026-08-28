//! 设置项默认值与数值归一化辅助。
//!
//! 抽取自 `setting_tab.rs`（轮 7 重构）。本模块只包含数据层的纯函数：
//! - `default_*`：serde 反序列化与 `AppSettings::Default` 引用的默认值
//! - `clamp_*`：数值归一化辅助（UI 滑块与 setter 调用）
//!
//! 全部函数标 `pub(super)`，仅供父模块 `setting_tab` 使用。
//! UI 主题化辅助（`themed_setting_field/group/page`、`settings_group_*_style`）
//! 强耦合 `SettingsPanel::render`，留待轮 9 处理。

use gpui_component::{MAX_GLASS_OPACITY, MIN_GLASS_OPACITY};
use terminal_view::{DEFAULT_LINE_HEIGHT_SCALE, DEFAULT_RECOVERY_SCROLLBACK_LINES};

pub(super) fn default_font_family() -> String {
    "Arial".to_string()
}

pub(super) fn default_font_size() -> f64 {
    14.0
}

pub(super) fn clamp_ui_font_size(size: f64) -> f32 {
    size.clamp(12.0, 32.0) as f32
}

pub(super) fn default_ui_surface_opacity() -> f64 {
    1.0
}

pub(super) fn clamp_ui_surface_opacity(opacity: f64) -> f64 {
    opacity.clamp(MIN_GLASS_OPACITY as f64, MAX_GLASS_OPACITY as f64)
}

pub(super) fn default_backdrop_opacity() -> f64 {
    1.0
}

pub(super) fn clamp_backdrop_opacity(opacity: f64) -> f64 {
    opacity.clamp(MIN_GLASS_OPACITY as f64, MAX_GLASS_OPACITY as f64)
}

pub(super) fn default_terminal_font_size() -> f64 {
    15.0
}

pub(super) fn default_terminal_font_family() -> String {
    terminal_view::theme::default_monospace_font().to_string()
}

pub(super) fn default_terminal_line_height_scale() -> f64 {
    DEFAULT_LINE_HEIGHT_SCALE as f64
}

pub(super) fn default_theme_name() -> String {
    "Default Light".to_string()
}

pub(super) fn default_scrollbar_show() -> String {
    "hover".to_string()
}

pub(super) fn default_mono_font_family() -> String {
    if cfg!(target_os = "macos") {
        "Menlo".to_string()
    } else if cfg!(target_os = "windows") {
        "Consolas".to_string()
    } else {
        "DejaVu Sans Mono".to_string()
    }
}

pub(super) fn default_radius() -> f64 {
    6.0
}

pub(super) fn default_terminal_theme() -> String {
    "ocean".to_string()
}

pub(super) fn default_terminal_recovery_scrollback_lines() -> f64 {
    DEFAULT_RECOVERY_SCROLLBACK_LINES as f64
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_auto_save_interval() -> f64 {
    5.0
}

pub(super) fn default_db_undo_stack_size() -> usize {
    50
}

pub(super) fn default_sync_backend_type() -> String {
    "sync_server".to_string()
}

pub(super) const DEFAULT_SQL_QUERY_MAX_ROWS: u32 = 1000;

pub(super) fn default_sql_query_max_rows() -> u32 {
    DEFAULT_SQL_QUERY_MAX_ROWS
}
