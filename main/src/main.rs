#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod auth;

mod app_init;
mod bootstrap;
mod connection_restore;
mod home;
mod home_tab;
pub mod new_connection;
mod onetcli_app;
mod saved_connection_picker;
mod setting_tab;
mod settings;
mod sync_server_theme;
mod update;

use crate::onetcli_app::OnetCliApp;
use db::GlobalDbState;
use gpui::*;

use gpui_component::Root;
use gpui_component_assets::Assets;

fn main() {
    if update::handle_update_command() {
        return;
    }

    if bootstrap::runtime::handle_startup_command() {
        return;
    }

    let app = Application::new()
        .with_assets(Assets)
        .with_quit_mode(QuitMode::LastWindowClosed);

    app.run(move |cx| {
        onetcli_app::init(cx);

        setting_tab::init_settings(cx);
        bootstrap::theme::init_theme_runtime(cx);
        bootstrap::window::init_global_runtime_state(cx);
        let options = bootstrap::window::main_window_options(cx);

        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                // 消除linux中可能出现的窗口直角
                window.set_blur_behind_corner_radius(
                    setting_tab::AppSettings::global(cx).window_corner_radius(),
                );
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
