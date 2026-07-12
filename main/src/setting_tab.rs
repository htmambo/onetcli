#[cfg(target_os = "linux")]
use std::process::Command;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use db_view::set_db_view_settings;
use gpui::{
    App, AppContext, Axis, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, StyleRefinement, Styled,
    PathPromptOptions, Window, WindowAppearance, div,
};
#[cfg(target_os = "linux")]
use gpui_component::linux_prefers_system_window_controls;
use gpui_component::button::Button;
use gpui_component::{
    ActiveTheme, Icon, IconName, MAX_GLASS_OPACITY, MIN_GLASS_OPACITY, Sizable, Size, Theme,
    ThemeRegistry, WindowExt, WindowsSurfaceLayer, group_box::GroupBoxVariant, h_flex,
    layered_level_surface_color,
    setting::{
        NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings,
    },
    tokens::Radius, v_flex,
};
use one_core::ai_chat::GlobalChatSettings;
use one_core::certificate_manager::CertificateManagerView;
use one_core::tab_container::{TabContent, TabContentEvent};
use one_core::utils::auto_save_config::AutoSaveConfig;
use reqwest_client::ReqwestClient;
use rust_i18n::t;
use terminal_view::{
    MAX_LINE_HEIGHT_SCALE, MAX_RECOVERY_SCROLLBACK_LINES, MIN_LINE_HEIGHT_SCALE, TerminalTheme,
};

use crate::app_init::is_valid_system_hotkey;
use crate::auth::get_auth_service;
use crate::omnihub_app::GlobalHomePage;
use crate::settings::{github_auth_dialog::GithubAuthDialog, llm_providers_view::LlmProvidersView};
use crate::sync_server_theme;
use crate::update;

mod about;
mod app_settings;
mod auth_form;
mod cloud;
mod global_user;
mod hotkey;
mod locale;
mod migrations;
mod proxy;
mod proxy_view;
mod saved_window;
mod shortcuts;
mod theme_utils;
mod types;

use about::render_about_section;
pub(crate) use app_settings::AppSettings;
use auth_form::{render_auth_form_sync, render_logged_in_user_sync};
pub(crate) use cloud::{GistSettings, GoogleDriveSettings, OneDriveSettings, WebDavSettings};
pub(crate) use global_user::GlobalCurrentUser;
use global_user::PendingSettingsPanelPage;
pub(crate) use hotkey::{DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};
pub(crate) use migrations::HotkeyMigration;
pub(crate) use proxy::GlobalProxySettings;
// `ProxyType` 仅供同模块测试用，加 `#[allow]` 规避非测试构建下的
// `unused_imports` 告警（轮 9d 抽取 proxy_view 后主代码不再直接引用）。
#[allow(unused_imports)]
pub(crate) use proxy::ProxyType;
use proxy_view::render_global_proxy_settings_item;
pub(crate) use saved_window::SavedWindowBounds;
// `SavedWindowDisplayState` 仅供同模块测试用，加 `#[allow]` 规避非测试构建下的
// `unused_imports` 告警。
#[allow(unused_imports)]
pub(crate) use saved_window::SavedWindowDisplayState;
use shortcuts::render_shortcuts_section;
pub(crate) use types::{
    ConnectionListSortField, ConnectionListSortOrder, ConnectionListViewMode, DatabaseOpenMode,
    LargeTextCellEditorOpenMode, SettingsPanelPage,
};

// ============================================================================
// 设置面板页面
// ============================================================================

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


fn monospace_font_options() -> Vec<(SharedString, SharedString)> {
    [
        "Menlo",
        "Consolas",
        "JetBrains Mono",
        "Fira Code",
        "Cascadia Mono",
        "DejaVu Sans Mono",
        "Source Code Pro",
        "Noto Sans Mono CJK SC",
        "Source Han Mono SC",
        "Microsoft YaHei",
        "PingFang SC",
        "Courier New",
    ]
    .into_iter()
    .map(|font| (font.into(), font.into()))
    .collect()
}

fn mono_font_options_with_custom(custom_paths: &[String]) -> Vec<(SharedString, SharedString)> {
    let mut options = monospace_font_options();
    for path in custom_paths {
        // 必须拥有字符串，避免 SharedString 从临时 &str 泄漏借用。
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        let label: SharedString = name.into();
        if !options.iter().any(|(value, _)| value == &label) {
            options.push((label.clone(), label));
        }
    }
    options
}

const FONT_FILE_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

fn is_supported_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            FONT_FILE_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

fn load_custom_font_path(path: &Path, cx: &mut App) -> Result<(), String> {
    if !is_supported_font_file(path) {
        return Err(t!("Settings.General.Font.unsupported_font_file").to_string());
    }
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    cx.text_system()
        .add_fonts(vec![Cow::Owned(bytes)])
        .map_err(|err| err.to_string())
}

fn load_custom_fonts(paths: &[String], cx: &mut App) -> usize {
    paths
        .iter()
        .filter(|path| load_custom_font_path(Path::new(path), cx).is_ok())
        .count()
}

fn import_custom_font_paths(paths: Vec<PathBuf>, cx: &mut App) -> String {
    // 先加载字体，再写入设置，避免 AppSettings 可变借用与 text_system 冲突。
    let mut loaded_paths = Vec::new();
    for path in paths {
        if load_custom_font_path(&path, cx).is_err() {
            continue;
        }
        loaded_paths.push(path.to_string_lossy().to_string());
    }

    if loaded_paths.is_empty() {
        return t!("Settings.General.Font.custom_fonts_import_empty").to_string();
    }

    let loaded = loaded_paths.len();
    {
        let settings = AppSettings::global_mut(cx);
        for path in loaded_paths {
            if !settings
                .custom_font_paths
                .iter()
                .any(|existing| existing == &path)
            {
                settings.custom_font_paths.push(path);
            }
        }
        settings.save();
    }
    let settings = AppSettings::global(cx).clone();
    settings.sync_db_view_settings(cx);
    t!(
        "Settings.General.Font.custom_fonts_import_success",
        count = loaded
    )
    .to_string()
}

pub fn init_settings(cx: &mut App) -> HotkeyMigration {
    init_settings_with(cx, None)
}

/// 等价 `init_settings`；允许调用方复用已加载的 `AppSettings` 以避免重复文件 I/O + JSON 解析。
/// 当 `preloaded` 为 `None` 时行为与 `init_settings` 完全一致。
pub fn init_settings_with(cx: &mut App, preloaded: Option<AppSettings>) -> HotkeyMigration {
    let mut settings = preloaded.unwrap_or_else(AppSettings::load);
    migrations::migrate_legacy_theme_state(&mut settings);
    let hotkey_migration = migrations::migrate_legacy_system_hotkey(&mut settings);
    let initial_sync_server_url = settings.sync_server_url.clone();
    terminal_view::init_settings(cx, Some(migrations::legacy_terminal_settings(&settings)));
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
    let custom_font_paths = AppSettings::global(cx).custom_font_paths.clone();
    load_custom_fonts(&custom_font_paths, cx);
    if hotkey_migration.any_changed() {
        AppSettings::save_global(cx);
    }
    let _ = get_auth_service(cx).update_sync_server_url(&initial_sync_server_url);
    hotkey_migration
}

pub(crate) fn build_app_http_client(
    proxy: &GlobalProxySettings,
) -> Result<Arc<ReqwestClient>, String> {
    let proxy_url = proxy.to_proxy_url()?;
    ReqwestClient::proxy_and_user_agent(proxy_url, "omnihub")
        .map(Arc::new)
        .map_err(|err| format!("HTTP 客户端初始化失败: {}", err))
}

fn apply_sync_server_url_setting(value: SharedString, cx: &mut App) {
    let editable = migrations::editable_sync_server_url(value.as_ref());
    let normalized = migrations::normalize_sync_server_url(&editable);
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
                                            locale::LOCALE_SYSTEM.into(),
                                            t!("Settings.General.Language.system").into(),
                                        ),
                                        (
                                            locale::LOCALE_ZH_CN.into(),
                                            t!("Settings.General.Language.zh_cn").into(),
                                        ),
                                        (
                                            locale::LOCALE_ZH_HK.into(),
                                            t!("Settings.General.Language.zh_hk").into(),
                                        ),
                                        (
                                            locale::LOCALE_EN.into(),
                                            t!("Settings.General.Language.en").into(),
                                        ),
                                    ],
                                    |cx: &App| {
                                        SharedString::from(AppSettings::global(cx).locale.clone())
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        let settings = AppSettings::global_mut(cx);
                                        settings.locale = val.to_string();
                                        gpui_component::set_locale(
                                            locale::effective_locale_for_setting(&settings.locale),
                                        );
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
                                            settings.ui_surface_opacity = theme_utils::clamp_ui_surface_opacity(val);
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
                                            settings.backdrop_opacity = theme_utils::clamp_backdrop_opacity(val);
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

                            SettingItem::new(
                                t!("Settings.General.Font.sql_editor_font_family"),
                                themed_setting_field(SettingField::dropdown(
                                    mono_font_options_with_custom(
                                        &AppSettings::global(cx).custom_font_paths,
                                    ),
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx).sql_editor_font_family.clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.sql_editor_font_family = val.to_string();
                                            settings.save();
                                        }
                                        let settings = AppSettings::global(cx).clone();
                                        settings.sync_db_view_settings(cx);
                                    },
                                ))
                                .default_value(SharedString::from(
                                    default_settings.sql_editor_font_family.clone(),
                                )),
                            )
                            .description(
                                t!("Settings.General.Font.sql_editor_font_family_desc").to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Font.table_preview_font_family"),
                                themed_setting_field(SettingField::dropdown(
                                    mono_font_options_with_custom(
                                        &AppSettings::global(cx).custom_font_paths,
                                    ),
                                    |cx: &App| {
                                        SharedString::from(
                                            AppSettings::global(cx)
                                                .table_preview_font_family
                                                .clone(),
                                        )
                                    },
                                    |val: SharedString, cx: &mut App| {
                                        {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.table_preview_font_family = val.to_string();
                                            settings.save();
                                        }
                                        let settings = AppSettings::global(cx).clone();
                                        settings.sync_db_view_settings(cx);
                                    },
                                ))
                                .default_value(SharedString::from(
                                    default_settings.table_preview_font_family.clone(),
                                )),
                            )
                            .description(
                                t!("Settings.General.Font.table_preview_font_family_desc")
                                    .to_string(),
                            ),
                            SettingItem::new(
                                t!("Settings.General.Font.custom_fonts"),
                                SettingField::render(|options, _window, _cx| {
                                    Button::new("import-custom-fonts")
                                        .label(
                                            t!("Settings.General.Font.import_custom_fonts")
                                                .to_string(),
                                        )
                                        .with_size(options.size)
                                        .on_click(|_, window, cx| {
                                            let future = cx.prompt_for_paths(PathPromptOptions {
                                                files: true,
                                                directories: false,
                                                multiple: true,
                                                prompt: Some(
                                                    t!("Settings.General.Font.select_font_files")
                                                        .to_string()
                                                        .into(),
                                                ),
                                            });
                                            window
                                                .spawn(cx, async move |cx| {
                                                    if let Ok(Ok(Some(paths))) = future.await {
                                                        let _ = cx.update(|window, cx| {
                                                            let message =
                                                                import_custom_font_paths(paths, cx);
                                                            window.push_notification(message, cx);
                                                        });
                                                    }
                                                })
                                                .detach();
                                        })
                                }),
                            )
                            .description(
                                t!("Settings.General.Font.custom_fonts_desc").to_string(),
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings_snapshot, cx);
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
                                        migrations::sync_terminal_settings_to_all(settings.clone(), cx);
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
                                t!("Settings.General.Database.sql_query_max_rows"),
                                themed_setting_field(SettingField::number_input(
                                    NumberFieldOptions {
                                        min: 0.0,
                                        max: 1_000_000.0,
                                        step: 100.0,
                                        ..Default::default()
                                    },
                                    |cx: &App| AppSettings::global(cx).sql_query_max_rows as f64,
                                    |val: f64, cx: &mut App| {
                                        {
                                            let settings = AppSettings::global_mut(cx);
                                            settings.sql_query_max_rows = val.max(0.0) as u32;
                                            settings.save();
                                        }
                                        let settings = AppSettings::global(cx).clone();
                                        settings.sync_db_view_settings(cx);
                                    },
                                ))
                                .default_value(default_settings.sql_query_max_rows as f64),
                            )
                            .description(
                                t!("Settings.General.Database.sql_query_max_rows_desc").to_string(),
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
    use super::saved_window::centered_window_bounds_within_visible_area;
    use super::theme_utils::clamp_ui_surface_opacity;
    use super::{
        AppSettings, GlobalProxySettings, ProxyType, SavedWindowBounds, SavedWindowDisplayState,
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

        assert_eq!(
            super::migrations::editable_sync_server_url(value),
            "https://example.com/api/"
        );
        assert_eq!(
            super::migrations::normalize_sync_server_url(value),
            "https://example.com/api"
        );
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
        let legacy = super::migrations::legacy_terminal_settings(&settings);

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

// ============================================================================
// 快捷键设置页
// ============================================================================

/// GitHub 开源地址

#[cfg(test)]
mod hotkey_migration_tests {
    use crate::setting_tab::{AppSettings, DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};

    #[test]
    fn detects_legacy_ctrl_space_case_insensitive() {
        assert!(super::migrations::is_legacy_ctrl_space("ctrl-space"));
        assert!(super::migrations::is_legacy_ctrl_space("CTRL-SPACE"));
        assert!(super::migrations::is_legacy_ctrl_space("  Ctrl-Space  "));
    }

    #[test]
    fn ignores_non_legacy_values() {
        assert!(!super::migrations::is_legacy_ctrl_space("ctrl-alt-m"));
        assert!(!super::migrations::is_legacy_ctrl_space("cmd-alt-m"));
        assert!(!super::migrations::is_legacy_ctrl_space("ctrl-shift-space"));
        assert!(!super::migrations::is_legacy_ctrl_space(""));
    }

    #[test]
    fn migration_rewrites_both_fields_when_legacy() {
        let mut settings = AppSettings::default();
        settings.system_hotkey_macos = "ctrl-space".to_string();
        settings.system_hotkey_other = "ctrl-space".to_string();

        let migration = super::migrations::migrate_legacy_system_hotkey(&mut settings);

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

        let migration = super::migrations::migrate_legacy_system_hotkey(&mut settings);

        assert!(!migration.any_changed());
        assert_eq!(settings.system_hotkey_macos, "cmd-shift-k");
        assert_eq!(settings.system_hotkey_other, "alt-shift-t");
    }

    #[test]
    fn migration_marks_only_legacy_field_changed() {
        let mut settings = AppSettings::default();
        settings.system_hotkey_macos = "cmd-alt-m".to_string();
        settings.system_hotkey_other = "ctrl-space".to_string();

        let migration = super::migrations::migrate_legacy_system_hotkey(&mut settings);

        assert!(!migration.macos_changed);
        assert!(migration.other_changed);
        assert_eq!(settings.system_hotkey_macos, "cmd-alt-m");
        assert_eq!(settings.system_hotkey_other, DEFAULT_SYSTEM_HOTKEY_OTHER);
    }
}
