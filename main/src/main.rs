#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod auth;

mod app_init;
mod connection_restore;
mod home;
mod home_tab;
mod onetcli_app;
mod saved_connection_picker;
mod setting_tab;
mod settings;
mod sync_server_theme;
mod update;

use crate::onetcli_app::OnetCliApp;
use crate::setting_tab::AppSettings;
use db::GlobalDbState;
use db_view::database_view_plugin::DatabaseViewPluginRegistry;
use gpui::*;

use gpui_component::Root;
use gpui_component_assets::Assets;

fn main() {
    if update::handle_update_command() {
        return;
    }

    #[cfg(unix)]
    if std::env::args().any(|arg| arg == "--local-pty-host") {
        let rt = tokio::runtime::Runtime::new().expect("创建 Tokio runtime 失败");
        if let Err(e) = rt.block_on(terminal::run_local_pty_host()) {
            eprintln!("local-pty-host 启动失败: {e}");
            std::process::exit(1);
        }
        return;
    }

    let app = Application::new()
        .with_assets(Assets)
        .with_quit_mode(QuitMode::LastWindowClosed);

    app.run(move |cx| {
        onetcli_app::init(cx);

        setting_tab::init_settings(cx);

        // 首次启动时从绑定到二进制的主题包复制主题文件到用户目录
        if let Err(err) = one_core::storage::manager::ensure_themes_copied() {
            tracing::warn!("Failed to copy bundled themes: {}", err);
        }

        // 开发态直接观察工作区 themes/，安装态观察用户主题目录。
        let themes_dir = one_core::storage::manager::get_runtime_themes_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("./themes"));
        if let Err(err) = gpui_component::ThemeRegistry::watch_dir(themes_dir, cx, |cx| {
            let settings = AppSettings::global(cx).clone();
            settings.apply_theme_preferences(None, cx);
        }) {
            tracing::error!("Failed to watch themes directory: {}", err);
        }

        let db_state = GlobalDbState::new();
        db_state.start_cleanup_task(cx);
        cx.set_global(db_state);

        db_view::init_ask_ai_notifier(cx);
        db_view::init_cell_editor_sidebar_notifier(cx);

        let view_registry = DatabaseViewPluginRegistry::new();
        cx.set_global(view_registry);
        let mut window_size = size(px(1600.0), px(1200.0));
        if let Some(display) = cx.primary_display() {
            let display_size = display.visible_bounds().size;
            window_size.width = window_size.width.min(display_size.width * 0.85);
            window_size.height = window_size.height.min(display_size.height * 0.85);
        }

        let window_bounds = AppSettings::global(cx).restored_main_window_bounds(window_size, cx);
        let options = WindowOptions {
            window_bounds: Some(window_bounds),
            #[cfg(not(target_os = "linux"))]
            titlebar: Some(gpui_component::TitleBar::title_bar_options()),
            window_min_size: Some(Size {
                width: px(640.),
                height: px(480.),
            }),
            window_background: AppSettings::global(cx).preferred_window_background(),
            #[cfg(target_os = "linux")]
            app_id: Some("onetcli".to_string()),
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            kind: WindowKind::Normal,
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                window.activate_window();
                app_init::init_window_systems(window, cx);
                update::schedule_update_check(window, cx);
                let view = cx.new(|cx| OnetCliApp::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
