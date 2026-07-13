#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

mod auth;

mod app_init;
mod bootstrap;
mod connection_restore;
mod home;
mod external_driver_display;
mod home_tab;
pub mod new_connection;
mod omnihub_app;
mod saved_connection_picker;
mod setting_tab;
mod settings;
mod sync_server_theme;
mod update;

use crate::omnihub_app::OmniHubApp;
use crate::setting_tab::HotkeyMigration;
use db::GlobalDbState;
use gpui::*;
use gpui_component::notification::Notification;
use gpui_component::{Root, WindowExt};
use gpui_component_assets::Assets;
use rust_i18n::t;

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
        let settings = omnihub_app::init(cx);

        let hotkey_migration = setting_tab::init_settings_with(cx, Some(settings));
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
                let view = cx.new(|cx| OmniHubApp::new(window, cx));
                let root = cx.new(|cx| Root::new(view, window, cx));
                maybe_show_hotkey_migration_toast(&hotkey_migration, window, cx);
                root
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}

fn maybe_show_hotkey_migration_toast(
    migration: &HotkeyMigration,
    window: &mut Window,
    cx: &mut App,
) {
    if !migration.any_changed() {
        return;
    }
    let new_key = if cfg!(target_os = "macos") {
        setting_tab::DEFAULT_SYSTEM_HOTKEY_MACOS
    } else {
        setting_tab::DEFAULT_SYSTEM_HOTKEY_OTHER
    };
    let message = t!(
        "Settings.Migrations.ctrl_space_toast",
        new_key = new_key
    );
    window.push_notification(Notification::info(message).autohide(true), cx);
}
