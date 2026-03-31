use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::{Arc, RwLock};

use gpui::{
    App, AppContext, AsyncApp, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, InteractiveElement, IntoElement, Keystroke, ParentElement, Pixels,
    Render, SharedString, StyleRefinement, Styled, Window, WindowAppearance, WindowBounds, div,
    point, prelude::FluentBuilder, px, size,
};
#[cfg(target_os = "linux")]
use gpui_component::linux_prefers_system_window_controls;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Size, Theme, ThemeMode,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    group_box::GroupBoxVariant,
    h_flex,
    kbd::Kbd,
    setting::{
        NumberFieldOptions, SelectIndex, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings,
    },
    v_flex,
};
use one_core::certificate_manager::CertificateManagerView;
use one_core::cloud_sync::{GlobalCloudUser, UserInfo, sync_server::SyncServerClient};
use one_core::storage::manager::get_config_dir;
use one_core::tab_container::{TabContent, TabContentEvent};
use one_core::utils::auto_save_config::AutoSaveConfig;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use terminal_view::{
    DEFAULT_LINE_HEIGHT_SCALE, MAX_LINE_HEIGHT_SCALE, MIN_LINE_HEIGHT_SCALE, TerminalTheme,
};
use tracing::{error, info};

use crate::auth::get_auth_service;
use crate::encourage::render_encourage_section;
use crate::onetcli_app::GlobalHomePage;
use crate::settings::llm_providers_view::LlmProvidersView;
use crate::sync_server_theme;

// ============================================================================
// 全局用户状态
// ============================================================================

/// 全局当前用户状态
///
/// 用于在设置面板中显示用户信息和执行登出操作。
#[derive(Clone, Default)]
pub struct GlobalCurrentUser {
    user: Arc<RwLock<Option<UserInfo>>>,
}

impl gpui::Global for GlobalCurrentUser {}

impl GlobalCurrentUser {
    /// 获取当前用户
    pub fn get_user(cx: &App) -> Option<UserInfo> {
        if let Some(state) = cx.try_global::<GlobalCurrentUser>() {
            state.user.read().ok().and_then(|u| u.clone())
        } else {
            None
        }
    }

    /// 设置当前用户
    pub fn set_user(user: Option<UserInfo>, cx: &mut App) {
        if !cx.has_global::<GlobalCurrentUser>() {
            cx.set_global(GlobalCurrentUser::default());
        }
        if let Some(state) = cx.try_global::<GlobalCurrentUser>() {
            if let Ok(mut guard) = state.user.write() {
                *guard = user.clone();
            }
        }
        GlobalCloudUser::set_user(user, cx);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsPanelPage {
    #[default]
    General,
    Certificate,
    Account,
}

impl SettingsPanelPage {
    fn select_index(self) -> SelectIndex {
        match self {
            Self::General => SelectIndex::default(),
            Self::Certificate => SelectIndex {
                page_ix: 3,
                group_ix: None,
            },
            Self::Account => SelectIndex {
                page_ix: 4,
                group_ix: None,
            },
        }
    }
}

#[derive(Clone, Default)]
struct PendingSettingsPanelPage {
    page: Arc<RwLock<Option<SettingsPanelPage>>>,
}

impl gpui::Global for PendingSettingsPanelPage {}

impl PendingSettingsPanelPage {
    fn set(page: SettingsPanelPage, cx: &mut App) {
        if !cx.has_global::<PendingSettingsPanelPage>() {
            cx.set_global(PendingSettingsPanelPage::default());
        }
        if let Some(state) = cx.try_global::<PendingSettingsPanelPage>() {
            if let Ok(mut guard) = state.page.write() {
                *guard = Some(page);
            }
        }
    }

    fn take(cx: &mut App) -> Option<SettingsPanelPage> {
        if let Some(state) = cx.try_global::<PendingSettingsPanelPage>() {
            if let Ok(mut guard) = state.page.write() {
                return guard.take();
            }
        }
        None
    }
}

// ============================================================================
// 数据库配置
// ============================================================================

/// 数据库打开方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DatabaseOpenMode {
    /// 单库模式：每个数据库单独打开一个标签页
    #[default]
    Single,
    /// 工作区模式：按工作区分组打开，同一工作区的数据库在同一标签页
    Workspace,
}

impl DatabaseOpenMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseOpenMode::Single => "single",
            DatabaseOpenMode::Workspace => "workspace",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "workspace" => DatabaseOpenMode::Workspace,
            _ => DatabaseOpenMode::Single,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListSortField {
    Name,
    CreatedAt,
    Manual,
    #[default]
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListSortOrder {
    Ascending,
    #[default]
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionListViewMode {
    #[default]
    Card,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedWindowDisplayState {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedWindowBounds {
    pub state: SavedWindowDisplayState,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn centered_bounds_in_visible_area(
    requested_size: gpui::Size<Pixels>,
    visible_bounds: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let centered_size = size(
        requested_size.width.min(visible_bounds.size.width),
        requested_size.height.min(visible_bounds.size.height),
    );
    Bounds::centered_at(visible_bounds.center(), centered_size)
}

fn centered_window_bounds_within_visible_area(
    requested_size: gpui::Size<Pixels>,
    visible_bounds: Option<Bounds<Pixels>>,
) -> WindowBounds {
    let bounds = visible_bounds
        .map(|visible_bounds| centered_bounds_in_visible_area(requested_size, visible_bounds))
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.0), px(0.0)),
            size: requested_size,
        });
    WindowBounds::Windowed(bounds)
}

impl SavedWindowBounds {
    fn from_bounds(state: SavedWindowDisplayState, bounds: Bounds<Pixels>) -> Option<Self> {
        let saved = Self {
            state,
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };

        saved.is_valid().then_some(saved)
    }

    fn from_window_bounds(window_bounds: WindowBounds) -> Option<Self> {
        match window_bounds {
            WindowBounds::Windowed(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Windowed, bounds)
            }
            WindowBounds::Maximized(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Maximized, bounds)
            }
            WindowBounds::Fullscreen(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Fullscreen, bounds)
            }
        }
    }

    fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    fn build_window_bounds(state: SavedWindowDisplayState, bounds: Bounds<Pixels>) -> WindowBounds {
        match state {
            SavedWindowDisplayState::Windowed => WindowBounds::Windowed(bounds),
            SavedWindowDisplayState::Maximized => WindowBounds::Maximized(bounds),
            SavedWindowDisplayState::Fullscreen => WindowBounds::Fullscreen(bounds),
        }
    }

    fn to_window_bounds(self) -> Option<WindowBounds> {
        if !self.is_valid() {
            return None;
        }

        let bounds = Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.width), px(self.height)),
        };

        Some(Self::build_window_bounds(self.state, bounds))
    }

    fn fit_in_visible_bounds(self, visible_bounds: Bounds<Pixels>) -> Option<WindowBounds> {
        let restored_window_bounds = self.to_window_bounds()?;
        let restored_bounds = restored_window_bounds.get_bounds();

        if restored_bounds.is_contained_within(&visible_bounds) {
            return Some(restored_window_bounds);
        }

        let centered_bounds = centered_bounds_in_visible_area(restored_bounds.size, visible_bounds);
        Some(Self::build_window_bounds(self.state, centered_bounds))
    }

    fn to_restored_window_bounds(self, cx: &App) -> Option<WindowBounds> {
        let restored_window_bounds = self.to_window_bounds()?;
        let restored_bounds = restored_window_bounds.get_bounds();

        if cx
            .displays()
            .into_iter()
            .any(|display| restored_bounds.is_contained_within(&display.visible_bounds()))
        {
            return Some(restored_window_bounds);
        }

        cx.primary_display()
            .map(|display| display.visible_bounds())
            .and_then(|visible_bounds| self.fit_in_visible_bounds(visible_bounds))
            .or(Some(restored_window_bounds))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub theme_mode: String,
    #[serde(default)]
    pub auto_switch_theme: bool,
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
    pub terminal_middle_click_paste: bool,
    #[serde(default)]
    pub terminal_sync_path_with_terminal: bool,
    #[serde(default = "default_terminal_theme")]
    pub terminal_theme: String,
    #[serde(default)]
    pub terminal_cursor_blink: bool,
    #[serde(default = "default_true")]
    pub terminal_confirm_multiline_paste: bool,
    #[serde(default = "default_true")]
    pub terminal_confirm_high_risk_command: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,
    #[serde(default)]
    pub sync_server_url: String,
    #[serde(default)]
    pub database_open_mode: DatabaseOpenMode,
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
}

fn default_font_family() -> String {
    "Arial".to_string()
}

fn default_font_size() -> f64 {
    14.0
}

fn clamp_ui_font_size(size: f64) -> f32 {
    size.clamp(8.0, 72.0) as f32
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

fn default_terminal_theme() -> String {
    "ocean".to_string()
}

fn default_true() -> bool {
    true
}

fn default_auto_save_interval() -> f64 {
    5.0
}

fn themed_setting_field<T>(field: SettingField<T>) -> SettingField<T> {
    field
        .bg(sync_server_theme::panel_alt_bg())
        .border_color(sync_server_theme::border_strong())
        .text_color(sync_server_theme::text_primary())
}

fn settings_group_content_style() -> StyleRefinement {
    sync_server_theme::surface_style().rounded(px(16.0))
}

fn settings_group_title_style() -> StyleRefinement {
    StyleRefinement::default().text_color(sync_server_theme::text_primary())
}

fn themed_setting_group(group: SettingGroup) -> SettingGroup {
    group
        .title_style(&settings_group_title_style())
        .content_style(&settings_group_content_style())
}

fn themed_setting_page(page: SettingPage) -> SettingPage {
    page.header_style(&sync_server_theme::page_header_style())
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: "zh-CN".to_string(),
            theme_mode: "light".to_string(),
            auto_switch_theme: false,
            font_family: default_font_family(),
            font_size: default_font_size(),
            terminal_font_size: default_terminal_font_size(),
            terminal_font_family: default_terminal_font_family(),
            terminal_font_ligatures: false,
            terminal_line_height_scale: default_terminal_line_height_scale(),
            terminal_auto_copy: default_true(),
            terminal_middle_click_paste: default_true(),
            terminal_sync_path_with_terminal: false,
            terminal_theme: default_terminal_theme(),
            terminal_cursor_blink: false,
            terminal_confirm_multiline_paste: default_true(),
            terminal_confirm_high_risk_command: default_true(),
            auto_update: true,
            sync_server_url: String::new(),
            database_open_mode: DatabaseOpenMode::default(),
            connection_list_sort_field: ConnectionListSortField::default(),
            connection_list_sort_order: ConnectionListSortOrder::default(),
            connection_list_view_mode: ConnectionListViewMode::default(),
            main_window_bounds: None,
            enable_sql_auto_save: true,
            sql_auto_save_interval: default_auto_save_interval(),
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

    fn set_main_window_bounds(&mut self, window_bounds: WindowBounds) -> bool {
        let Some(next_bounds) = SavedWindowBounds::from_window_bounds(window_bounds) else {
            return false;
        };

        if self.main_window_bounds == Some(next_bounds) {
            return false;
        }

        self.main_window_bounds = Some(next_bounds);
        true
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

    fn resolve_system_appearance(window: Option<&Window>, cx: &mut App) -> WindowAppearance {
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
        Theme::change(mode, window, cx);
        Self::apply_ui_font_preferences(self.font_family.clone(), self.font_size, cx);
    }

    pub fn apply(&self, cx: &mut App) {
        gpui_component::set_locale(&self.locale);
        self.apply_theme_preferences(None, cx);

        // 同步自动保存配置
        self.sync_auto_save_config(cx);
    }

    /// 同步自动保存配置到全局状态
    pub fn sync_auto_save_config(&self, cx: &mut App) {
        Self::update_auto_save_config(self.enable_sql_auto_save, self.sql_auto_save_interval, cx);
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

pub fn init_settings(cx: &mut App) {
    let settings = AppSettings::load();
    let initial_sync_server_url = settings.sync_server_url.clone();
    // 初始化自动保存配置全局状态
    cx.set_global(AutoSaveConfig::new(
        settings.enable_sql_auto_save,
        settings.sql_auto_save_interval,
    ));
    settings.apply(cx);
    cx.set_global(settings);
    let _ = get_auth_service(cx).update_sync_server_url(&initial_sync_server_url);
}

fn sync_terminal_settings_to_all(settings: AppSettings, cx: &mut App) {
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

fn normalize_sync_server_url(value: &str) -> String {
    SyncServerClient::normalize_base_url(value)
}

fn apply_sync_server_url_setting(value: SharedString, cx: &mut App) {
    let normalized = normalize_sync_server_url(value.as_ref());
    let settings_changed = {
        let settings = AppSettings::global_mut(cx);
        if settings.sync_server_url == normalized {
            false
        } else {
            settings.sync_server_url = normalized.clone();
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
        Self {
            focus_handle: cx.focus_handle(),
            certificate_manager_view,
            llm_providers_view,
            size: Size::default(),
            group_variant: GroupBoxVariant::Outline,
            selected_page: PendingSettingsPanelPage::take(cx).unwrap_or_default(),
            state_version: 0,
        }
    }

    pub fn request_page(page: SettingsPanelPage, cx: &mut App) {
        PendingSettingsPanelPage::set(page, cx);
    }

    fn apply_requested_page(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = PendingSettingsPanelPage::take(cx) {
            self.selected_page = page;
            self.state_version = self.state_version.wrapping_add(1);
            cx.notify();
        }
    }

    fn setting_pages(&self, _window: &mut Window, _cx: &App) -> Vec<SettingPage> {
        let certificate_manager_view = self.certificate_manager_view.clone();
        let llm_view = self.llm_providers_view.clone();
        let default_settings = AppSettings::default();

        vec![
            themed_setting_page(SettingPage::new(t!("Settings.General.title")))
                .resettable(true)
                .default_open(true)
                .groups(vec![
                    themed_setting_group(SettingGroup::new())
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
                    themed_setting_group(SettingGroup::new())
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
                                        min: 8.0,
                                        max: 72.0,
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
                        ]),
                    themed_setting_group(SettingGroup::new())
                        .title(t!("Settings.General.Sync.group_title"))
                        .item(
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
                            .description(t!("Settings.General.Sync.server_url_desc").to_string()),
                        ),
                    themed_setting_group(SettingGroup::new())
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
                                        min: 8.0,
                                        max: 72.0,
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
                        ]),
                    themed_setting_group(SettingGroup::new())
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
                        ]),
                ]),
            // 快捷键页面
            themed_setting_page(SettingPage::new(t!("Settings.Shortcuts.title"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
                    move |_options, _window, cx| render_shortcuts_section(cx),
                )),
            ),
            themed_setting_page(SettingPage::new(t!("LlmProviders.title"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
                    move |_options, _window, _cx| llm_view.clone().into_any_element(),
                )),
            ),
            themed_setting_page(SettingPage::new(t!("CertificateManager.title"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
                    move |_options, _window, _cx| {
                        certificate_manager_view.clone().into_any_element()
                    },
                )),
            ),
            // 账户设置页
            themed_setting_page(SettingPage::new(t!("Settings.Account.title"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
                    move |_options, window, cx| render_account_section(window, cx),
                )),
            ),
            // 支持作者页面
            themed_setting_page(SettingPage::new(t!("Encourage.button_label"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
                    move |_options, _window, cx| render_encourage_section(cx),
                )),
            ),
            // 关于页面
            themed_setting_page(SettingPage::new(t!("Settings.About.title"))).group(
                themed_setting_group(SettingGroup::new()).item(SettingItem::render(
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
        AppSettings, SavedWindowBounds, SavedWindowDisplayState,
        centered_window_bounds_within_visible_area,
    };
    use gpui::{Bounds, WindowBounds, point, px, size};
    use gpui::{WindowAppearance, WindowAppearance::*};
    use gpui_component::ThemeMode;

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
            init_settings(cx);
        }
        self.apply_requested_page(cx);
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !cx.has_global::<AppSettings>() {
            init_settings(cx);
        }

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(sync_server_theme::page_bg())
            .child(
                Settings::new("main-app-settings")
                    .with_size(self.size)
                    .with_group_variant(self.group_variant)
                    .sidebar_style(&sync_server_theme::sidebar_style())
                    .header_style(&sync_server_theme::control_style())
                    .default_selected_index(self.selected_page.select_index())
                    .pages(self.setting_pages(window, cx)),
            )
    }
}

/// 渲染账户设置区域
fn render_account_section(_window: &mut Window, cx: &App) -> gpui::AnyElement {
    let render_account_row = |label: String, value: String| {
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .text_sm()
                    .text_color(sync_server_theme::text_muted())
                    .child(label),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(sync_server_theme::text_primary())
                    .child(value),
            )
    };

    let render_account_shell = |title: String,
                                subtitle: Option<String>,
                                body: gpui::AnyElement|
     -> gpui::AnyElement {
        v_flex()
                .gap_4()
                .p_4()
                .child(
                    div()
                        .w_full()
                        .rounded_xl()
                        .border_1()
                        .border_color(sync_server_theme::border())
                        .bg(sync_server_theme::panel_bg())
                        .shadow_lg()
                        .child(
                            v_flex()
                                .gap_4()
                                .p_5()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .gap_3()
                                        .child(
                                            h_flex()
                                                .items_center()
                                                .gap_3()
                                                .child(
                                                    div()
                                                        .w(px(42.))
                                                        .h(px(42.))
                                                        .rounded_xl()
                                                        .bg(sync_server_theme::accent_dim())
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(
                                                            Icon::new(IconName::Server)
                                                                .with_size(px(18.))
                                                                .text_color(
                                                                    sync_server_theme::accent(),
                                                                ),
                                                        ),
                                                )
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(
                                                                    FontWeight::SEMIBOLD,
                                                                )
                                                                .text_color(
                                                                    sync_server_theme::text_primary(),
                                                                )
                                                                .child(title),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(
                                                                    sync_server_theme::text_soft(),
                                                                )
                                                                .child(
                                                                    t!(
                                                                        "Settings.General.Sync.server_name"
                                                                    )
                                                                    .to_string(),
                                                                ),
                                                        )
                                                        .when_some(subtitle, |this, subtitle| {
                                                            this.child(
                                                                div()
                                                                    .text_sm()
                                                                    .text_color(
                                                                        sync_server_theme::text_muted(),
                                                                    )
                                                                    .child(subtitle),
                                                            )
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .rounded_full()
                                                .px_2()
                                                .py_1()
                                                .bg(sync_server_theme::accent_dim_strong())
                                                .text_xs()
                                                .text_color(sync_server_theme::accent())
                                                .child(t!("Settings.Account.title").to_string()),
                                        ),
                                )
                                .child(body),
                        ),
                )
                .into_any_element()
    };

    let user = GlobalCurrentUser::get_user(cx);

    if let Some(user) = user {
        let display_name = user.display_name();
        let secondary_identity = user
            .secondary_identity()
            .unwrap_or_else(|| user.email.clone());

        render_account_shell(
            display_name.clone(),
            Some(secondary_identity),
            v_flex()
                .gap_4()
                .child(
                    div()
                        .w_full()
                        .rounded_xl()
                        .border_1()
                        .border_color(sync_server_theme::border())
                        .bg(sync_server_theme::panel_alt_bg())
                        .child(
                            v_flex()
                                .gap_3()
                                .p_4()
                                .child(render_account_row(
                                    t!("Settings.Account.username").to_string(),
                                    display_name,
                                ))
                                .child(render_account_row(
                                    t!("Settings.Account.email").to_string(),
                                    user.email.clone(),
                                )),
                        ),
                )
                .child(
                    h_flex().gap_2().justify_end().child(
                        Button::new("logout-button")
                            .icon(IconName::Close)
                            .label(t!("Auth.logout"))
                            .with_variant(sync_server_theme::danger_button_variant(cx))
                            .on_click(move |_, _window, cx| {
                                let auth = get_auth_service(cx);
                                cx.spawn(async move |cx: &mut AsyncApp| {
                                    auth.sign_out().await;
                                    cx.update(|cx| {
                                        GlobalCurrentUser::set_user(None, cx);
                                        if let Some(home) = cx.try_global::<GlobalHomePage>() {
                                            let home_page = home.home_page.clone();
                                            home_page.update(cx, |home_page, cx| {
                                                home_page.handle_auth_state_cleared(cx);
                                            });
                                        }
                                    });
                                })
                                .detach();
                            }),
                    ),
                )
                .into_any_element(),
        )
    } else {
        render_account_shell(
            t!("Settings.Account.title").to_string(),
            None,
            div()
                .w_full()
                .rounded_xl()
                .border_1()
                .border_color(sync_server_theme::border())
                .bg(sync_server_theme::panel_alt_bg())
                .child(
                    v_flex()
                        .gap_2()
                        .p_4()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(sync_server_theme::text_primary())
                                .child(t!("Settings.Account.title").to_string()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(sync_server_theme::text_muted())
                                .child(t!("Settings.Account.not_logged_in").to_string()),
                        ),
                )
                .into_any_element(),
        )
    }
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
        key_macos: "cmd-alt-m",
        key_other: "ctrl-space",
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

/// 渲染快捷键说明页面
fn render_shortcuts_section(cx: &App) -> gpui::AnyElement {
    let is_macos = cfg!(target_os = "macos");

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
            let key_str = if is_macos {
                entry.key_macos
            } else {
                entry.key_other
            };

            let keystroke = Keystroke::parse(key_str).expect("快捷键定义非法");

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
                    .child(Kbd::new(keystroke)),
            );
        }

        group_container = group_container.child(list);
        container = container.child(group_container);
    }

    container.into_any_element()
}

/// GitHub 开源地址
const GITHUB_URL: &str = "https://github.com/feigeCode/onetcli";

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
