use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::Arc;

use db_view::{DbViewSettings, set_db_view_settings};
use gpui::http_client::{AsyncBody, Method, Request};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Axis, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, Keystroke, ParentElement,
    Pixels, Render, SharedString, StyleRefinement, Styled, WeakEntity, Window, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, div, px,
};
#[cfg(target_os = "linux")]
use gpui_component::linux_prefers_system_window_controls;
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, IndexPath, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY,
    Sizable, Size, Theme, ThemeMode, ThemeRegistry, TitleBar, WindowExt, WindowsSurfaceLayer,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    group_box::GroupBoxVariant,
    h_flex,
    input::{Input, InputState},
    kbd::Kbd,
    layered_level_surface_color,
    scroll::ScrollableElement,
    select::{Select, SelectItem, SelectState},
    setting::{
        NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem,
        SettingPage, Settings,
    },
    switch::Switch,
    tokens::Radius,
    v_flex,
};
use one_core::ai_chat::GlobalChatSettings;
use one_core::certificate_manager::CertificateManagerView;
use one_core::cloud_sync::{UserInfo, sync_server::SyncServerClient};
use one_core::gpui_tokio::Tokio;
use one_core::llm::manager::GlobalProviderState;
use one_core::popup_window::{PopupWindowOptions, open_popup_window};
use one_core::storage::get_config_dir;
use one_core::tab_container::{TabContent, TabContentEvent};
use one_core::utils::auto_save_config::AutoSaveConfig;
use reqwest_client::ReqwestClient;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use terminal_view::{
    DEFAULT_LINE_HEIGHT_SCALE, DEFAULT_RECOVERY_SCROLLBACK_LINES, MAX_LINE_HEIGHT_SCALE,
    MAX_RECOVERY_SCROLLBACK_LINES, MIN_LINE_HEIGHT_SCALE, TerminalSettings, TerminalTheme,
    set_recovery_scrollback_lines,
    settings::{GlobalTerminalSettings, TerminalSettingsStore},
};
use tracing::{error, info};

use crate::app_init::is_valid_system_hotkey;
use crate::auth::{PasswordAuthAction, get_auth_service};
use crate::onetcli_app::GlobalHomePage;
use crate::settings::{github_auth_dialog::GithubAuthDialog, llm_providers_view::LlmProvidersView};
use crate::sync_server_theme;
use crate::update;

mod cloud;
mod global_user;
mod hotkey;
mod proxy;
mod saved_window;
mod types;

pub(crate) use cloud::{GistSettings, GoogleDriveSettings, OneDriveSettings, WebDavSettings};
pub(crate) use global_user::GlobalCurrentUser;
use global_user::PendingSettingsPanelPage;
pub(crate) use hotkey::{DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};
pub(crate) use proxy::{GlobalProxySettings, ProxyType};
pub(crate) use saved_window::SavedWindowBounds;
// `SavedWindowDisplayState` 仅供同模块测试用，加 `#[allow]` 规避非测试构建下的
// `unused_imports` 告警。
#[allow(unused_imports)]
pub(crate) use saved_window::SavedWindowDisplayState;
use saved_window::centered_window_bounds_within_visible_area;
pub(crate) use types::{
    ConnectionListSortField, ConnectionListSortOrder, ConnectionListViewMode, DatabaseOpenMode,
    LargeTextCellEditorOpenMode, SettingsPanelPage,
};

// ============================================================================
// 设置面板页面
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub theme_mode: String,
    #[serde(default)]
    pub auto_switch_theme: bool,
    #[serde(default = "default_true")]
    pub enable_glass_effect: bool,
    #[serde(default = "default_ui_surface_opacity")]
    pub ui_surface_opacity: f64,
    #[serde(default = "default_backdrop_opacity")]
    pub backdrop_opacity: f64,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: f64,
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default)]
    pub terminal_font_ligatures: bool,
    #[serde(default = "default_terminal_line_height_scale")]
    pub terminal_line_height_scale: f64,
    #[serde(default = "default_true")]
    pub terminal_auto_copy: bool,
    #[serde(default = "default_true")]
    pub terminal_enable_autocomplete: bool,
    #[serde(default = "default_true")]
    pub terminal_middle_click_paste: bool,
    #[serde(default)]
    pub terminal_sync_path_with_terminal: bool,
    #[serde(default = "default_theme_name")]
    pub theme_name: String,
    #[serde(default = "default_scrollbar_show")]
    pub scrollbar_show: String,
    #[serde(default = "default_mono_font_family")]
    pub mono_font_family: String,
    #[serde(default = "default_radius")]
    pub radius: f64,
    #[serde(default = "default_true")]
    pub shadow: bool,
    #[serde(default = "default_terminal_theme")]
    pub terminal_theme: String,
    #[serde(default)]
    pub terminal_cursor_blink: bool,
    #[serde(default = "default_terminal_recovery_scrollback_lines")]
    pub terminal_recovery_scrollback_lines: f64,
    #[serde(default = "default_true")]
    pub terminal_confirm_multiline_paste: bool,
    #[serde(default = "default_true")]
    pub terminal_confirm_high_risk_command: bool,
    #[serde(default = "default_true")]
    pub terminal_check_running_processes_on_exit: bool,
    #[serde(default)]
    pub log_file_path: String,
    #[serde(default = "default_true")]
    pub restore_connections_on_startup: bool,
    #[serde(default = "default_true")]
    pub restore_session_content: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,
    #[serde(default)]
    pub sync_server_url: String,
    /// 同步后端类型："sync_server" | "webdav"
    #[serde(default = "default_sync_backend_type")]
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
    #[serde(default = "default_true")]
    pub enable_sql_auto_save: bool,
    /// SQL查询自动保存的间隔（秒），默认5秒
    #[serde(default = "default_auto_save_interval")]
    pub sql_auto_save_interval: f64,
    /// 数据库编辑器撤销栈容量，0表示禁用逐步撤销
    #[serde(default = "default_db_undo_stack_size")]
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

fn default_font_family() -> String {
    "Arial".to_string()
}

fn default_font_size() -> f64 {
    14.0
}

fn clamp_ui_font_size(size: f64) -> f32 {
    size.clamp(12.0, 32.0) as f32
}

fn default_ui_surface_opacity() -> f64 {
    1.0
}

fn clamp_ui_surface_opacity(opacity: f64) -> f64 {
    opacity.clamp(MIN_GLASS_OPACITY as f64, MAX_GLASS_OPACITY as f64)
}

fn default_backdrop_opacity() -> f64 {
    1.0
}

fn clamp_backdrop_opacity(opacity: f64) -> f64 {
    opacity.clamp(MIN_GLASS_OPACITY as f64, MAX_GLASS_OPACITY as f64)
}

#[cfg(target_os = "linux")]
fn parse_deepin_theme_appearance(value: &str) -> Option<WindowAppearance> {
    let normalized = value
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim()
        .to_ascii_lowercase();

    if normalized.is_empty() {
        None
    } else if normalized.contains("dark") {
        Some(WindowAppearance::Dark)
    } else {
        Some(WindowAppearance::Light)
    }
}

#[cfg(target_os = "linux")]
fn read_command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

#[cfg(target_os = "linux")]
fn read_gsettings_string(schema: &str, key: &str) -> Option<String> {
    read_command_stdout("gsettings", &["get", schema, key])
}

#[cfg(target_os = "linux")]
fn read_deepin_appearance_property(property: &str) -> Option<String> {
    read_command_stdout(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.deepin.dde.Appearance1",
            "--object-path",
            "/org/deepin/dde/Appearance1",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.deepin.dde.Appearance1",
            property,
        ],
    )
}

#[cfg(target_os = "linux")]
fn resolve_deepin_window_appearance() -> Option<WindowAppearance> {
    if !linux_prefers_system_window_controls() {
        return None;
    }

    ["GlobalTheme", "GtkTheme"]
        .into_iter()
        .find_map(|property| {
            read_deepin_appearance_property(property)
                .and_then(|value| parse_deepin_theme_appearance(&value))
        })
        .or_else(|| {
            [
                ("com.deepin.xsettings", "theme-name"),
                ("com.deepin.xsettings", "gtk-theme-name"),
                ("com.deepin.dde.appearance", "gtk-theme"),
            ]
            .into_iter()
            .find_map(|(schema, key)| {
                read_gsettings_string(schema, key)
                    .and_then(|value| parse_deepin_theme_appearance(&value))
            })
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn resolve_linux_window_appearance_override() -> Option<WindowAppearance> {
    resolve_deepin_window_appearance()
}

fn default_terminal_font_size() -> f64 {
    15.0
}

fn default_terminal_font_family() -> String {
    terminal_view::theme::default_monospace_font().to_string()
}

fn default_terminal_line_height_scale() -> f64 {
    DEFAULT_LINE_HEIGHT_SCALE as f64
}

fn default_theme_name() -> String {
    "Default Light".to_string()
}

fn default_scrollbar_show() -> String {
    "hover".to_string()
}

fn default_mono_font_family() -> String {
    if cfg!(target_os = "macos") {
        "Menlo".to_string()
    } else if cfg!(target_os = "windows") {
        "Consolas".to_string()
    } else {
        "DejaVu Sans Mono".to_string()
    }
}

fn default_radius() -> f64 {
    6.0
}

fn default_terminal_theme() -> String {
    "ocean".to_string()
}

fn default_terminal_recovery_scrollback_lines() -> f64 {
    DEFAULT_RECOVERY_SCROLLBACK_LINES as f64
}

fn default_true() -> bool {
    true
}

fn default_auto_save_interval() -> f64 {
    5.0
}

fn default_db_undo_stack_size() -> usize {
    50
}

fn themed_setting_field<T>(field: SettingField<T>) -> SettingField<T> {
    field
        .bg(sync_server_theme::surface_alt())
        .border_color(sync_server_theme::border_strong())
        .text_color(sync_server_theme::text())
}

fn settings_group_content_style(cx: &App) -> StyleRefinement {
    let blur_enabled = cx.theme().window_blur_enabled;
    let window_opacity = cx.theme().backdrop_opacity;
    let bg = layered_level_surface_color(
        cx.theme().group,
        blur_enabled,
        window_opacity,
        1,
        WindowsSurfaceLayer::ContentBase,
    );
    sync_server_theme::surface_style()
        .rounded(Radius::Xl.px())
        .bg(bg)
}

fn settings_group_title_style() -> StyleRefinement {
    StyleRefinement::default().text_color(sync_server_theme::text())
}

fn themed_setting_group(group: SettingGroup, cx: &App) -> SettingGroup {
    group
        .title_style(&settings_group_title_style())
        .content_style(&settings_group_content_style(cx))
}

fn themed_setting_page(page: SettingPage, cx: &App) -> SettingPage {
    let blur_enabled = cx.theme().window_blur_enabled;
    let window_opacity = cx.theme().backdrop_opacity;
    // 与首页右侧标题栏保持同一分层，统一“标题 + 内容区”的壳层视觉。
    let header_bg = layered_level_surface_color(
        cx.theme().background,
        blur_enabled,
        window_opacity,
        1,
        WindowsSurfaceLayer::ContentSection,
    );
    page.header_style(
        &StyleRefinement::default()
            .bg(header_bg)
            .border_color(sync_server_theme::border())
            .text_color(sync_server_theme::text()),
    )
}

fn default_sync_backend_type() -> String {
    "sync_server".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: "zh-CN".to_string(),
            theme_mode: "auto".to_string(),
            auto_switch_theme: false,
            enable_glass_effect: default_true(),
            ui_surface_opacity: default_ui_surface_opacity(),
            backdrop_opacity: default_backdrop_opacity(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            terminal_font_size: default_terminal_font_size(),
            terminal_font_family: default_terminal_font_family(),
            terminal_font_ligatures: false,
            terminal_line_height_scale: default_terminal_line_height_scale(),
            terminal_auto_copy: default_true(),
            terminal_enable_autocomplete: default_true(),
            terminal_middle_click_paste: default_true(),
            terminal_sync_path_with_terminal: false,
            theme_name: default_theme_name(),
            scrollbar_show: default_scrollbar_show(),
            mono_font_family: default_mono_font_family(),
            radius: default_radius(),
            shadow: true,
            terminal_theme: default_terminal_theme(),
            terminal_cursor_blink: false,
            terminal_recovery_scrollback_lines: default_terminal_recovery_scrollback_lines(),
            terminal_confirm_multiline_paste: default_true(),
            terminal_confirm_high_risk_command: default_true(),
            terminal_check_running_processes_on_exit: default_true(),
            restore_connections_on_startup: default_true(),
            restore_session_content: default_true(),
            log_file_path: String::new(),
            auto_update: true,
            sync_server_url: String::new(),
            sync_backend_type: default_sync_backend_type(),
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
            sql_auto_save_interval: default_auto_save_interval(),
            db_undo_stack_size: default_db_undo_stack_size(),
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
        default_size: gpui::Size<Pixels>,
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

    fn theme_preference_value(&self) -> String {
        if self.auto_switch_theme {
            "auto".to_string()
        } else if self.theme_mode == "dark" {
            "dark".to_string()
        } else {
            "light".to_string()
        }
    }

    fn set_theme_preference(&mut self, value: &str) {
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

    fn apply_ui_font_preferences(
        font_family: impl Into<SharedString>,
        font_size: f64,
        cx: &mut App,
    ) {
        {
            let theme = Theme::global_mut(cx);
            theme.font_family = font_family.into();
            theme.font_size = px(clamp_ui_font_size(font_size));
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

    fn effective_theme_mode(&self, appearance: WindowAppearance) -> ThemeMode {
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

    fn normalized_terminal_recovery_scrollback_lines(&self) -> usize {
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

pub fn init_settings(cx: &mut App) -> HotkeyMigration {
    init_settings_with(cx, None)
}

/// 等价 `init_settings`；允许调用方复用已加载的 `AppSettings` 以避免重复文件 I/O + JSON 解析。
/// 当 `preloaded` 为 `None` 时行为与 `init_settings` 完全一致。
pub fn init_settings_with(cx: &mut App, preloaded: Option<AppSettings>) -> HotkeyMigration {
    let mut settings = preloaded.unwrap_or_else(AppSettings::load);
    migrate_legacy_theme_state(&mut settings);
    let hotkey_migration = migrate_legacy_system_hotkey(&mut settings);
    let initial_sync_server_url = settings.sync_server_url.clone();
    terminal_view::init_settings(cx, Some(legacy_terminal_settings(&settings)));
    // 初始化自动保存配置全局状态
    cx.set_global(AutoSaveConfig::new(
        settings.enable_sql_auto_save,
        settings.sql_auto_save_interval,
    ));
    // 初始化聊天全局设置
    cx.set_global(GlobalChatSettings {
        ai_auto_generate_session_title: settings.ai_auto_generate_session_title,
    });
    // apply() 内部可能会写回规范化后的主题设置，因此必须先注册全局状态。
    cx.set_global(settings);
    AppSettings::global(cx).clone().apply(cx);
    if hotkey_migration.any_changed() {
        AppSettings::save_global(cx);
    }
    let _ = get_auth_service(cx).update_sync_server_url(&initial_sync_server_url);
    hotkey_migration
}

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

fn migrate_legacy_system_hotkey(settings: &mut AppSettings) -> HotkeyMigration {
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

fn is_legacy_ctrl_space(spec: &str) -> bool {
    spec.trim().eq_ignore_ascii_case("ctrl-space")
}

fn migrate_legacy_theme_state(settings: &mut AppSettings) {
    const LEGACY_STATE_FILE: &str = "target/state.json";
    if settings.theme_name != default_theme_name()
        && settings.scrollbar_show != default_scrollbar_show()
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
        if settings.theme_name == default_theme_name() {
            if let Some(theme) = legacy.theme.filter(|t| !t.is_empty()) {
                settings.theme_name = theme;
            }
        }
        if settings.scrollbar_show == default_scrollbar_show() {
            if let Some(sb) = legacy.scrollbar_show.filter(|t| !t.is_empty()) {
                settings.scrollbar_show = sb;
            }
        }
    }
}

fn sync_terminal_settings_to_all(settings: AppSettings, cx: &mut App) {
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

fn sync_follow_app_terminal_themes(cx: &mut App) {
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

fn legacy_terminal_settings(settings: &AppSettings) -> TerminalSettings {
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

pub(crate) fn build_app_http_client(
    proxy: &GlobalProxySettings,
) -> Result<Arc<ReqwestClient>, String> {
    let proxy_url = proxy.to_proxy_url()?;
    ReqwestClient::proxy_and_user_agent(proxy_url, "one-hub")
        .map(Arc::new)
        .map_err(|err| format!("HTTP 客户端初始化失败: {}", err))
}

fn editable_sync_server_url(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_sync_server_url(value: &str) -> String {
    SyncServerClient::normalize_base_url(value)
}

fn apply_sync_server_url_setting(value: SharedString, cx: &mut App) {
    let editable = editable_sync_server_url(value.as_ref());
    let normalized = normalize_sync_server_url(&editable);
    let settings_changed = {
        let settings = AppSettings::global_mut(cx);
        if settings.sync_server_url == editable {
            false
        } else {
            settings.sync_server_url = editable.clone();
            settings.save();
            true
        }
    };

    if !settings_changed {
        return;
    }

    let auth_changed = get_auth_service(cx).update_sync_server_url(&normalized);
    if !auth_changed {
        return;
    }

    GlobalCurrentUser::set_user(None, cx);

    if let Some(home) = cx.try_global::<GlobalHomePage>() {
        let home_page = home.home_page.clone();
        home_page.update(cx, |home_page, cx| {
            home_page.handle_sync_server_url_changed(cx);
        });
    }
}

pub struct SettingsPanel {
    focus_handle: FocusHandle,
    certificate_manager_view: Entity<CertificateManagerView>,
    llm_providers_view: Entity<LlmProvidersView>,
    size: Size,
    group_variant: GroupBoxVariant,
    selected_page: SettingsPanelPage,
    state_version: u64,
}

impl SettingsPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let certificate_manager_view = cx.new(|cx| CertificateManagerView::new(cx));
        let llm_providers_view = cx.new(|cx| LlmProvidersView::new(cx));
        let this = Self {
            focus_handle: cx.focus_handle(),
            certificate_manager_view,
            llm_providers_view,
            size: Size::default(),
            group_variant: GroupBoxVariant::Outline,
            selected_page: PendingSettingsPanelPage::take(cx).unwrap_or_default(),
            state_version: 0,
        };
        // 订阅 AppSettings 全局变化，确保外部（如 GitHub 授权弹窗）更新设置后能刷新 UI
        cx.observe_global::<AppSettings>(|_, cx| {
            cx.notify();
        })
        .detach();
        // 订阅 GlobalCurrentUser 变化，确保同步登录/登出后能刷新账号 UI
        cx.observe_global::<GlobalCurrentUser>(|_, cx| {
            cx.notify();
        })
        .detach();
        this
    }

    fn apply_requested_page(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = PendingSettingsPanelPage::take(cx) {
            self.selected_page = page;
            self.state_version = self.state_version.wrapping_add(1);
            cx.notify();
        }
    }

    fn setting_pages(&self, _window: &mut Window, cx: &App) -> Vec<SettingPage> {
        let certificate_manager_view = self.certificate_manager_view.clone();
        let llm_view = self.llm_providers_view.clone();
        let default_settings = AppSettings::default();
        let default_system_hotkey = AppSettings::default().current_system_hotkey().to_string();

        vec![
            themed_setting_page(SettingPage::new(t!("Settings.General.title")), cx)
                .resettable(true)
                .default_open(true)
                .groups(vec![
                    themed_setting_group(SettingGroup::new(), cx)
                        .title(t!("Settings.General.Language.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Language.ui_language"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "zh-CN".into(),
                                            t!("Settings.General.Language.zh_cn").into(),
                                        ),
                                        (
                                            "zh-HK".into(),
                                            t!("Settings.General.Language.zh_hk").into(),
                                        ),
                                        ("en".into(), t!("Settings.General.Language.en").into()),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(AppSettings::global(cx).locale.clone())
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.locale = val.to_string();
                                        gpui_component::set_locale(&settings.locale);
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(default_settings.locale.clone())),
                            )
                            .description(
                                t!("Settings.General.Language.ui_language_desc").to_string(),
                            ),
                        ]),
                    themed_setting_group(SettingGroup::new(), cx)
                        .title(t!("Settings.General.Appearance.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Appearance.theme_mode"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "auto".into(),
                                            t!("Settings.General.Appearance.theme_mode_auto")
                                                .into(),
                                        ),
                                        (
                                            "light".into(),
                                            t!("Settings.General.Appearance.theme_mode_light")
                                                .into(),
                                        ),
                                        (
                                            "dark".into(),
                                            t!("Settings.General.Appearance.theme_mode_dark")
                                                .into(),
                                        ),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).theme_preference_value(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.set_theme_preference(val.as_ref());
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.theme_preference_value()),
                            )
                            .description(
                                t!("Settings.General.Appearance.theme_mode_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.glass_effect"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).enable_glass_effect,
                                    |val: bool, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.enable_glass_effect = val;
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                )
                                .default_value(default_settings.enable_glass_effect),
                            )
                            .description(
                                t!("Settings.General.Appearance.glass_effect_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.glass_opacity"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: MIN_GLASS_OPACITY as f64,
                                        max: MAX_GLASS_OPACITY as f64,
                                        step: 0.01,
                                    },
                                    |cx: &App| AppSettings::global(cx).ui_surface_opacity,
                                    |val: f64, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.ui_surface_opacity = clamp_ui_surface_opacity(val);
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.ui_surface_opacity),
                            )
                            .description(
                                t!("Settings.General.Appearance.glass_opacity_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.window_opacity"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: MIN_GLASS_OPACITY as f64,
                                        max: MAX_GLASS_OPACITY as f64,
                                        step: 0.01,
                                    },
                                    |cx: &App| AppSettings::global(cx).backdrop_opacity,
                                    |val: f64, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.backdrop_opacity = clamp_backdrop_opacity(val);
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.backdrop_opacity),
                            )
                            .description(
                                t!("Settings.General.Appearance.window_opacity_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Font.font_family"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        ("Arial".into(), "Arial".into()),
                                        ("Helvetica".into(), "Helvetica".into()),
                                        ("Times New Roman".into(), "Times New Roman".into()),
                                        ("Courier New".into(), "Courier New".into()),
                                        ("JetBrains Mono".into(), "JetBrains Mono".into()),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).font_family.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let font_size = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.font_family = val.to_string();
                                            settings.save();
                                            settings.font_size
                                        };
                                        AppSettings::apply_ui_font_preferences(
                                            val.clone(),
                                            font_size,
                                            cx,
                                        );
                                    },
                                ))
                                .default_value(
                                    SharedString::from(default_settings.font_family.clone()),
                                ),
                            )
                            .description(t!("Settings.General.Font.font_family_desc").to_string()),
                            SettingItem::new(
                                t!("Settings.General.Font.font_size"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 12.0,
                                        max: 32.0,
                                        ..Default::default()
                                    },
                                    |cx: &App| AppSettings::global(cx).font_size,
                                    |val: f64, cx: &mut App| {
                                        let font_family = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.font_size = val;
                                            settings.save();
                                            settings.font_family.clone()
                                        };
                                        AppSettings::apply_ui_font_preferences(
                                            font_family,
                                            val,
                                            cx,
                                        );
                                    },
                                ))
                                .default_value(default_settings.font_size),
                            )
                            .description(t!("Settings.General.Font.font_size_desc").to_string()),
                            SettingItem::new(
                                t!("Settings.General.Appearance.theme_name"),
                                themed_setting_field(SettingField::dropdown(
                                    {
                                        let current_mode = Theme::global(cx).mode;
                                        let mut themes: Vec<_> = ThemeRegistry::global(cx)
                                            .themes()
                                            .values()
                                            .filter(|t| t.mode == current_mode)
                                            .map(|t| (t.name.clone(), t.name.clone()))
                                            .collect();
                                        themes.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                                        themes
                                    },
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).theme_name.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.theme_name = val.to_string();
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.theme_name.clone()),
                            )
                            .description(
                                t!("Settings.General.Appearance.theme_name_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.scrollbar_show"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "scrolling".into(),
                                            t!("Settings.General.Appearance.scrollbar_show_scrolling")
                                                .into(),
                                        ),
                                        (
                                            "hover".into(),
                                            t!("Settings.General.Appearance.scrollbar_show_hover")
                                                .into(),
                                        ),
                                        (
                                            "always".into(),
                                            t!("Settings.General.Appearance.scrollbar_show_always")
                                                .into(),
                                        ),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).scrollbar_show.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.scrollbar_show = val.to_string();
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.scrollbar_show.clone()),
                            )
                            .description(
                                t!("Settings.General.Appearance.scrollbar_show_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.shadow"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).shadow,
                                    |val: bool, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.shadow = val;
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                )
                                .default_value(default_settings.shadow),
                            )
                            .description(
                                t!("Settings.General.Appearance.shadow_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.radius"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 0.0,
                                        max: 24.0,
                                        step: 1.0,
                                    },
                                    |cx: &App| AppSettings::global(cx).radius,
                                    |val: f64, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.radius = val.max(0.0);
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(default_settings.radius),
                            )
                            .description(
                                t!("Settings.General.Appearance.radius_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Appearance.mono_font_family"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        ("Menlo".into(), "Menlo".into()),
                                        ("Consolas".into(), "Consolas".into()),
                                        ("DejaVu Sans Mono".into(), "DejaVu Sans Mono".into()),
                                        ("JetBrains Mono".into(), "JetBrains Mono".into()),
                                        ("Fira Code".into(), "Fira Code".into()),
                                        ("Source Code Pro".into(), "Source Code Pro".into()),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).mono_font_family.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.mono_font_family = val.to_string();
                                            settings.save();
                                            settings.clone()
                                        };
                                        settings_snapshot.apply_theme_preferences(None, cx);
                                    },
                                ))
                                .default_value(
                                    SharedString::from(default_settings.mono_font_family.clone()),
                                ),
                            )
                            .description(
                                t!("Settings.General.Appearance.mono_font_family_desc")
                                    .to_string(),
                            ),
                        ]),
                    themed_setting_group(SettingGroup::new(), cx)
                        .title(t!("Settings.General.Sync.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Sync.backend_type"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "sync_server".into(),
                                            t!("Settings.General.Sync.self_hosted_backend").into(),
                                        ),
                                        (
                                            "webdav".into(),
                                            t!("Settings.General.Sync.webdav_backend").into(),
                                        ),
                                        (
                                            "github_gist".into(),
                                            t!("Settings.General.Sync.github_gist_backend").into(),
                                        ),
                                        ("google_drive".into(), "Google Drive".into()),
                                        ("onedrive".into(), "OneDrive".into()),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).sync_backend_type.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.sync_backend_type = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(
                                    SharedString::from(default_settings.sync_backend_type.clone()),
                                ),
                            )
                            .description(t!("Settings.General.Sync.backend_type_desc").to_string()),
                            // sync_server URL（仅 sync_server 后端显示）
                            SettingItem::new(
                                t!("Settings.General.Sync.server_url"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).sync_server_url.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        apply_sync_server_url_setting(val, cx);
                                    },
                                ))
                                .default_value(
                                    SharedString::from(default_settings.sync_server_url.clone()),
                                ),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "sync_server"
                            })
                            .layout(Axis::Vertical)
                            .description(t!("Settings.General.Sync.server_url_desc").to_string()),
                            // 登录/认证表单（仅 sync_server 后端显示）
                            SettingItem::render(
                                |_opts: &RenderOptions,
                                 window: &mut gpui::Window,
                                 cx: &mut gpui::App| {
                                    if AppSettings::global(cx).sync_backend_type != "sync_server" {
                                        return gpui::div().into_any_element();
                                    }
                                    let user = GlobalCurrentUser::get_user(cx);
                                    if let Some(user) = user {
                                        render_logged_in_user_sync(&user, cx)
                                    } else {
                                        render_auth_form_sync(window, cx)
                                    }
                                },
                            ),
                            // WebDAV 配置（仅 webdav 后端显示）
                            SettingItem::new(
                                t!("Settings.General.Sync.webdav_endpoint"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.endpoint.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.endpoint = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            })
                            .description(
                                t!("Settings.General.Sync.webdav_endpoint_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Sync.webdav_auth_type"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "basic".into(),
                                            t!("Settings.General.Sync.webdav_username_password")
                                                .into(),
                                        ),
                                        ("bearer".into(), "Bearer Token".into()),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.auth_type.clone())
                                                .unwrap_or_else(|| "basic".to_string()),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.auth_type = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from("basic".to_string())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            }),
                            SettingItem::new(
                                t!("Settings.General.Sync.webdav_username"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.username.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.username = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            }),
                            SettingItem::new(
                                t!("Settings.General.Sync.webdav_password"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.password.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.password = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            }),
                            SettingItem::new(
                                "WebDAV Bearer Token",
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.bearer_token.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.bearer_token = val.to_string();
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            }),
                            SettingItem::new(
                                t!("Settings.General.Sync.webdav_storage_path"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .webdav_config
                                                .as_ref()
                                                .map(|c| c.vault_path.clone())
                                                .unwrap_or_else(|| "ONetCli-vault".to_string()),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let webdav = settings
                                            .webdav_config
                                            .get_or_insert_with(WebDavSettings::default);
                                        webdav.vault_path = if val.is_empty() {
                                            "ONetCli-vault".to_string()
                                        } else {
                                            val.to_string()
                                        };
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from("ONetCli-vault".to_string())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "webdav"
                            })
                            .description(
                                t!("Settings.General.Sync.webdav_storage_path_desc").to_string(),
                            ),
                            // GitHub Gist 配置（仅 github_gist 后端显示）
                            SettingItem::new(
                                t!("Settings.General.Sync.github_client_id"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .gist_config
                                                .as_ref()
                                                .map(|c| c.client_id.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let gist = settings
                                            .gist_config
                                            .get_or_insert_with(GistSettings::default);
                                        let next_client_id = val.to_string();
                                        if gist.client_id != next_client_id {
                                            gist.gist_id = None;
                                            gist.tokens = None;
                                        }
                                        gist.client_id = next_client_id;
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "github_gist"
                            })
                            .description(
                                t!("Settings.General.Sync.github_client_id_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Sync.gist_id"),
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .gist_config
                                                .as_ref()
                                                .and_then(|c| c.gist_id.clone())
                                                .unwrap_or_else(|| {
                                                    t!("Settings.General.Sync.gist_not_authorized")
                                                        .to_string()
                                                }),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let gist = settings
                                            .gist_config
                                            .get_or_insert_with(GistSettings::default);
                                        let gist_id = Some(val.to_string()).filter(|s| {
                                            !s.is_empty()
                                                && *s
                                                    != *t!(
                                                        "Settings.General.Sync.gist_not_authorized"
                                                    )
                                        });
                                        if gist_id.is_none() {
                                            gist.tokens = None;
                                        }
                                        gist.gist_id = gist_id;
                                        settings.save();
                                    },
                                ))
                                .default_value(
                                    SharedString::from(t!(
                                        "Settings.General.Sync.gist_not_authorized"
                                    )),
                                ),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "github_gist"
                            })
                            .description(
                                t!("Settings.General.Sync.gist_id_auto_fill_desc").to_string(),
                            ),
                            SettingItem::action_button(
                                |_opts: &RenderOptions,
                                 _window: &mut gpui::Window,
                                 cx: &mut gpui::App| {
                                    use gpui_component::button::{
                                        Button, ButtonVariant, ButtonVariants as _,
                                    };
                                    let client_id = AppSettings::global(cx)
                                        .gist_config
                                        .as_ref()
                                        .map(|c| c.client_id.clone())
                                        .unwrap_or_default();
                                    if client_id.is_empty() {
                                        gpui::div().into_any_element()
                                    } else {
                                        let has_auth = AppSettings::global(cx)
                                            .gist_config
                                            .as_ref()
                                            .map(|c| {
                                                c.gist_id
                                                    .as_ref()
                                                    .map(|id| !id.is_empty())
                                                    .unwrap_or(false)
                                                    && c.tokens.is_some()
                                            })
                                            .unwrap_or(false);
                                        Button::new("github-auth-btn")
                                            .with_variant(if has_auth {
                                                ButtonVariant::Ghost
                                            } else {
                                                ButtonVariant::Primary
                                            })
                                            .child(if has_auth {
                                                t!("Settings.General.Sync.reauthorize")
                                            } else {
                                                t!("Settings.General.Sync.authorize_github")
                                            })
                                            .into_any_element()
                                    }
                                },
                                move |window, cx| {
                                    let client_id = AppSettings::global(cx)
                                        .gist_config
                                        .as_ref()
                                        .map(|c| c.client_id.clone())
                                        .unwrap_or_default();
                                    if !client_id.is_empty() {
                                        let dialog_entity =
                                            cx.new(|_cx| GithubAuthDialog::new(client_id.clone()));
                                        window.open_dialog(cx, move |dialog, _window, _cx| {
                                            dialog
                                                .title(t!(
                                                    "Settings.General.Sync.github_auth_title"
                                                ))
                                                .child(dialog_entity.clone())
                                        });
                                    }
                                },
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "github_gist"
                            }),
                            // Google Drive 配置（仅 google_drive 后端显示）
                            SettingItem::new(
                                "Google Drive Client ID",
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .google_drive_config
                                                .as_ref()
                                                .map(|c| c.client_id.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let gd = settings
                                            .google_drive_config
                                            .get_or_insert_with(GoogleDriveSettings::default);
                                        let next_client_id = val.to_string();
                                        if gd.client_id != next_client_id {
                                            gd.tokens = None;
                                        }
                                        gd.client_id = next_client_id;
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "google_drive"
                            })
                            .description(
                                t!("Settings.General.Sync.google_drive_client_id_desc").to_string(),
                            ),
                            SettingItem::new(
                                "Google Drive Client Secret",
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .google_drive_config
                                                .as_ref()
                                                .map(|c| c.client_secret.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let gd = settings
                                            .google_drive_config
                                            .get_or_insert_with(GoogleDriveSettings::default);
                                        let next_secret = val.to_string();
                                        if gd.client_secret != next_secret {
                                            gd.tokens = None;
                                        }
                                        gd.client_secret = next_secret;
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "google_drive"
                            })
                            .description(
                                t!("Settings.General.Sync.google_drive_client_secret_desc")
                                    .to_string(),
                            ),
                            SettingItem::action_button(
                                |_opts: &RenderOptions,
                                 _window: &mut gpui::Window,
                                 cx: &mut gpui::App| {
                                    use gpui_component::button::{
                                        Button, ButtonVariant, ButtonVariants as _,
                                    };
                                    let config =
                                        AppSettings::global(cx).google_drive_config.as_ref();
                                    let client_id =
                                        config.map(|c| c.client_id.clone()).unwrap_or_default();
                                    let client_secret =
                                        config.map(|c| c.client_secret.clone()).unwrap_or_default();
                                    if client_id.is_empty() || client_secret.is_empty() {
                                        return gpui::div().into_any_element();
                                    }
                                    let has_auth =
                                        config.map(|c| c.tokens.is_some()).unwrap_or(false);
                                    Button::new("gdrive-auth-btn")
                                        .with_variant(if has_auth {
                                            ButtonVariant::Ghost
                                        } else {
                                            ButtonVariant::Primary
                                        })
                                        .child(if has_auth {
                                            t!("Settings.General.Sync.reauthorize")
                                        } else {
                                            t!("Settings.General.Sync.authorize_google_drive")
                                        })
                                        .into_any_element()
                                },
                                move |window, cx| {
                                    let config =
                                        AppSettings::global(cx).google_drive_config.as_ref();
                                    let client_id =
                                        config.map(|c| c.client_id.clone()).unwrap_or_default();
                                    let client_secret =
                                        config.map(|c| c.client_secret.clone()).unwrap_or_default();
                                    if client_id.is_empty() || client_secret.is_empty() {
                                        return;
                                    }
                                    let dialog_entity = cx.new(|_cx| {
                                        crate::settings::oauth_dialog::GoogleDriveAuthDialog::new(
                                            client_id,
                                            client_secret,
                                        )
                                    });
                                    window.open_dialog(cx, move |dialog, _window, _cx| {
                                        dialog
                                            .title(t!(
                                                "Settings.General.Sync.google_drive_auth_title"
                                            ))
                                            .child(dialog_entity.clone())
                                    });
                                },
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "google_drive"
                            }),
                            // OneDrive 配置（仅 onedrive 后端显示）
                            SettingItem::new(
                                "OneDrive Client ID",
                                themed_setting_field(SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .onedrive_config
                                                .as_ref()
                                                .map(|c| c.client_id.clone())
                                                .unwrap_or_default(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        let od = settings
                                            .onedrive_config
                                            .get_or_insert_with(OneDriveSettings::default);
                                        let next_client_id = val.to_string();
                                        if od.client_id != next_client_id {
                                            od.tokens = None;
                                        }
                                        od.client_id = next_client_id;
                                        settings.save();
                                    },
                                ))
                                .default_value(SharedString::from(String::new())),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "onedrive"
                            })
                            .description(
                                t!("Settings.General.Sync.onedrive_client_id_desc").to_string(),
                            ),
                            SettingItem::action_button(
                                |_opts: &RenderOptions,
                                 _window: &mut gpui::Window,
                                 cx: &mut gpui::App| {
                                    use gpui_component::button::{
                                        Button, ButtonVariant, ButtonVariants as _,
                                    };
                                    let client_id = AppSettings::global(cx)
                                        .onedrive_config
                                        .as_ref()
                                        .map(|c| c.client_id.clone())
                                        .unwrap_or_default();
                                    if client_id.is_empty() {
                                        return gpui::div().into_any_element();
                                    }
                                    let has_auth = AppSettings::global(cx)
                                        .onedrive_config
                                        .as_ref()
                                        .map(|c| c.tokens.is_some())
                                        .unwrap_or(false);
                                    Button::new("onedrive-auth-btn")
                                        .with_variant(if has_auth {
                                            ButtonVariant::Ghost
                                        } else {
                                            ButtonVariant::Primary
                                        })
                                        .child(if has_auth {
                                            t!("Settings.General.Sync.reauthorize")
                                        } else {
                                            t!("Settings.General.Sync.authorize_onedrive")
                                        })
                                        .into_any_element()
                                },
                                move |window, cx| {
                                    let client_id = AppSettings::global(cx)
                                        .onedrive_config
                                        .as_ref()
                                        .map(|c| c.client_id.clone())
                                        .unwrap_or_default();
                                    if client_id.is_empty() {
                                        return;
                                    }
                                    let dialog_entity = cx.new(|_cx| {
                                        crate::settings::oauth_dialog::OneDriveAuthDialog::new(
                                            client_id,
                                        )
                                    });
                                    window.open_dialog(cx, move |dialog, _window, _cx| {
                                        dialog
                                            .title(t!("Settings.General.Sync.onedrive_auth_title"))
                                            .child(dialog_entity.clone())
                                    });
                                },
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).sync_backend_type == "onedrive"
                            }),
                        ]),
                    themed_setting_group(SettingGroup::new(), cx)
                        .title(t!("Settings.General.Terminal.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Terminal.font_family"),
                                themed_setting_field(SettingField::dropdown(
                                    TerminalTheme::available_monospace_fonts()
                                        .into_iter()
                                        .map(|font| {
                                            let font = SharedString::from(font);
                                            (font.clone(), font)
                                        })
                                        .collect(),
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).terminal_font_family.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.terminal_font_family = val.to_string();
                                            settings.save();
                                            settings.clone()
                                        };
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                ))
                                .default_value(
                                    SharedString::from(
                                        default_settings.terminal_font_family.clone(),
                                    ),
                                ),
                            )
                            .description(
                                t!("Settings.General.Terminal.font_family_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.font_ligatures"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).terminal_font_ligatures,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_font_ligatures = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                )
                                .default_value(default_settings.terminal_font_ligatures),
                            )
                            .description(
                                t!("Settings.General.Terminal.font_ligatures_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.font_size"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 12.0,
                                        max: 32.0,
                                        ..Default::default()
                                    },
                                    |cx: &App| AppSettings::global(cx).terminal_font_size,
                                    |val: f64, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_font_size = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                ))
                                .default_value(default_settings.terminal_font_size),
                            )
                            .description(
                                t!("Settings.General.Terminal.font_size_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.line_height"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: MIN_LINE_HEIGHT_SCALE as f64,
                                        max: MAX_LINE_HEIGHT_SCALE as f64,
                                        step: 0.1,
                                    },
                                    |cx: &App| AppSettings::global(cx).terminal_line_height_scale,
                                    |val: f64, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_line_height_scale = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                ))
                                .default_value(default_settings.terminal_line_height_scale),
                            )
                            .description(
                                t!("Settings.General.Terminal.line_height_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.auto_copy"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).terminal_auto_copy,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_auto_copy = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                )
                                .default_value(default_settings.terminal_auto_copy),
                            )
                            .description(
                                t!("Settings.General.Terminal.auto_copy_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.middle_click_paste"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).terminal_middle_click_paste,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_middle_click_paste = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                )
                                .default_value(default_settings.terminal_middle_click_paste),
                            )
                            .description(
                                t!("Settings.General.Terminal.middle_click_paste_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.recovery_scrollback_lines"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 0.0,
                                        max: MAX_RECOVERY_SCROLLBACK_LINES as f64,
                                        step: 100.0,
                                    },
                                    |cx: &App| {
                                        AppSettings::global(cx).terminal_recovery_scrollback_lines
                                    },
                                    |val: f64, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_recovery_scrollback_lines = val;
                                        settings.save();
                                        let settings_snapshot = settings.clone();
                                        sync_terminal_settings_to_all(settings_snapshot, cx);
                                    },
                                ))
                                .default_value(default_settings.terminal_recovery_scrollback_lines),
                            )
                            .description(
                                t!("Settings.General.Terminal.recovery_scrollback_lines_desc")
                                    .to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.restore_connections_on_startup"),
                                SettingField::switch(
                                    |cx: &App| {
                                        AppSettings::global(cx).restore_connections_on_startup
                                    },
                                    |val: bool, cx: &mut App| {
                                        let should_clear_pending_snapshot = {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.restore_connections_on_startup = val;
                                            settings.save();
                                            !val
                                        };

                                        if should_clear_pending_snapshot {
                                            crate::connection_restore::clear_pending_connection_restore_snapshot();
                                        }
                                    },
                                )
                                .default_value(default_settings.restore_connections_on_startup),
                            )
                            .description(
                                t!("Settings.General.Terminal.restore_connections_on_startup_desc")
                                    .to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Terminal.restore_session_content"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).restore_session_content,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.restore_session_content = val;
                                        settings.save();
                                    },
                                )
                                .default_value(default_settings.restore_session_content),
                            )
                            .description(
                                t!("Settings.General.Terminal.restore_session_content_desc")
                                    .to_string(),
                            )
                            .visible_when(|cx| {
                                AppSettings::global(cx).restore_connections_on_startup
                            }),
                            SettingItem::new(
                                t!("Settings.General.Terminal.check_running_processes_on_exit"),
                                SettingField::switch(
                                    |cx: &App| {
                                        AppSettings::global(cx).terminal_check_running_processes_on_exit
                                    },
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.terminal_check_running_processes_on_exit = val;
                                        settings.save();
                                        sync_terminal_settings_to_all(settings.clone(), cx);
                                    },
                                )
                                .default_value(
                                    default_settings.terminal_check_running_processes_on_exit,
                                ),
                            )
                            .description(
                                t!("Settings.General.Terminal.check_running_processes_on_exit_desc")
                                    .to_string(),
                            ),
                        ]),
                    themed_setting_group(SettingGroup::new(), cx)
                        .title(t!("Settings.General.Database.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Database.open_mode"),
                                themed_setting_field(SettingField::dropdown(
                                    vec![
                                        (
                                            "single".into(),
                                            t!("Settings.General.Database.open_mode_single").into(),
                                        ),
                                        (
                                            "workspace".into(),
                                            t!("Settings.General.Database.open_mode_workspace")
                                                .into(),
                                        ),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).database_open_mode.as_str(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.database_open_mode =
                                            DatabaseOpenMode::from_str(&val);
                                        settings.save();
                                    },
                                ))
                                .default_value(
                                    SharedString::from(
                                        default_settings.database_open_mode.as_str(),
                                    ),
                                ),
                            )
                            .description(
                                t!("Settings.General.Database.open_mode_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Database.large_text_editor_open_mode"),
                                SettingField::dropdown(
                                    vec![
                                        (
                                            "sidebar_preview".into(),
                                            t!(
                                                "Settings.General.Database.large_text_editor_open_mode_sidebar"
                                            )
                                            .into(),
                                        ),
                                        (
                                            "dialog".into(),
                                            t!(
                                                "Settings.General.Database.large_text_editor_open_mode_dialog"
                                            )
                                            .into(),
                                        ),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .large_text_cell_editor_open_mode
                                                .as_str(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let mode =
                                            LargeTextCellEditorOpenMode::from_str(val.as_ref());
                                        let settings = AppSettings::global_mut(cx);
                                        settings.large_text_cell_editor_open_mode = mode;
                                        settings.save();
                                        db_view::set_large_text_editor_open_mode(mode.into(), cx);
                                    },
                                )
                                .default_value(SharedString::from(
                                    default_settings
                                        .large_text_cell_editor_open_mode
                                        .as_str(),
                                )),
                            )
                            .description(
                                t!("Settings.General.Database.large_text_editor_open_mode_desc")
                                    .to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Database.auto_save"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).enable_sql_auto_save,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.enable_sql_auto_save = val;
                                        settings.save();
                                        AppSettings::update_auto_save_config(
                                            val,
                                            cx.global::<AppSettings>().sql_auto_save_interval,
                                            cx,
                                        );
                                    },
                                )
                                .default_value(default_settings.enable_sql_auto_save),
                            )
                            .description(
                                t!("Settings.General.Database.auto_save_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Database.auto_save_interval"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 1.0,
                                        max: 60.0,
                                        step: 1.0,
                                    },
                                    |cx: &App| AppSettings::global(cx).sql_auto_save_interval,
                                    |val: f64, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.sql_auto_save_interval = val;
                                        settings.save();
                                        AppSettings::update_auto_save_config(
                                            cx.global::<AppSettings>().enable_sql_auto_save,
                                            val,
                                            cx,
                                        );
                                    },
                                ))
                                .default_value(default_settings.sql_auto_save_interval),
                            )
                            .description(
                                t!("Settings.General.Database.auto_save_interval_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Database.undo_stack_size"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 0.0,
                                        max: 100.0,
                                        step: 1.0,
                                        ..Default::default()
                                    },
                                    |cx: &App| AppSettings::global(cx).db_undo_stack_size as f64,
                                    |val: f64, cx: &mut App| {
                                        {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.db_undo_stack_size = val as usize;
                                            settings.save();
                                        }
                                        let undo_stack_size =
                                            AppSettings::global(cx).db_undo_stack_size;
                                        set_db_view_settings(cx, undo_stack_size);
                                    },
                                ))
                                .default_value(default_settings.db_undo_stack_size as f64),
                            )
                            .description(
                                t!("Settings.General.Database.undo_stack_size_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Database.ai_auto_title"),
                                SettingField::switch(
                                    |cx: &App| {
                                        AppSettings::global(cx).ai_auto_generate_session_title
                                    },
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.ai_auto_generate_session_title = val;
                                        settings.save();
                                        cx.set_global(GlobalChatSettings {
                                            ai_auto_generate_session_title: val,
                                        });
                                    },
                                )
                                .default_value(default_settings.ai_auto_generate_session_title),
                            )
                            .description(
                                t!("Settings.General.Database.ai_auto_title_desc").to_string(),
                            ),
                        ]),
                    SettingGroup::new()
                        .title(t!("Settings.General.Log.group_title"))
                        .item(
                            SettingItem::new(
                                t!("Settings.General.Log.file_path"),
                                SettingField::input(
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).log_file_path.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.log_file_path = val.trim().to_string();
                                        settings.save();
                                    },
                                )
                                .default_value(SharedString::from("")),
                            )
                            .description(t!("Settings.General.Log.file_path_desc").to_string()),
                        ),
                    SettingGroup::new()
                        .title(t!("Settings.General.Update.group_title"))
                        .items(vec![
                            SettingItem::new(
                                t!("Settings.General.Update.auto_update"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).auto_update,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.auto_update = val;
                                        settings.save();
                                    },
                                )
                                .default_value(default_settings.auto_update),
                            )
                            .description(
                                t!("Settings.General.Update.auto_update_desc").to_string(),
                            ),
                            SettingItem::render(move |_options, _window, cx| {
                                render_manual_update_check_item(cx)
                            }),
                        ]),
                    SettingGroup::new()
                        .title(t!("Settings.General.Proxy.group_title"))
                        .item(SettingItem::render(move |_options, _window, cx| {
                            render_global_proxy_settings_item(cx)
                        })),
                    SettingGroup::new()
                        .title(t!("Settings.General.SSH.group_title"))
                        .item(
                            SettingItem::new(
                                t!("Settings.General.SSH.auto_accept_new_keys"),
                                SettingField::switch(
                                    |cx: &App| AppSettings::global(cx).ssh_auto_accept_new_keys,
                                    |val: bool, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.ssh_auto_accept_new_keys = val;
                                        settings.save();
                                    },
                                )
                                .default_value(default_settings.ssh_auto_accept_new_keys),
                            )
                            .description(
                                t!("Settings.General.SSH.auto_accept_new_keys_desc").to_string(),
                            ),
                        ),
                ]),
            // 快捷键页面
            themed_setting_page(SettingPage::new(t!("Settings.Shortcuts.title")), cx).group(
                themed_setting_group(SettingGroup::new(), cx)
                    .item(
                        SettingItem::new(
                            t!("Settings.Shortcuts.system_hotkey"),
                            SettingField::input(
                                |cx: &App| {
                                    SharedString::from(
                                        AppSettings::global(cx).current_system_hotkey().to_string(),
                                    )
                                },
                                |val: SharedString, cx: &mut App| {
                                    let spec = val.trim().to_string();
                                    if spec.is_empty() {
                                        let settings = AppSettings::global_mut(cx);
                                        #[cfg(target_os = "macos")]
                                        {
                                            settings.system_hotkey_macos =
                                                DEFAULT_SYSTEM_HOTKEY_MACOS.to_string();
                                        }
                                        #[cfg(not(target_os = "macos"))]
                                        {
                                            settings.system_hotkey_other =
                                                DEFAULT_SYSTEM_HOTKEY_OTHER.to_string();
                                        }
                                        settings.save();
                                        return;
                                    }

                                    if !is_valid_system_hotkey(&spec) {
                                        return;
                                    }

                                    let settings = AppSettings::global_mut(cx);
                                    #[cfg(target_os = "macos")]
                                    {
                                        settings.system_hotkey_macos = spec;
                                    }
                                    #[cfg(not(target_os = "macos"))]
                                    {
                                        settings.system_hotkey_other = spec;
                                    }
                                    settings.save();
                                },
                            )
                            .default_value(SharedString::from(default_system_hotkey)),
                        )
                        .description(t!("Settings.Shortcuts.system_hotkey_desc").to_string()),
                    )
                    .item(SettingItem::render(move |_options, _window, cx| {
                        render_shortcuts_section(cx)
                    })),
            ),
            themed_setting_page(SettingPage::new(t!("LlmProviders.title")), cx).group(
                themed_setting_group(SettingGroup::new(), cx).item(SettingItem::render(
                    move |_options, _window, _cx| llm_view.clone().into_any_element(),
                )),
            ),
            themed_setting_page(SettingPage::new(t!("CertificateManager.title")), cx).group(
                themed_setting_group(SettingGroup::new(), cx).item(SettingItem::render(
                    move |_options, _window, _cx| {
                        certificate_manager_view.clone().into_any_element()
                    },
                )),
            ),
            // 关于页面
            themed_setting_page(SettingPage::new(t!("Settings.About.title")), cx).group(
                themed_setting_group(SettingGroup::new(), cx).item(SettingItem::render(
                    move |_options, _window, cx| render_about_section(cx),
                )),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::parse_deepin_theme_appearance;
    use super::{
        AppSettings, GlobalProxySettings, ProxyType, SavedWindowBounds, SavedWindowDisplayState,
        centered_window_bounds_within_visible_area, clamp_ui_surface_opacity,
        editable_sync_server_url, normalize_sync_server_url,
    };
    use gpui::{Bounds, WindowBackgroundAppearance, WindowBounds, point, px, size};
    use gpui::{WindowAppearance, WindowAppearance::*};
    use gpui_component::{MAX_GLASS_OPACITY, MIN_GLASS_OPACITY, ThemeMode};

    #[test]
    fn 自动切换关闭时沿用手动主题() {
        let mut settings = AppSettings::default();
        settings.theme_mode = "dark".to_string();
        settings.auto_switch_theme = false;

        assert_eq!(settings.effective_theme_mode(Light), ThemeMode::Dark);
        assert_eq!(settings.effective_theme_mode(Dark), ThemeMode::Dark);
    }

    #[test]
    fn 自动切换开启时跟随系统外观() {
        let mut settings = AppSettings::default();
        settings.theme_mode = "light".to_string();
        settings.auto_switch_theme = true;

        assert_eq!(
            settings.effective_theme_mode(WindowAppearance::Light),
            ThemeMode::Light
        );
        assert_eq!(
            settings.effective_theme_mode(WindowAppearance::Dark),
            ThemeMode::Dark
        );
        assert_eq!(
            settings.effective_theme_mode(WindowAppearance::VibrantDark),
            ThemeMode::Dark
        );
    }

    #[test]
    fn 主题模式下拉值可映射到当前设置() {
        let mut settings = AppSettings::default();

        assert_eq!(settings.theme_preference_value(), "light");

        settings.theme_mode = "dark".to_string();
        assert_eq!(settings.theme_preference_value(), "dark");

        settings.auto_switch_theme = true;
        assert_eq!(settings.theme_preference_value(), "auto");
    }

    #[test]
    fn 主题模式下拉值可写回亮暗和自动设置() {
        let mut settings = AppSettings::default();

        settings.set_theme_preference("dark");
        assert_eq!(settings.theme_mode, "dark");
        assert!(!settings.auto_switch_theme);

        settings.set_theme_preference("auto");
        assert!(settings.auto_switch_theme);
        assert_eq!(settings.theme_mode, "dark");

        settings.set_theme_preference("light");
        assert_eq!(settings.theme_mode, "light");
        assert!(!settings.auto_switch_theme);
    }

    #[test]
    fn 同步地址输入保留末尾斜杠但规范化结果移除末尾斜杠() {
        let value = " https://example.com/api/ ";

        assert_eq!(editable_sync_server_url(value), "https://example.com/api/");
        assert_eq!(normalize_sync_server_url(value), "https://example.com/api");
    }

    #[test]
    fn 主窗口状态可在窗口边界之间往返转换() {
        let bounds = Bounds {
            origin: point(px(120.0), px(80.0)),
            size: size(px(1440.0), px(900.0)),
        };

        let saved = SavedWindowBounds::from_window_bounds(WindowBounds::Maximized(bounds)).unwrap();

        assert_eq!(saved.state, SavedWindowDisplayState::Maximized);
        assert_eq!(
            saved.to_window_bounds(),
            Some(WindowBounds::Maximized(bounds))
        );
    }

    #[test]
    fn 非法主窗口状态不会参与恢复() {
        let saved = SavedWindowBounds {
            state: SavedWindowDisplayState::Windowed,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 800.0,
        };

        assert_eq!(saved.to_window_bounds(), None);
    }

    #[test]
    fn 越界主窗口状态恢复时会回到主屏中间() {
        let visible_bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1040.0)),
        };
        let saved = SavedWindowBounds {
            state: SavedWindowDisplayState::Windowed,
            x: 2600.0,
            y: 120.0,
            width: 900.0,
            height: 700.0,
        };

        assert_eq!(
            saved.fit_in_visible_bounds(visible_bounds),
            Some(WindowBounds::Windowed(Bounds {
                origin: point(px(510.0), px(170.0)),
                size: size(px(900.0), px(700.0)),
            }))
        );
    }

    #[test]
    fn 超出可见区域的主窗口尺寸会先裁剪再居中恢复() {
        let visible_bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1040.0)),
        };
        let saved = SavedWindowBounds {
            state: SavedWindowDisplayState::Maximized,
            x: -400.0,
            y: -300.0,
            width: 2600.0,
            height: 1600.0,
        };

        assert_eq!(
            saved.fit_in_visible_bounds(visible_bounds),
            Some(WindowBounds::Maximized(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1920.0), px(1040.0)),
            }))
        );
    }

    #[test]
    fn 默认主窗口居中会避开底部不可见区域() {
        let visible_bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1040.0)),
        };

        assert_eq!(
            centered_window_bounds_within_visible_area(
                size(px(1600.0), px(1200.0)),
                Some(visible_bounds),
            ),
            WindowBounds::Windowed(Bounds {
                origin: point(px(160.0), px(0.0)),
                size: size(px(1600.0), px(1040.0)),
            })
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn deepin_主题名可映射为亮暗模式() {
        assert_eq!(
            parse_deepin_theme_appearance("'deepin-dark'"),
            Some(WindowAppearance::Dark)
        );
        assert_eq!(
            parse_deepin_theme_appearance("(<\'hazy-color.dark\'>,)"),
            Some(WindowAppearance::Dark)
        );
        assert_eq!(
            parse_deepin_theme_appearance("'deepin'"),
            Some(WindowAppearance::Light)
        );
        assert_eq!(
            parse_deepin_theme_appearance("(<\'hazy-color.light\'>,)"),
            Some(WindowAppearance::Light)
        );
    }

    #[test]
    fn global_proxy_settings_build_proxy_url_without_auth() {
        let settings = GlobalProxySettings {
            enabled: true,
            proxy_type: ProxyType::Socks5,
            host: "127.0.0.1".to_string(),
            port: 7890,
            username: String::new(),
            password: String::new(),
        };

        let proxy_url = settings
            .to_proxy_url()
            .expect("代理 URL 应构建成功")
            .expect("启用代理时应返回 URL");

        assert_eq!(proxy_url.as_str(), "socks5://127.0.0.1:7890");
    }

    #[test]
    fn global_proxy_settings_build_proxy_url_with_auth() {
        let settings = GlobalProxySettings {
            enabled: true,
            proxy_type: ProxyType::Http,
            host: "proxy.example.com".to_string(),
            port: 8080,
            username: "demo-user".to_string(),
            password: "demo-pass".to_string(),
        };

        let proxy_url = settings
            .to_proxy_url()
            .expect("代理 URL 应构建成功")
            .expect("启用代理时应返回 URL");

        assert_eq!(
            proxy_url.as_str(),
            "http://demo-user:demo-pass@proxy.example.com:8080/"
        );
    }

    #[test]
    fn disabled_global_proxy_settings_return_none() {
        let settings = GlobalProxySettings {
            enabled: false,
            ..GlobalProxySettings::default()
        };

        let proxy_url = settings.to_proxy_url().expect("禁用代理时不应返回错误");

        assert!(proxy_url.is_none());
    }

    #[test]
    fn global_proxy_settings_validate_required_fields() {
        let settings = GlobalProxySettings {
            enabled: true,
            proxy_type: ProxyType::Https,
            host: String::new(),
            port: 0,
            username: String::new(),
            password: String::new(),
        };

        let err = settings.validate().expect_err("缺少主机和端口时应校验失败");

        assert!(err.contains("主机"));
    }

    #[test]
    fn legacy_terminal_settings_maps_terminal_fields() {
        let settings = AppSettings::default();
        let legacy = super::legacy_terminal_settings(&settings);

        assert_eq!(legacy.font_size, settings.terminal_font_size as f32);
        assert_eq!(legacy.auto_copy, settings.terminal_auto_copy);
        assert_eq!(
            legacy.enable_autocomplete,
            settings.terminal_enable_autocomplete
        );
        assert_eq!(legacy.theme, settings.terminal_theme);
    }
}

impl Focusable for SettingsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TabContentEvent> for SettingsPanel {}

impl TabContent for SettingsPanel {
    fn content_key(&self) -> &'static str {
        "Settings"
    }

    fn title(&self, _cx: &App) -> SharedString {
        SharedString::from(t!("Common.settings"))
    }

    fn icon(&self, _cx: &App) -> Option<Icon> {
        Some(IconName::SettingColor.color())
    }

    fn closeable(&self, _cx: &App) -> bool {
        true
    }

    fn on_activate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !cx.has_global::<AppSettings>() {
            let _ = init_settings(cx);
        }
        self.apply_requested_page(cx);
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !cx.has_global::<AppSettings>() {
            let _ = init_settings(cx);
        }

        let sidebar_bg = cx.theme().sidebar;
        // 与首页右侧内容区保持一致，形成统一的内容底板层级。
        let page_bg = cx.theme().muted;
        let sidebar_style = StyleRefinement::default()
            .bg(sidebar_bg)
            .border_color(cx.theme().border)
            .text_color(cx.theme().sidebar_foreground);
        let content_style = StyleRefinement::default().bg(page_bg);

        div().track_focus(&self.focus_handle).size_full().child(
            div().size_full().child(
                Settings::new("main-app-settings")
                    .with_size(self.size)
                    .with_group_variant(self.group_variant)
                    .sidebar_style(&sidebar_style)
                    .content_style(&content_style)
                    .header_style(&sync_server_theme::control_style())
                    .default_selected_index(self.selected_page.select_index())
                    .pages(self.setting_pages(window, cx)),
            ),
        )
    }
}

fn render_manual_update_check_item(cx: &mut App) -> gpui::AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .gap_3()
        .child(
            v_flex()
                .gap_1()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .child(t!("Settings.General.Update.check_now").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("Settings.General.Update.check_now_desc").to_string()),
                ),
        )
        .child(
            Button::new("settings-check-update")
                .icon(IconName::Refresh)
                .label(t!("Settings.General.Update.check_now"))
                .on_click(|_, window, cx| {
                    update::check_for_updates_manually(window, cx);
                }),
        )
        .into_any_element()
}

fn render_global_proxy_settings_item(cx: &mut App) -> gpui::AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .gap_3()
        .child(
            v_flex()
                .gap_1()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .child(t!("Settings.General.Proxy.title").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("Settings.General.Proxy.description").to_string()),
                ),
        )
        .child(
            Button::new("settings-global-proxy")
                .icon(IconName::Globe)
                .label(t!("Settings.General.Proxy.open").to_string())
                .on_click(|_, window, cx| {
                    show_global_proxy_settings_window(window, cx);
                }),
        )
        .into_any_element()
}

#[derive(Clone, PartialEq)]
struct ProxyTypeOption {
    value: ProxyType,
    label: SharedString,
}

impl SelectItem for ProxyTypeOption {
    type Value = ProxyType;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

struct GlobalProxySettingsView {
    focus_handle: FocusHandle,
    enabled: bool,
    proxy_type_select: Entity<SelectState<Vec<ProxyTypeOption>>>,
    host_input: Entity<InputState>,
    port_input: Entity<InputState>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    testing: bool,
    status_message: Option<(bool, String)>,
}

impl GlobalProxySettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = AppSettings::global(cx).global_proxy.clone();
        let proxy_types = vec![
            ProxyTypeOption {
                value: ProxyType::Http,
                label: "HTTP".into(),
            },
            ProxyTypeOption {
                value: ProxyType::Https,
                label: "HTTPS".into(),
            },
            ProxyTypeOption {
                value: ProxyType::Socks5,
                label: "SOCKS5".into(),
            },
        ];
        let selected_index = match current.proxy_type {
            ProxyType::Http => 0,
            ProxyType::Https => 1,
            ProxyType::Socks5 => 2,
        };
        let proxy_type_select = cx.new(|cx| {
            SelectState::new(
                proxy_types,
                Some(IndexPath::new(selected_index)),
                window,
                cx,
            )
        });
        let host_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("127.0.0.1");
            if !current.host.is_empty() {
                state.set_value(current.host.clone(), window, cx);
            }
            state
        });
        let port_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("1080");
            state.set_value(current.port.to_string(), window, cx);
            state
        });
        let username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("Settings.General.Proxy.username_placeholder"));
            if !current.username.is_empty() {
                state.set_value(current.username.clone(), window, cx);
            }
            state
        });
        let password_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("Settings.General.Proxy.password_placeholder"));
            if !current.password.is_empty() {
                state.set_value(current.password.clone(), window, cx);
            }
            state
        });

        Self {
            focus_handle: cx.focus_handle(),
            enabled: current.enabled,
            proxy_type_select,
            host_input,
            port_input,
            username_input,
            password_input,
            testing: false,
            status_message: None,
        }
    }

    fn build_proxy_settings(&self, cx: &App) -> GlobalProxySettings {
        GlobalProxySettings {
            enabled: self.enabled,
            proxy_type: self
                .proxy_type_select
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or_default(),
            host: self
                .host_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string(),
            port: self
                .port_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .parse::<u16>()
                .unwrap_or(0),
            username: self
                .username_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string(),
            password: self.password_input.read(cx).text().to_string(),
        }
    }

    fn render_form_row(
        &self,
        label: String,
        child: impl IntoElement,
        disabled: bool,
        cx: &App,
    ) -> gpui::AnyElement {
        h_flex()
            .gap_3()
            .items_center()
            .child(
                div()
                    .w(gpui::px(120.0))
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .child(child)
                    .when(disabled, |this| this.opacity(0.55)),
            )
            .into_any_element()
    }

    fn on_test(&mut self, cx: &mut Context<Self>) {
        if self.testing || !self.enabled {
            return;
        }

        let proxy_settings = self.build_proxy_settings(cx);
        let client = match build_app_http_client(&proxy_settings) {
            Ok(client) => client,
            Err(err) => {
                self.status_message = Some((false, err));
                cx.notify();
                return;
            }
        };

        self.testing = true;
        self.status_message = None;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let test_task = Tokio::spawn(cx, async move {
                let http_client: Arc<dyn gpui::http_client::HttpClient> = client;
                test_proxy_connectivity(http_client).await
            });

            let result = match test_task.await {
                Ok(result) => result,
                Err(err) => Err(format!("代理测试任务执行失败: {}", err)),
            };

            let _ = this.update(cx, |view, cx| {
                view.testing = false;
                view.status_message = Some(match result {
                    Ok(()) => (true, t!("Settings.General.Proxy.test_success").to_string()),
                    Err(err) => (false, err),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn on_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }

        let proxy_settings = self.build_proxy_settings(cx);
        let new_client = match build_app_http_client(&proxy_settings) {
            Ok(client) => client,
            Err(err) => {
                self.status_message = Some((false, err));
                cx.notify();
                return;
            }
        };

        let proxy_settings_for_apply = proxy_settings.clone();
        let new_client_for_apply = new_client.clone();
        cx.defer(move |cx| {
            let settings = AppSettings::global_mut(cx);
            settings.global_proxy = proxy_settings_for_apply;
            settings.save();
            apply_global_http_client(new_client_for_apply, cx);
        });

        window.push_notification(t!("Settings.General.Proxy.save_success").to_string(), cx);
        window.remove_window();
    }

    fn on_cancel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        window.remove_window();
    }
}

impl Focusable for GlobalProxySettingsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GlobalProxySettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.enabled;

        v_flex()
            .size_full()
            .rounded(cx.theme().radius_lg)
            .bg(cx.theme().background)
            .child(
                TitleBar::new().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_1()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(t!("Settings.General.Proxy.dialog_title").to_string()),
                ),
            )
            .child(
                div().flex_1().min_h_0().overflow_y_scrollbar().p_4().child(
                    v_flex()
                        .gap_4()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("Settings.General.Proxy.dialog_desc").to_string()),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(t!("Settings.General.Proxy.enable").to_string()),
                                )
                                .child(
                                    Switch::new("global-proxy-enabled")
                                        .checked(self.enabled)
                                        .on_click(cx.listener(|view, checked, _, cx| {
                                            view.enabled = *checked;
                                            view.status_message = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.type").to_string(),
                            Select::new(&self.proxy_type_select).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.host").to_string(),
                            Input::new(&self.host_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.port").to_string(),
                            Input::new(&self.port_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.username").to_string(),
                            Input::new(&self.username_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(
                            self.render_form_row(
                                t!("Settings.General.Proxy.password").to_string(),
                                Input::new(&self.password_input)
                                    .mask_toggle()
                                    .disabled(disabled),
                                disabled,
                                cx,
                            ),
                        )
                        .when_some(self.status_message.clone(), |this, (success, message)| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(if success {
                                        cx.theme().muted_foreground
                                    } else {
                                        cx.theme().danger
                                    })
                                    .child(message),
                            )
                        }),
                ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    .p_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("proxy-test")
                            .small()
                            .label(if self.testing {
                                t!("Settings.General.Proxy.testing").to_string()
                            } else {
                                t!("Settings.General.Proxy.test").to_string()
                            })
                            .disabled(self.testing || !self.enabled)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.on_test(cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-cancel")
                            .small()
                            .label(t!("Common.cancel").to_string())
                            .disabled(self.testing)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.on_cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-save")
                            .small()
                            .primary()
                            .label(t!("Common.save").to_string())
                            .disabled(self.testing)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.on_save(window, cx);
                            })),
                    ),
            )
    }
}

fn show_global_proxy_settings_window(window: &mut Window, cx: &mut App) {
    open_popup_window(
        window,
        PopupWindowOptions::new(t!("Settings.General.Proxy.dialog_title").to_string())
            .size(560.0, 460.0),
        move |window, cx| cx.new(|cx| GlobalProxySettingsView::new(window, cx)),
        cx,
    );
}

fn apply_global_http_client(http_client: Arc<ReqwestClient>, cx: &mut App) {
    let auth_service = get_auth_service(cx);
    let http_for_auth: Arc<dyn gpui::http_client::HttpClient> = http_client.clone();
    auth_service.replace_http_client(http_for_auth, AppSettings::global(cx));

    if let Some(provider_state) = cx.try_global::<GlobalProviderState>() {
        provider_state.set_cloud_client(auth_service.cloud_client());
        provider_state.manager().clear_cache();
    }

    cx.set_http_client(http_client);
}

async fn test_proxy_connectivity(
    http_client: Arc<dyn gpui::http_client::HttpClient>,
) -> Result<(), String> {
    let request = Request::builder()
        .method(Method::HEAD)
        .uri("https://www.gstatic.com/generate_204")
        .header("User-Agent", "onetcli-updater")
        .body(AsyncBody::empty())
        .map_err(|err| format!("构建代理测试请求失败: {}", err))?;

    let response = http_client
        .send(request)
        .await
        .map_err(|err| format!("代理连接测试失败: {}", err))?;

    if !response.status().is_success() {
        return Err(format!("代理测试返回异常状态码: {}", response.status()));
    }

    Ok(())
}
/// 同步认证表单状态（独立 Entity，通过 lazy init 创建）
struct SyncAuthForm {
    email_input: Entity<InputState>,
    password_input: Entity<InputState>,
    confirm_password_input: Entity<InputState>,
    error: Entity<Option<String>>,
    is_sign_up: bool,
    is_submitting: bool,
}

struct GlobalSyncAuthForm(Entity<SyncAuthForm>);
impl gpui::Global for GlobalSyncAuthForm {}

impl SyncAuthForm {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            email_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(t!("Auth.email_placeholder"))),
            password_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(t!("Auth.password_placeholder"))
                    .masked(true)
            }),
            confirm_password_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(t!("Auth.confirm_password_placeholder"))
            }),
            error: cx.new(|_| None),
            is_sign_up: false,
            is_submitting: false,
        }
    }

    fn init(window: &mut Window, cx: &mut App) -> Entity<Self> {
        if !cx.has_global::<GlobalSyncAuthForm>() {
            let form = cx.new(|cx| Self::new(window, cx));
            cx.set_global(GlobalSyncAuthForm(form));
        }
        cx.global::<GlobalSyncAuthForm>().0.clone()
    }

    fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalSyncAuthForm>().map(|g| g.0.clone())
    }
}

/// 在 SyncAuthForm 中执行登录/注册认证
fn auth_submit(cx: &mut App) {
    let Some(form) = SyncAuthForm::global(cx) else {
        return;
    };
    let state = form.read(cx);
    let email = state.email_input.read(cx).text().to_string();
    let password = state.password_input.read(cx).text().to_string();
    let confirm_password = state.confirm_password_input.read(cx).text().to_string();
    let is_sign_up = state.is_sign_up;
    let _ = state;

    if email.is_empty() {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.email_required").to_string());
                cx.notify();
            });
        });
        return;
    }
    if password.is_empty() {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.password_required").to_string());
                cx.notify();
            });
        });
        return;
    }
    if is_sign_up && password != confirm_password {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.password_mismatch").to_string());
                cx.notify();
            });
        });
        return;
    }

    form.update(cx, |this, cx| {
        this.is_submitting = true;
        this.error.update(cx, |v, cx| {
            *v = None;
            cx.notify();
        });
        cx.notify();
    });

    let action = if is_sign_up {
        PasswordAuthAction::SignUp
    } else {
        PasswordAuthAction::Login
    };
    let auth = get_auth_service(cx);
    let form_weak = form.downgrade();
    let home_page = cx
        .try_global::<GlobalHomePage>()
        .map(|h| h.home_page.clone());

    cx.spawn(async move |cx| {
        let result = match action {
            PasswordAuthAction::Login => auth.login_with_password(&email, &password).await,
            PasswordAuthAction::SignUp => auth.sign_up_with_password(&email, &password).await,
        };

        let _ = form_weak.update(cx, |this, cx| {
            this.is_submitting = false;
            match &result {
                Ok(user) => {
                    GlobalCurrentUser::set_user(Some(user.clone()), cx);
                    cx.notify();
                }
                Err(error) => {
                    tracing::error!("密码登录失败: {}", error);
                    this.error.update(cx, |v, cx| {
                        *v = Some(error.clone());
                        cx.notify();
                    });
                }
            }
        });

        if let Some(home_page) = home_page
            && let Ok(user) = result
        {
            let _ = home_page.update(cx, |h, cx| {
                h.handle_auth_state_restored(user, cx);
            });
        }
    })
    .detach();
}

/// 渲染已登录用户信息（同步分组内）
fn render_logged_in_user_sync(user: &UserInfo, _cx: &mut App) -> AnyElement {
    let display_name = user.display_name();
    let secondary_identity = user
        .secondary_identity()
        .unwrap_or_else(|| user.email.clone());

    v_flex()
        .gap_3()
        .p_3()
        .rounded_md()
        .bg(sync_server_theme::surface_alt())
        .border_1()
        .border_color(sync_server_theme::border())
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::User).with_size(px(16.)))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(sync_server_theme::text())
                        .child(display_name),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(sync_server_theme::text_muted())
                .child(secondary_identity),
        )
        .child(
            h_flex().justify_end().mt_1().child(
                Button::new("sync-logout-button")
                    .label(t!("Auth.logout"))
                    .ghost()
                    .text_color(sync_server_theme::danger())
                    .on_click(move |_, _window, cx| {
                        let auth = get_auth_service(cx);
                        let home_page = cx
                            .try_global::<GlobalHomePage>()
                            .map(|h| h.home_page.clone());
                        cx.spawn(async move |cx| {
                            auth.sign_out().await;
                            cx.update(|cx| {
                                GlobalCurrentUser::set_user(None, cx);
                            });
                            if let Some(home_page) = home_page {
                                let _ = home_page.update(cx, |h, cx| {
                                    h.handle_auth_state_cleared(cx);
                                });
                            }
                        })
                        .detach();
                    }),
            ),
        )
        .into_any_element()
}

/// 渲染未登录时的登录/注册表单（同步分组内）
fn render_auth_form_sync(window: &mut Window, cx: &mut App) -> AnyElement {
    let form = SyncAuthForm::init(window, cx);

    v_flex()
        .gap_3()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(sync_server_theme::text_muted())
                .child(t!("Settings.General.Sync.account_auth").to_string()),
        )
        .child(
            v_flex()
                .gap_2()
                .child(Input::new(&form.read(cx).email_input).w_full())
                .child(
                    Input::new(&form.read(cx).password_input)
                        .w_full()
                        .mask_toggle(),
                )
                .when(form.read(cx).is_sign_up, |this| {
                    this.child(Input::new(&form.read(cx).confirm_password_input).w_full())
                }),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("auth-submit")
                        .label(if form.read(cx).is_sign_up {
                            t!("Auth.sign_up")
                        } else {
                            t!("Auth.login")
                        })
                        .w_full()
                        .on_click(move |_, _window, cx: &mut App| {
                            auth_submit(cx);
                        }),
                )
                .child(
                    Button::new("auth-switch")
                        .label(if form.read(cx).is_sign_up {
                            t!("Auth.switch_to_login")
                        } else {
                            t!("Auth.switch_to_sign_up")
                        })
                        .ghost()
                        .on_click({
                            let f = form.clone();
                            move |_, _window, cx: &mut App| {
                                f.update(cx, |this, cx| {
                                    this.is_sign_up = !this.is_sign_up;
                                    this.error.update(cx, |v, cx| {
                                        *v = None;
                                        cx.notify();
                                    });
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
        .when_some(form.read(cx).error.read(cx).clone(), |this, msg| {
            this.child(
                div()
                    .text_xs()
                    .text_color(sync_server_theme::danger())
                    .child(msg),
            )
        })
        .into_any_element()
}

// ============================================================================
// 快捷键设置页
// ============================================================================

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
fn render_shortcuts_section(cx: &App) -> gpui::AnyElement {
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

/// GitHub 开源地址
const GITHUB_URL: &str = "https://github.com/htmambo/onetcli";

/// 渲染关于页面
fn render_about_section(cx: &App) -> gpui::AnyElement {
    let version = env!("CARGO_PKG_VERSION");
    let muted = cx.theme().muted_foreground;

    let disclaimer_items: Vec<String> = (1..=5)
        .map(|i| {
            let key = format!("Settings.About.disclaimer_item_{}", i);
            let text = t!(&key).to_string();
            format!("{}. {}", i, text)
        })
        .collect();

    let data_safety_items: Vec<String> = (1..=3)
        .map(|i| {
            let key = format!("Settings.About.data_safety_item_{}", i);
            let text = t!(&key).to_string();
            format!("• {}", text)
        })
        .collect();

    v_flex()
        .gap_4()
        .p_4()
        // 版本信息
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(div().text_sm().child(format!(
                    "{}: {}",
                    t!("Settings.About.version"),
                    version
                ))),
        )
        // GitHub 开源地址
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .child(format!("{}: ", t!("Settings.About.opensource_label"))),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().link)
                        .child(GITHUB_URL),
                )
                .child(Clipboard::new("about-copy-github-url").value(GITHUB_URL))
                .child(
                    Button::new("about-open-github")
                        .icon(IconName::ExternalLink)
                        .xsmall()
                        .ghost()
                        .on_click(|_: &ClickEvent, _, cx| {
                            cx.open_url(GITHUB_URL);
                        }),
                ),
        )
        // 免责声明
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t!("Settings.About.disclaimer_title").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("Settings.About.disclaimer_status").to_string()),
                )
                .child(
                    v_flex().gap_1().pl_2().children(
                        disclaimer_items
                            .into_iter()
                            .map(|item| div().text_sm().text_color(muted).child(item)),
                    ),
                ),
        )
        // 数据与安全提示
        .child(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t!("Settings.About.data_safety_title").to_string()),
                )
                .child(
                    v_flex().gap_1().pl_2().children(
                        data_safety_items
                            .into_iter()
                            .map(|item| div().text_sm().text_color(muted).child(item)),
                    ),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod hotkey_migration_tests {
    use super::{is_legacy_ctrl_space, migrate_legacy_system_hotkey};
    use crate::setting_tab::{AppSettings, DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};

    #[test]
    fn detects_legacy_ctrl_space_case_insensitive() {
        assert!(is_legacy_ctrl_space("ctrl-space"));
        assert!(is_legacy_ctrl_space("CTRL-SPACE"));
        assert!(is_legacy_ctrl_space("  Ctrl-Space  "));
    }

    #[test]
    fn ignores_non_legacy_values() {
        assert!(!is_legacy_ctrl_space("ctrl-alt-m"));
        assert!(!is_legacy_ctrl_space("cmd-alt-m"));
        assert!(!is_legacy_ctrl_space("ctrl-shift-space"));
        assert!(!is_legacy_ctrl_space(""));
    }

    #[test]
    fn migration_rewrites_both_fields_when_legacy() {
        let mut settings = AppSettings::default();
        settings.system_hotkey_macos = "ctrl-space".to_string();
        settings.system_hotkey_other = "ctrl-space".to_string();

        let migration = migrate_legacy_system_hotkey(&mut settings);

        assert!(migration.macos_changed);
        assert!(migration.other_changed);
        assert_eq!(settings.system_hotkey_macos, DEFAULT_SYSTEM_HOTKEY_MACOS);
        assert_eq!(settings.system_hotkey_other, DEFAULT_SYSTEM_HOTKEY_OTHER);
    }

    #[test]
    fn migration_skips_user_customized_values() {
        let mut settings = AppSettings::default();
        settings.system_hotkey_macos = "cmd-shift-k".to_string();
        settings.system_hotkey_other = "alt-shift-t".to_string();

        let migration = migrate_legacy_system_hotkey(&mut settings);

        assert!(!migration.any_changed());
        assert_eq!(settings.system_hotkey_macos, "cmd-shift-k");
        assert_eq!(settings.system_hotkey_other, "alt-shift-t");
    }

    #[test]
    fn migration_marks_only_legacy_field_changed() {
        let mut settings = AppSettings::default();
        settings.system_hotkey_macos = "cmd-alt-m".to_string();
        settings.system_hotkey_other = "ctrl-space".to_string();

        let migration = migrate_legacy_system_hotkey(&mut settings);

        assert!(!migration.macos_changed);
        assert!(migration.other_changed);
        assert_eq!(settings.system_hotkey_macos, "cmd-alt-m");
        assert_eq!(settings.system_hotkey_other, DEFAULT_SYSTEM_HOTKEY_OTHER);
    }
}
