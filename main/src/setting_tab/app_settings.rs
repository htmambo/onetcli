//! `AppSettings` 数据模型与默认值（轮 8a 重构抽取）。
//!
//! 仅包含数据形状与序列化默认值，无运行时方法 / GPUI 初始化逻辑。
//! `impl AppSettings { ... }` 行为层与各 setter 留在父模块或后续子轮处理。
//!
//! 通过父模块 `pub(crate) use app_settings::AppSettings;` re-export，
//! 维持外部 14 处 `crate::setting_tab::AppSettings` 引用路径不变。

use serde::{Deserialize, Serialize};

use super::cloud::{GistSettings, GoogleDriveSettings, OneDriveSettings, WebDavSettings};
use super::proxy::GlobalProxySettings;
use super::saved_window::SavedWindowBounds;
use super::types::{
    ConnectionListSortField, ConnectionListSortOrder, ConnectionListViewMode, DatabaseOpenMode,
    LargeTextCellEditorOpenMode,
};
use super::{hotkey, theme_utils};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub theme_mode: String,
    #[serde(default)]
    pub auto_switch_theme: bool,
    #[serde(default = "theme_utils::default_true")]
    pub enable_glass_effect: bool,
    #[serde(default = "theme_utils::default_ui_surface_opacity")]
    pub ui_surface_opacity: f64,
    #[serde(default = "theme_utils::default_backdrop_opacity")]
    pub backdrop_opacity: f64,
    #[serde(default = "theme_utils::default_font_family")]
    pub font_family: String,
    #[serde(default = "theme_utils::default_font_size")]
    pub font_size: f64,
    #[serde(default = "theme_utils::default_terminal_font_size")]
    pub terminal_font_size: f64,
    #[serde(default = "theme_utils::default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default)]
    pub terminal_font_ligatures: bool,
    #[serde(default = "theme_utils::default_terminal_line_height_scale")]
    pub terminal_line_height_scale: f64,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_auto_copy: bool,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_enable_autocomplete: bool,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_middle_click_paste: bool,
    #[serde(default)]
    pub terminal_sync_path_with_terminal: bool,
    #[serde(default = "theme_utils::default_theme_name")]
    pub theme_name: String,
    #[serde(default = "theme_utils::default_scrollbar_show")]
    pub scrollbar_show: String,
    #[serde(default = "theme_utils::default_mono_font_family")]
    pub mono_font_family: String,
    #[serde(default = "theme_utils::default_radius")]
    pub radius: f64,
    #[serde(default = "theme_utils::default_true")]
    pub shadow: bool,
    #[serde(default = "theme_utils::default_terminal_theme")]
    pub terminal_theme: String,
    #[serde(default)]
    pub terminal_cursor_blink: bool,
    #[serde(default = "theme_utils::default_terminal_recovery_scrollback_lines")]
    pub terminal_recovery_scrollback_lines: f64,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_confirm_multiline_paste: bool,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_confirm_high_risk_command: bool,
    #[serde(default = "theme_utils::default_true")]
    pub terminal_check_running_processes_on_exit: bool,
    #[serde(default)]
    pub log_file_path: String,
    #[serde(default = "theme_utils::default_true")]
    pub restore_connections_on_startup: bool,
    #[serde(default = "theme_utils::default_true")]
    pub restore_session_content: bool,
    #[serde(default = "theme_utils::default_true")]
    pub auto_update: bool,
    #[serde(default)]
    pub sync_server_url: String,
    /// 同步后端类型："sync_server" | "webdav"
    #[serde(default = "theme_utils::default_sync_backend_type")]
    pub sync_backend_type: String,
    /// WebDAV 配置
    #[serde(default)]
    pub webdav_config: Option<WebDavSettings>,
    /// GitHub Gist 配置
    #[serde(default)]
    pub gist_config: Option<GistSettings>,
    /// Google Drive 配置
    #[serde(default)]
    pub google_drive_config: Option<GoogleDriveSettings>,
    /// OneDrive 配置
    #[serde(default)]
    pub onedrive_config: Option<OneDriveSettings>,
    #[serde(default)]
    pub global_proxy: GlobalProxySettings,
    #[serde(default)]
    pub database_open_mode: DatabaseOpenMode,
    #[serde(default)]
    pub large_text_cell_editor_open_mode: LargeTextCellEditorOpenMode,
    #[serde(default)]
    pub connection_list_sort_field: ConnectionListSortField,
    #[serde(default)]
    pub connection_list_sort_order: ConnectionListSortOrder,
    #[serde(default)]
    pub connection_list_view_mode: ConnectionListViewMode,
    #[serde(default)]
    pub main_window_bounds: Option<SavedWindowBounds>,
    /// 是否启用SQL查询的自动保存功能
    #[serde(default = "theme_utils::default_true")]
    pub enable_sql_auto_save: bool,
    /// SQL查询自动保存的间隔（秒），默认5秒
    #[serde(default = "theme_utils::default_auto_save_interval")]
    pub sql_auto_save_interval: f64,
    /// 数据库编辑器撤销栈容量，0表示禁用逐步撤销
    #[serde(default = "theme_utils::default_db_undo_stack_size")]
    pub db_undo_stack_size: usize,
    /// 是否使用 AI 自动生成会话标题
    #[serde(default)]
    pub ai_auto_generate_session_title: bool,
    #[serde(default = "hotkey::default_system_hotkey_macos")]
    pub system_hotkey_macos: String,
    #[serde(default = "hotkey::default_system_hotkey_other")]
    pub system_hotkey_other: String,
    /// SSH 自动接受新密钥（密钥变更时自动替换）
    #[serde(default)]
    pub ssh_auto_accept_new_keys: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: "zh-CN".to_string(),
            theme_mode: "auto".to_string(),
            auto_switch_theme: false,
            enable_glass_effect: theme_utils::default_true(),
            ui_surface_opacity: theme_utils::default_ui_surface_opacity(),
            backdrop_opacity: theme_utils::default_backdrop_opacity(),
            font_family: theme_utils::default_font_family(),
            font_size: theme_utils::default_font_size(),
            terminal_font_size: theme_utils::default_terminal_font_size(),
            terminal_font_family: theme_utils::default_terminal_font_family(),
            terminal_font_ligatures: false,
            terminal_line_height_scale: theme_utils::default_terminal_line_height_scale(),
            terminal_auto_copy: theme_utils::default_true(),
            terminal_enable_autocomplete: theme_utils::default_true(),
            terminal_middle_click_paste: theme_utils::default_true(),
            terminal_sync_path_with_terminal: false,
            theme_name: theme_utils::default_theme_name(),
            scrollbar_show: theme_utils::default_scrollbar_show(),
            mono_font_family: theme_utils::default_mono_font_family(),
            radius: theme_utils::default_radius(),
            shadow: true,
            terminal_theme: theme_utils::default_terminal_theme(),
            terminal_cursor_blink: false,
            terminal_recovery_scrollback_lines:
                theme_utils::default_terminal_recovery_scrollback_lines(),
            terminal_confirm_multiline_paste: theme_utils::default_true(),
            terminal_confirm_high_risk_command: theme_utils::default_true(),
            terminal_check_running_processes_on_exit: theme_utils::default_true(),
            restore_connections_on_startup: theme_utils::default_true(),
            restore_session_content: theme_utils::default_true(),
            log_file_path: String::new(),
            auto_update: true,
            sync_server_url: String::new(),
            sync_backend_type: theme_utils::default_sync_backend_type(),
            webdav_config: None,
            gist_config: None,
            google_drive_config: None,
            onedrive_config: None,
            global_proxy: GlobalProxySettings::default(),
            database_open_mode: DatabaseOpenMode::default(),
            large_text_cell_editor_open_mode: LargeTextCellEditorOpenMode::default(),
            connection_list_sort_field: ConnectionListSortField::default(),
            connection_list_sort_order: ConnectionListSortOrder::default(),
            connection_list_view_mode: ConnectionListViewMode::default(),
            main_window_bounds: None,
            enable_sql_auto_save: true,
            sql_auto_save_interval: theme_utils::default_auto_save_interval(),
            db_undo_stack_size: theme_utils::default_db_undo_stack_size(),
            ai_auto_generate_session_title: false,
            system_hotkey_macos: hotkey::default_system_hotkey_macos(),
            system_hotkey_other: hotkey::default_system_hotkey_other(),
            ssh_auto_accept_new_keys: false,
        }
    }
}

impl gpui::Global for AppSettings {}
