//! `AppSettings` 数据模型与默认值（轮 8a/8b 重构抽取）。
//!
//! 轮 8a：搬入数据形状（struct + Default + Global）。
//! 轮 8b：搬入 `impl AppSettings { ... }` 运行时方法（global/load/save、
//!        主题应用、窗口几何、自动保存同步等）。
//!
//! 通过父模块 `pub(crate) use app_settings::AppSettings;` re-export，
//! 维持外部 14 处 `crate::setting_tab::AppSettings` 引用路径不变。

use std::path::PathBuf;

use db_view::DbViewSettings;
use gpui::{
    App, Pixels, SharedString, Size, Window, WindowAppearance, WindowBackgroundAppearance,
    WindowBounds, px,
};
use gpui_component::{Theme, ThemeMode, ThemeRegistry};
use one_core::storage::get_config_dir;
use one_core::utils::auto_save_config::AutoSaveConfig;
use serde::{Deserialize, Serialize};
use terminal_view::{MAX_RECOVERY_SCROLLBACK_LINES, set_recovery_scrollback_lines};
use tracing::{error, info};

use super::cloud::{GistSettings, GoogleDriveSettings, OneDriveSettings, WebDavSettings};
use super::migrations::sync_follow_app_terminal_themes;
use super::proxy::GlobalProxySettings;
#[cfg(target_os = "linux")]
use super::resolve_linux_window_appearance_override;
use super::saved_window::{SavedWindowBounds, centered_window_bounds_within_visible_area};
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

impl AppSettings {
    pub fn global(cx: &App) -> &AppSettings {
        cx.global::<AppSettings>()
    }

    pub fn global_mut(cx: &mut App) -> &mut AppSettings {
        cx.global_mut::<AppSettings>()
    }

    pub(crate) fn current_system_hotkey(&self) -> &str {
        #[cfg(target_os = "macos")]
        {
            &self.system_hotkey_macos
        }

        #[cfg(not(target_os = "macos"))]
        {
            &self.system_hotkey_other
        }
    }

    fn config_path() -> Option<PathBuf> {
        get_config_dir().ok().map(|dir| dir.join("settings.json"))
    }

    fn write_to_disk(&self) {
        let Some(path) = Self::config_path() else {
            error!("Could not determine config path");
            return;
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                error!("Failed to create config directory: {}", e);
                return;
            }
        }

        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    error!("Failed to write settings file: {}", e);
                } else {
                    info!("Settings saved to {:?}", path);
                }
            }
            Err(e) => {
                error!("Failed to serialize settings: {}", e);
            }
        }
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(settings) => {
                    info!("Settings loaded from {:?}", path);
                    settings
                }
                Err(e) => {
                    error!("Failed to parse settings: {}", e);
                    Self::default()
                }
            },
            Err(e) => {
                error!("Failed to read settings file: {}", e);
                Self::default()
            }
        }
    }

    pub fn save(&mut self) {
        self.write_to_disk();
    }

    pub fn snapshot_main_window_bounds(window: &Window) -> Option<SavedWindowBounds> {
        SavedWindowBounds::from_window_bounds(window.window_bounds())
    }

    pub fn set_global_main_window_bounds(
        saved_window_bounds: SavedWindowBounds,
        cx: &mut App,
    ) -> bool {
        let settings = Self::global_mut(cx);
        if settings.main_window_bounds == Some(saved_window_bounds) {
            return false;
        }

        settings.main_window_bounds = Some(saved_window_bounds);
        true
    }

    pub fn restored_main_window_bounds(
        &self,
        default_size: Size<Pixels>,
        cx: &App,
    ) -> WindowBounds {
        self.main_window_bounds
            .and_then(|saved_window_bounds| saved_window_bounds.to_restored_window_bounds(cx))
            .unwrap_or_else(|| {
                centered_window_bounds_within_visible_area(
                    default_size,
                    cx.primary_display().map(|display| display.visible_bounds()),
                )
            })
    }

    pub fn preferred_window_background(&self) -> WindowBackgroundAppearance {
        // macOS 原生 Vibrancy 始终启用，与毛玻璃开关无关
        // 毛玻璃开关仅控制应用层 UI 颜色的 frosted 效果
        #[cfg(target_os = "macos")]
        {
            return WindowBackgroundAppearance::Blurred;
        }

        if !self.enable_glass_effect {
            return WindowBackgroundAppearance::Opaque;
        }

        #[cfg(target_os = "linux")]
        {
            return WindowBackgroundAppearance::Blurred;
        }

        #[cfg(target_os = "windows")]
        {
            // 使用 Blurred (Acrylic) 而非 MicaAltBackdrop：
            // - Acrylic 通过 SetWindowCompositionAttribute 实现，兼容性更广
            // - MicaAltBackdrop 在某些 Windows 环境下可能静默失败
            // - Blurred 与 macOS/Linux 的透明+模糊行为更一致
            return WindowBackgroundAppearance::Blurred;
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            return WindowBackgroundAppearance::Opaque;
        }
    }

    pub fn window_corner_radius(&self) -> Pixels {
        px((self.radius + 2.0) as f32)
    }

    pub fn save_global(cx: &mut App) {
        if !cx.has_global::<AppSettings>() {
            return;
        }

        Self::global_mut(cx).save();
    }

    pub(super) fn theme_preference_value(&self) -> String {
        if self.auto_switch_theme {
            "auto".to_string()
        } else if self.theme_mode == "dark" {
            "dark".to_string()
        } else {
            "light".to_string()
        }
    }

    pub(super) fn set_theme_preference(&mut self, value: &str) {
        match value {
            "auto" => {
                self.auto_switch_theme = true;
            }
            "dark" => {
                self.auto_switch_theme = false;
                self.theme_mode = "dark".to_string();
            }
            _ => {
                self.auto_switch_theme = false;
                self.theme_mode = "light".to_string();
            }
        }
    }

    pub(super) fn apply_ui_font_preferences(
        font_family: impl Into<SharedString>,
        font_size: f64,
        cx: &mut App,
    ) {
        {
            let theme = Theme::global_mut(cx);
            theme.font_family = font_family.into();
            theme.font_size = px(theme_utils::clamp_ui_font_size(font_size));
        }
        cx.refresh_windows();
    }

    fn manual_theme_mode(&self) -> ThemeMode {
        if self.theme_mode == "dark" {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }

    pub(super) fn effective_theme_mode(&self, appearance: WindowAppearance) -> ThemeMode {
        if self.auto_switch_theme {
            appearance.into()
        } else {
            self.manual_theme_mode()
        }
    }

    pub fn resolve_system_appearance(window: Option<&Window>, cx: &mut App) -> WindowAppearance {
        #[cfg(target_os = "linux")]
        if let Some(appearance) = resolve_linux_window_appearance_override() {
            return appearance;
        }

        window
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
    }

    pub fn apply_theme_preferences(&self, window: Option<&mut Window>, cx: &mut App) {
        let appearance = Self::resolve_system_appearance(window.as_deref(), cx);
        let mode = self.effective_theme_mode(appearance);

        // 当 effective mode 改变时，如果当前 theme_name 不匹配新模式，
        // 尝试找同名变体（Light <-> Dark），找不到则回退到默认主题。
        let effective_theme_name = if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(self.theme_name.as_str())
        {
            if theme_config.mode != mode {
                Self::find_matching_theme_name(&self.theme_name, mode, cx).unwrap_or_else(|| {
                    if mode.is_dark() {
                        ThemeRegistry::global(cx)
                            .default_dark_theme()
                            .name
                            .to_string()
                    } else {
                        ThemeRegistry::global(cx)
                            .default_light_theme()
                            .name
                            .to_string()
                    }
                })
            } else {
                self.theme_name.clone()
            }
        } else {
            self.theme_name.clone()
        };

        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(effective_theme_name.as_str())
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme_config);
        }

        // 如果有效主题名与当前设置不同（模式切换导致主题名改变），同步更新设置
        if effective_theme_name != self.theme_name {
            AppSettings::global_mut(cx).theme_name = effective_theme_name.clone();
            AppSettings::global_mut(cx).save();
        }

        Theme::set_window_surface_preferences(
            self.enable_glass_effect,
            self.ui_surface_opacity,
            self.backdrop_opacity,
            cx,
        );
        Theme::change(mode, window, cx);
        Self::apply_ui_font_preferences(self.font_family.clone(), self.font_size, cx);
        self.apply_misc_appearance_preferences(cx);
        self.apply_window_background_preferences(cx);
        sync_follow_app_terminal_themes(cx);
        cx.refresh_windows();
    }

    fn find_matching_theme_name(current: &str, mode: ThemeMode, cx: &App) -> Option<String> {
        if let Some(theme) = ThemeRegistry::global(cx).get_by_name(current) {
            if theme.mode == mode {
                return Some(current.to_string());
            }
        }

        let candidate = if mode.is_dark() {
            current.replace("Light", "Dark")
        } else {
            current.replace("Dark", "Light")
        };

        if let Some(theme) = ThemeRegistry::global(cx).themes().get(candidate.as_str()) {
            if theme.mode == mode {
                return Some(candidate);
            }
        }

        None
    }

    fn apply_misc_appearance_preferences(&self, cx: &mut App) {
        let scrollbar_show = match self.scrollbar_show.as_str() {
            "scrolling" => gpui_component::scroll::ScrollbarShow::Scrolling,
            "always" => gpui_component::scroll::ScrollbarShow::Always,
            _ => gpui_component::scroll::ScrollbarShow::Hover,
        };

        let theme = Theme::global_mut(cx);
        theme.scrollbar_show = scrollbar_show;
        theme.mono_font_family = self.mono_font_family.clone().into();
        theme.radius = px(self.radius as f32);
        theme.radius_lg = self.window_corner_radius();
        theme.shadow = self.shadow;
    }

    fn apply_window_background_preferences(&self, cx: &mut App) {
        let background = self.preferred_window_background();
        let corner_radius = self.window_corner_radius();
        for window_handle in cx.windows() {
            let _ = window_handle.update(cx, |_, window, _| {
                window.set_blur_behind_corner_radius(corner_radius);
                window.set_background_appearance(background);
                window.refresh();
            });
        }
    }

    pub fn apply(&self, cx: &mut App) {
        gpui_component::set_locale(&self.locale);
        self.apply_theme_preferences(None, cx);
        set_recovery_scrollback_lines(cx, self.normalized_terminal_recovery_scrollback_lines());

        // 同步自动保存配置
        self.sync_auto_save_config(cx);
        self.sync_db_view_settings(cx);
    }

    pub(super) fn normalized_terminal_recovery_scrollback_lines(&self) -> usize {
        self.terminal_recovery_scrollback_lines
            .clamp(0.0, MAX_RECOVERY_SCROLLBACK_LINES as f64)
            .round() as usize
    }

    /// 同步自动保存配置到全局状态
    pub fn sync_auto_save_config(&self, cx: &mut App) {
        Self::update_auto_save_config(self.enable_sql_auto_save, self.sql_auto_save_interval, cx);
    }

    pub fn sync_db_view_settings(&self, cx: &mut App) {
        db_view::init_db_view_settings(
            cx,
            DbViewSettings {
                db_undo_stack_size: self.db_undo_stack_size,
                large_text_editor_open_mode: self.large_text_cell_editor_open_mode.into(),
            },
        );
    }

    /// 更新自动保存配置（静态方法，避免借用冲突）
    pub fn update_auto_save_config(enabled: bool, interval_seconds: f64, cx: &mut App) {
        if let Some(config) = cx.try_global::<AutoSaveConfig>() {
            config.set_enabled(enabled);
            config.set_interval_seconds(interval_seconds);
        }
    }

    pub(crate) fn reload_global_from_disk(cx: &mut App) -> AppSettings {
        let settings = Self::load();
        settings.apply(cx);

        if cx.has_global::<AppSettings>() {
            let global = Self::global_mut(cx);
            *global = settings.clone();
        } else {
            cx.set_global(settings.clone());
        }

        settings
    }
}
